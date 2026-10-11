# Crypto wave 0, batch 6

Route: **type**. Objective: **wrap**. Items: `AVRC4` (`rc4.rs`) and
`AVRIPEMD` (`ripemd.rs`). No C implementation, header or shim changed.

## Ownership and surface

- `AVRC4` has only `state[256]`, `x` and `y`. `rc4.c` alone accesses the
  fields; `asfcrypt.c` allocates/frees it with `av_rc4_alloc`/`av_free` (or
  `av_freep`), and `rtmpcrypt.c` embeds it by value. No retained key/context
  pointer, suballocation, field disposer or C-specific cloner exists.
  `Rc4`/`Rc4Ref`/`Rc4Mut` use ffibox, with scalar accessors and borrowed
  `CSlice`/`CSliceMut` state views. Inline values are independently copyable;
  `Rc4Box` uses existing `Heap` release/byte-copy strategies. `CNew<Rc4>`
  calls the type's own allocator. `new` and `from_key` finish initialization
  safely; a zero-filled context is memory-safe (C truncates indices to bytes).
- An empty RC4 key causes C to read past its buffer despite satisfying the
  documented bit-alignment condition. Rust rejects it before allocation or
  mutation, together with bit counts exceeding `i32::MAX`.
  Encryption/decryption initialization is identical; the ignored flag is
  not exported as a separate Rust distinction.
- `AVRIPEMD` is opaque in its public header. Its private `transform` pointer
  is NULL during allocation, then borrows one of four immutable static
  routines; update passes only temporary state/block pointers. `hash.c`
  owns the erased context and releases it through `av_hash_freep`/`av_freep`;
  the C tests free it directly. There are no owned fields, disposers or
  observed C-specific cloners. `RipemdBox` uses a dedicated stateless release
  policy delegating to `Heap`, without a generic allocator or zeroable
  marker. Safe construction uses C's actual allocation size and installs a
  supported transform before adoption. No inline construction or byte-copy
  of the incomplete Rust binding is exposed.
- `DigestBits` admits exactly 128/160/256/320. C changes `digest_len` even
  when initialization fails, so arbitrary integers are converted/rejected
  before obtaining mutable access. No runtime lifecycle metadata is needed:
  destruction only frees storage. Public private-field/callback access is
  intentionally absent. Neither wrapped type asserts `Send` or `Sync`.
- Wavefront declarations, callers, field touchers and dependency records were
  inspected using the repository-wide config. Findings for both types and
  their four allocator/initializer symbols were submitted through `--update`.
  Existing lower-layer Rust code had no raw consumers of these types to replace.

## Regression evidence

- Six inline unit tests: integer/cardinality boundaries, Rust-only layout and
  copying, rejection without mutation, enum conversions, and one unsafe raw
  adoption round-trip per type using safely constructed inputs.
- Six `ub_safe` tests: inline/heap handles, nested shared/exclusive reborrows,
  array edges, zeroable forms, independent deep copies and early parent drop,
  allocation/clone failure in an isolated subprocess, every RIPEMD algorithm
  and reset, FFI-free projections, and concurrent thread-local construction.
  No public unsafe API beyond ffibox's sanctioned seams was added, so there
  is no new `ub_unsafe` suite.
- Four equivalence tests: complete RC4 state after each initialization, field
  edit and C stream step; inline versus boxed construction; RIPEMD digest
  checkpoints after each reset/update on independent contexts; invalid digest
  conversion errors versus C, with C's failed context remaining unpublished.
- Four trybuild cases reject dropping an owner before shared/exclusive handles
  or array views and aliasing an array reborrow. Two compile-fail doctests
  reject sending the owners across threads.

## Validation

Matching immutable prebuilds were selected from build.json v3. C/header
revision is unchanged from `1019f8f036`; the build spec matches the orchestrator
copy. Object compiler identities are GCC 14.2.0 (plain/coverage), Clang 20.1.2
(ASan/UBSan, TSan), and the BSan toolchain's Clang 23.1.3. Coverage is decoded
with matching GCC 14.2.0 gcov, privately extracted under `/tmp/crypto-b6`;
GCOV_PREFIX redirects all counters away from the shared build.

- Affected `libavutil-sys`: regenerated bindings, check and test pass.
- `cargo check --workspace` and `cargo clippy --workspace --all-targets` pass.
- Workspace runtime tests and doctests pass. Full `cargo test --workspace`
  encounters three **pre-existing** trybuild snapshot mismatches:
  `intfloat_mut_handle_sent_past_value`, `intreadwrite_view_sent_past_bytes`,
  and `hash_context_no_rust_sized_allocation`. All programs still fail
  compilation correctly. The first two diagnostics quote the installed
  standard-library source for `std::thread::spawn`; the third now lists the
  upstream AES allocation policy alongside Heap. All three were independently
  reproduced on the unmodified wave parent `83f598388c`, in the detached
  `/tmp/crypto-b6/wave-baseline` worktree
  (`wave-baseline-compile-fail.log`, exit 101). The two thread snapshots also
  reproduce on unchanged scaffold `7f98285377` at `/target`
  (`baseline-compile-fail.log`). These unrelated goldens remain unchanged.
  The four new lifetime cases and thread-safety doctests pass.
- ASan/UBSan and TSan: workspace lib/integration tests, `-Zbuild-std`, matching
  C variants, with compile_fail skipped. Leak detection, stack-use-after-return
  detection and halt-on-error enabled; no sanitizer report.
- BSan: only exact tests `rc4::inline_and_heap_reborrows` and
  `ripemd::all_digest_forms_construct_reborrow_reset_and_drop`; both pass.
- Miri: exact FFI-free layout/copy, key-length, digest-size and public projection
  tests; all four pass. Miri does not call foreign code.
- `crustify scan-unsafe --json --sites all`: no new public unsafe/raw-pointer
  smells, no unsanctioned wrapper references, and no undocumented unsafe site
  in the changed files. Trait lifecycle operations and ffibox adoption seams
  are classified as seams. No auditor was spawned.

## Coverage

`coverage.json` records authored production Rust lines and compiled C TU lines.
RC4: Rust 48/48; C 28/28. RIPEMD: Rust 35/37; C 170/170. The RIPEMD
TU includes the three covered lines of batch 10's immutable size accessor,
preserved during integration; this batch's own production lines remain 32/34.
The two unexecuted
Rust lines are defensive cleanup if a supported RIPEMD initialization
unexpectedly fails: unreachable with the matching C revision. RC4's invalid
bit-alignment branch is also unreachable through its byte-key API. The
CONFIG_SMALL alternatives are not compiled in the reusable library. Allocation
failures and all supported initializer branches are exercised. C hash/stream
operations in the equivalence probes remain reference calls, not extra safe
symbol wrappers.
