# AVXTEA type wrapper

Worklist: `type`, objective: `wrap`, home: `libavutil/xtea.rs`.

## Contract

`AVXTEA.key` is sixteen inline `uint32_t` words; only the first four
are read by the cipher and written by the endian-specific initializers.
The remaining twelve words are observable public storage and are preserved
on reinitialization. The 16-byte input key is copied, never retained.
There are no pointer fields, child allocations, refcounts, locks, or casts.

`Xtea`/`XteaRef`/`XteaMut` use `define_ctype!`. Inline values are resource-free
and copyable. `XteaBox` uses the existing `Heap` policy, with `CNew<Xtea>`
calling `av_xtea_alloc`; zero-filled allocation is paired with `av_free`.
Independent heap clones use the existing `av_memdup` policy. Safe constructors
fully initialize all sixteen words before any field getter can read them.
No `Send`/`Sync` implementation or public unsafe function was added.

Evidence: `xtea.h`, all of `xtea.c`, `mem.c`'s allocation/free implementation,
`libavutil/tests/xtea.c`, `libavformat/rtmpcrypt.c`, `tools/crypto_bench.c`, and
Wavefront's type footprint/dependency records. Ownership/lifecycle findings
were submitted through Wavefront for the type, allocator and both initializers.
Array key parameters are absent from Wavefront's extracted pointer slots;
their extent and non-retention are recorded in the initializer findings.

## Validation

Local, agent-private logs and build outputs: `/tmp/xtea-b9/`.
Campaign C libraries are reused immutably from `/target/crustify/builds/`:
C source revision `1019f8f036`, build.json version 3; configurations and
recorded compiler/instrumentation identities were checked in `ffbuild/config.mak`
(GCC 14 plain/coverage, Clang 20 ASan/UBSan/TSan, matched BSan Clang 23.1.3).
No production C or compiled shim changed; only the required bindgen allowlist
was extended, so no C rebuild or full C baseline is needed.

Checks (all passed):
- `cargo check -p libavutil-sys` and `cargo test -p libavutil-sys`.
- `cargo check --workspace`.
- `cargo clippy --workspace --all-targets -- -D warnings`.
- `cargo test --workspace`, including two lifetime/aliasing compile-fail doctests.
- ASan/UBSan and TSan: workspace library/integration tests with matching C
  variants, `cargo +nightly test -Zbuild-std --target x86_64-unknown-linux-gnu`,
  `RUSTFLAGS=-Zsanitizer=address|thread`; trybuild is run on stable, not repeated
  with build-std's differing compiler diagnostics. UBSan is set to halt on errors.
- BSan only by exact name: `xtea::initialization_and_foreign_writes_across_reborrows`,
  `xtea::heap_lifecycle_independent_clones_and_parent_drop`, and
  `xtea::arrays_and_borrowed_elements` in `--test ub_safe`, each under a timeout.
- Miri: the FFI-free inline shared/exclusive reborrow integration test and
  inline layout/value-copy unit test, each by exact name. Miri does not call C.
- `crustify scan-unsafe ... --json --sites all`: no XTEA unsafe/raw smell,
  nonconformant wrapper, or undocumented unsafe site; generated ffibox raw
  gateways are classified as seams. Existing sites outside this workset are unchanged.

Three unit, four `ub_safe`, and two equivalence tests cover inline/heap/array
storage, shared/mutable handles and reborrows, independent clones, parent-first
drop, deterministic allocation failure in an isolated child, full-array field
replacement, both initialization byte orders and preservation of unused words.
Raw owner roundtrips are internal unit tests, not `ub_unsafe` tests. There is
no new unsafe public API to test. Equivalence uses independently owned contexts
and compares every state transition; raw ECB/CBC cipher calls observe their
keys in both byte orders, including encryption/decryption and zero-block calls.
The cipher functions remain scheduled for their own symbol wrappers.

## Coverage

Run `cargo +nightly test -p ffmpeg xtea --lib --tests` with
`CRUSTIFY_FFMPEG_VARIANT=coverage`, `RUSTFLAGS='-C instrument-coverage'`,
`GCOV_PREFIX=/tmp/xtea-b9/cov/c`, `GCOV_PREFIX_STRIP=0`, and private
`LLVM_PROFILE_FILE=/tmp/xtea-b9/cov/profiles/%p-%m.profraw`.

- C `xtea.c`: 136/136 lines, 28/28 branches executed and taken, 6/6 calls.
  Both initializers and the allocator are 100%; cipher observer paths are also 100%.
- Rust `xtea.rs`: 77/77 regions, 11/11 functions, 64/64 lines (100%).
- No reachable uncovered path remains in this workset on the prepared build.
  `CONFIG_SMALL`'s alternative big-endian implementation is compiled out.
- GCOV 13 emits a version warning for the GCC 14 files (exit 3), but reads
  their counters and produces the complete per-function and annotated report.
  Raw summaries and line reports are retained under `/tmp/xtea-b9/cov/`.
