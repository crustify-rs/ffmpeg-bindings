# Crypto wave 1 / batch 0

Route verified: `symbol`; objective: `wrap`. All 25 scheduled names have exactly
one filled anchor in their prescribed Rust homes. Twelve allocation, initialization
and release symbols were already completed by wave 0 and are reused, not duplicated.
Thirteen processing, IV and lookup symbols receive new safe APIs. No C body, ABI,
layout, compiled shim, lifecycle policy, or context Send/Sync assertion changes.

## Analysis and API

Read the selected public declarations, C implementations, AES dispatch/private
layout, architecture CRC headers, random-seed dependency, Wavefront records and
three-hop callees, and C consumers in MOV/CENC, SRTP, crypto, HLS, MXF, RTMP and
crypto_bench. Verified the earlier lifecycle findings. Submitted all thirteen
remaining pointer analyses through Wavefront `--update` (including no-lifecycle
verdicts); constructors continue to copy keys and owners retain their original
ffibox release policies. All processing borrows buffers synchronously, retaining
no user pointer. No buffer ownership is transferred.

- AES and CTR processing use exclusive handles because C writes inline scratch,
  counter and offset state. AES's processing direction is selected by init: the
  redundant C decrypt argument is actually ignored.
- Blowfish, Camellia and CAST5 processing read schedules without modifying them,
  despite mutable C parameter types. Methods use shared handles, with exclusive
  output/IV borrows. Camellia crypt holds the same mutex as init, excluding writes
  to the C global substitution table.
- Distinct disjoint-buffer and exact-alias in-place forms cover ECB and CBC.
  Fixed-size IV arrays prove extent. Whole-block, equal-length and C int-count
  checks run before FFI; empty processing is a no-op. Errors preserve all state.
- CTR supports arbitrary byte lengths, partial-block continuation, copied nonce
  and full-counter setters, wrapping nonce increment and random nonce generation.
  Its shared getter copies all sixteen counter bytes; snapshots survive parent
  mutation and destruction without borrowing C memory.
- Random IV generation reaches av_get_random_seed's unsynchronized static
  timing-jitter fallback. The lower-layer safe seed wrapper now shares a private
  mutex with CTR's random-IV routine. This necessary dependency fix preserves
  the C implementation; raw C callers must still exclude concurrent fallback
  access. Lock recovery is tested without calling C.
- CRC standard tables remain C-owned immutable static storage, initialized under
  C once-controls. StandardTable is a stateless shared identifier handle: lookup
  validates the ID; update reconstructs the C pointer at FFI. It never owns,
  frees, exposes or assumes a fixed table extent (x86 accelerated tables have
  only 17 words). Existing custom tables and standard handles share a typed
  internal view, not an unsafe raw-pointer helper. Short-input padding preserves
  the existing correction for generic C's `end - 3` pointer arithmetic.

Existing Rust-side raw lifecycle observers were replaced with the newly available
safe processing operations. Raw calls remain only for independent C equivalence
references, or internal lifecycle/layout fixtures. No new public unsafe API, so
no new ub_unsafe suite. All context owners/layouts still use ffibox; the CRC
identifier-backed shared view is marked Handwritten because it needs no allocation
policy or fixed-length foreign slice.

## Builds and regressions

Reused immutable `/target/crustify/builds/{plain,coverage,asan-ubsan,tsan,bsan}`.
C sources match `1019f8f036`; build.json version 3 and configure/instrumentation
records match GCC 14.2 (plain/coverage), Clang 20.1.2 (ASan/UBSan and TSan), and
matched BSan Clang 23.1.3/plugin. No reusable batch runner was supplied. Only the
six missing function allowlist entries were added; bindgen regenerated libavutil-sys
in private Cargo output directories, and its check/tests pass. No C rebuild or
wrap-only C baseline is required.

Agent-unique targets, logs, commands, sanitizer exit records and coverage reports:
`/tmp/crustify-crypto-w1-b0/`. GCOV_PREFIX and LLVM_PROFILE_FILE keep the shared
prebuilds immutable.

- Workspace check and Clippy pass, including all-target Clippy with no workset warnings.
- Required full workspace test invocation under Rust ASan and sanitized C was
  attempted. Its only failures are pre-existing nightly diagnostic snapshots
  bprint_append_with_storage_escapes and log_record_class_outlives_callback: the
  illegal programs still fail compilation, but diagnostic wording differs. Those
  unrelated fixtures are not changed. The complete workspace runtime gate with
  the diagnostic-snapshot test skipped passes.
