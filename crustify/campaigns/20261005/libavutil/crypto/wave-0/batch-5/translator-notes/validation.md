# Crypto wave 0 / batch 5

Route verified: `type`; objective: `wrap`. Workset: `AVMD5`, `AVMurMur3`.

## Ownership and surface

- Both public headers publish only forward declarations. The private bodies
  contain integers and inline byte/word arrays, no pointers or child resources.
  There are no public fields and no field anchors to emit.
- `av_md5_alloc` / `av_murmur3_alloc` return uniquely owned, zero-filled
  `av_mallocz` allocations or NULL. Allocation alone does not initialize the
  hashing algorithm. Policies combine allocation with the appropriate C init
  before returning an owner. Seeded Murmur3 initialization accepts every `u64`.
- `av_free` (including callers' `av_freep` form) releases the entire block.
  No disposer, cloner, refcount, locking, or stored input borrow exists.
  `hash.c` owns its erased context and releases it in `av_hash_freep`; `hmac.c`
  owns its MD5 context and frees it in `av_hmac_free`. Other observed MD5 owners
  include HTTP authentication, RTMP authentication, MD5 protocol, FLAC, H.274,
  and HEVC; they initialize/reinitialize and eventually free the same allocation.
- Only the defining TUs touch fields. Update reads/copies its input during the
  call without retaining the pointer; final writes exactly 16 output bytes.
  Finalized state remains memory-valid, with the distinct C post-final semantics
  preserved rather than silently resetting it. MD5 empty updates are a Rust
  no-op, avoiding C pointer arithmetic on a dangling empty-slice sentinel.
- ffibox supplies layout markers, shared/exclusive handles and sole owners.
  Dedicated stateless policies retain C's real allocation size: no generic
  Rust-sized allocator or zeroed-value constructor is offered for opaque types.
  Borrowing/reborrowing never forms a Rust reference over C memory. No `Send`,
  `Sync`, clone, or public unsafe API is added beyond ffibox's ownership seams.
- No existing Rust consumer used these previously scaffold-only modules.
  Unavailable higher-layer hash/HMAC parents continue using their own C APIs;
  this batch introduces no raw higher-layer dependency seam.
- Wavefront records, footprints, lifecycle schema, symbol records and public
  dependency closures were consulted. Two type findings and nine symbol
  findings were submitted successfully through `query ... --update`.

## Evidence and validation

Eight inline unit tests cover allocation failure in isolated subprocesses,
layout/handle sizes, raw ownership round-trips and borrowed seams. Four safe
integration tests cover construction through both type and policy, partial
blocks, non-aligned input, dropped/modified input, shared handles, mutable
reborrowing, repeated finalization, reset and drop without finalization. Four
independent-context equivalence tests compare chunking, seeds, resets and
interleaved update/final transitions. Five compile-fail programs prove borrowed
handles cannot escape, alias writes, survive parent destruction, or use a
Rust-sized uninitialized allocator. There is no batch-defined public unsafe API
requiring an `ub_unsafe` suite; ffibox raw seams are tested internally.

Logs and outputs are agent-unique under `/tmp/crypto-b5/`.

- Affected `libavutil-sys`: regenerated bindings, check and test passed.
- `cargo check --workspace`, `cargo clippy --workspace`, and
  `cargo test --workspace`: initially passed, including compile-fail snapshots
  and doctests. Post-rebase diagnostic-only baseline failures are noted below.
- ASan/UBSan: workspace runtime tests passed against the matching `asan-ubsan`
  prebuild, with `-Zsanitizer=address`, `-Zbuild-std`, leak detection and
  `UBSAN_OPTIONS=halt_on_error=1:print_stacktrace=1`.
- TSan: workspace runtime tests passed against the `tsan` prebuild, with
  `-Zsanitizer=thread`, `-Zbuild-std`, and `TSAN_OPTIONS=halt_on_error=1`.
- BSan: only `md5::lifecycle_and_foreign_writes` and
  `murmur3::lifecycle_and_foreign_writes`, each by exact test name, passed with
  `CRUSTIFY_FFMPEG_VARIANT=bsan cargo bsan test`, each under a timeout. No whole
  BSan binary, suite or workspace was run.
- Miri: both Rust-only layout/handle tests passed; foreign calls are not run.
- `crustify scan-unsafe ... --json --sites all`: no unsafe/raw/reference smells
  or undocumented unsafe sites belonging to these two wrappers. Release
  adapters were eliminated in favor of the typed `CDrop` policy seam.

All C implementations are unchanged from the reusable build revision
`1019f8f036`; build.json version 3, configured compilers and instrumentation
were checked against build metadata. No compiled shim changed; only the
agent-owned allowlist changed, so no private C build or full C baseline was
necessary. Prepared builds were treated as immutable.

Initial sanitizer runs inadvertently included doctests without passing the
sanitizer to rustdoc; those invocations failed at sanitizer linking/ABI checks,
not at runtime. Corrected runtime gates use `--lib --tests`; ordinary workspace
validation includes all doctests. Coverage's initial host gcov 13 differed from
GCC 14.2's notes; GCC 14.2's gcov was extracted privately and the run repeated.

## Coverage

The batch's unit, safe-UB and equivalence tests ran with the immutable coverage
C prebuild and nightly Rust `-C instrument-coverage`. `GCOV_PREFIX` redirected
all C counters privately, and `LLVM_PROFILE_FILE` redirected Rust profiles.
Non-test authored Rust execution lines: MD5 **25/25**, Murmur3 **30/30**.
C TU execution lines: MD5 **63/63**, Murmur3 **84/84**. No reachable measured
execution lines remain uncovered. The MD5 alternate CONFIG_SMALL body and
non-fast-unaligned fallback are configuration-dependent, compiled out of these
prepared builds, and not represented in the coverage denominator.

## Post-rebase baseline diagnostic drift

Revalidation after incorporating other wave landings passes check, clippy, all
workspace ASan/UBSan and TSan runtime tests, both exact-name BSan tests, Miri,
coverage and the workset safety scan. All five newly added negative programs
continue to match their expected compiler diagnostics. The complete ordinary
workspace test run now reports two **pre-existing** trybuild snapshot-format
mismatches: `intfloat_mut_handle_sent_past_value` and
`intreadwrite_view_sent_past_bytes`. Both still fail compilation correctly with
E0597; the actual diagnostic merely adds the source excerpt `F: Send + 'static`
to a standard-library note. Neither file nor its snapshot belongs to or was
changed by this batch. The mismatches are recorded rather than blessing
unrelated snapshots. Logs: `/tmp/crypto-b5/workspace-rebased.log` and
`workspace-pinned.log`. Runtime workspace revalidation excludes the shared
compile-fail harness; that harness was separately run in full, with these two
baseline-only mismatches and all workset cases passing.