- ASan/UBSan: Rust address instrumentation, sanitized C, leak detection and
  halt-on-error; whole workspace runtime tests pass. Both new compile-fail
  doctests reject input/output aliasing and output/IV aliasing.
- TSan: whole workspace runtime tests pass, including simultaneous Camellia
  initialization/processing, concurrent random-IV/seed calls, and standard CRC
  handles shared across threads and used after their originating thread exits.
- BSan: only the five exact new foreign-buffer/reborrow tests are run, each
  under timeout 900: aes, blowfish, camellia and cast5's
  processing_across_reborrows_and_unaligned_buffers, and aes_ctr's
  partial_stream_iv_changes_and_reborrows. All pass; no whole binary or suite
  is run under BSan.
- Miri: exact Rust-only buffer_validation, standard_handle_traits and
  seed_lock_recovers_after_unwind tests pass, without executing a foreign call.
- Static scan: no new public/private unsafe/raw smell or undocumented unsafe
  site in the affected modules. Every new unsafe block has a specific proof.

Regression commands (from the repo, with the private CARGO_TARGET_DIR appropriate
for each toolchain):

```
cargo check --manifest-path crustify/rust/Cargo.toml --workspace
cargo clippy --manifest-path crustify/rust/Cargo.toml --workspace --all-targets
CRUSTIFY_FFMPEG_VARIANT=asan-ubsan \
 RUSTFLAGS='-Zsanitizer=address -C link-arg=-lubsan' \
 RUSTDOCFLAGS='-Zsanitizer=address -C link-arg=-lubsan' \
 ASAN_OPTIONS=detect_leaks=1:halt_on_error=1 UBSAN_OPTIONS=halt_on_error=1 \
 cargo +nightly test --manifest-path crustify/rust/Cargo.toml --workspace \
 -Zbuild-std --target x86_64-unknown-linux-gnu
# Runtime gate adds --lib --tests -- --skip compile_fail.
# New compile-fail doctests use -p ffmpeg --doc crypt with the same sanitizer env.
CRUSTIFY_FFMPEG_VARIANT=tsan RUSTFLAGS=-Zsanitizer=thread TSAN_OPTIONS=halt_on_error=1 \
 cargo +nightly test --manifest-path crustify/rust/Cargo.toml --workspace --lib --tests \
 -Zbuild-std --target x86_64-unknown-linux-gnu -- --skip compile_fail
CRUSTIFY_FFMPEG_VARIANT=bsan timeout 900 cargo +bsan bsan test \
 --manifest-path crustify/rust/Cargo.toml -p ffmpeg --test ub_safe <exact-name> -- --exact
cargo +nightly miri test --manifest-path crustify/rust/Cargo.toml -p ffmpeg --lib \
 <exact-Rust-only-test> -- --exact
crustify scan-unsafe crustify/rust/ffmpeg --json --sites unsafe_fns_pub_smell \
 unsafe_fns_priv_smell raw_ptr_pub_smell raw_ptr_priv_smell ref_to_type_wrapper_smell
```

## Coverage

Campaign coverage C library and Rust `-C instrument-coverage`, analyzed with
GCC 14 gcov and the matched nightly llvm-profdata/llvm-cov. An isolated child test
forces software CPU dispatch before creating any C state, exercising reachable
AES software routines and all standard generic CRC tables in addition to the
host's accelerated dispatch. All selected C functions execute 100% of reported
blocks. Whole-TU C line coverage is AES 143/143, CTR 59/59, Blowfish 75/75,
Camellia 231/231, CAST5 113/113 and CRC 60/60. DES allocator executes 2/2 blocks;
unselected DES MAC code remains outside this workset.

Non-test Rust line coverage: AES 99/101, CTR 53/55, Blowfish 85/85,
Camellia 74/74, CAST5 105/106, CRC 69/69, DES 39/39. The five uncovered
AES/CTR/CAST5 lines are pre-existing negative initialization-status branches:
current C cannot return those errors for the lengths admitted by the safe API.
The seed dependency's entropy-failure fallback and uninitialized-random-buffer
error depend on unavailable platform entropy; random values intentionally have
no exact equivalence oracle. The random-IV test instead synchronizes its C
reference to the sampled nonce and compares subsequent reset/stream transitions.
