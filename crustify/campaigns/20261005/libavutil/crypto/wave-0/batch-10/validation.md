# Crypto wave 0, symbol batch 10

Objective: **wrap**, all 24 assigned symbols, in their assigned homes. No public
unsafe API or public raw pointers were added. `av_base64_decode` intentionally
has two anchors, for bounded decoding and validity-only checking.

## Contracts and ownership

- Every input/output buffer and integer slot is a synchronous borrow; none is
  retained, freed or transferred. MD5's temporary context stays inside C.
- All nine context-size globals are immutable `const int` objects in C static
  storage; their wrappers return `&'static i32`, not an owner or a mutable view.
- Hash registry names borrow immutable, immortal, NUL-terminated C storage;
  invalid indices are `None`.
- CRC tables use private Rust-owned, zero-initialized arrays of either 257 or
  1024 `u32` words. Successful C initialization establishes their dispatch-tag
  invariant; callers can only read the words. Cloning is an independent Rust
  copy; C never retains or writes the initialized table. Short CRC inputs have
  a live three-byte prefix for the generic C path's `end - 3` arithmetic.
- LZO's erased pointers actually address byte arrays. The wrapper privately
  supplies initialized 8-byte input and 12-byte output padding. It preserves
  C's error flags, remaining lengths, and all writes within the logical output,
  including writes beyond the reported produced prefix. Rust owns all scratch
  allocations; no foreign allocation/lifecycle policy is needed.
- Base64 separates C's null-output validation form from its bounded-output
  decoding form. Short outputs truncate, invalid input can leave partial writes,
  and `=` stops scanning. Encoding validates integer limits and output capacity
  before FFI; its returned CStr keeps the destination exclusively borrowed.
- UUIDs are plain 16-byte values. Inline copy/equality/nil use equivalent std
  operations; parsing/formatting use C. Returned parsed arrays are fully formed
  on success; partial error output is intentionally not exposed. C's skipped
  separator positions and case-insensitive URN prefix are preserved. Formatting
  takes exactly 37 bytes and returns a destination-tied CStr.
- Wavefront submissions cover all 24 records and every enumerated pointer slot.
  Its extraction does not enumerate array-typedef UUID parameters as pointer
  slots; their 16-byte borrowed input/output contracts are represented by the
  fixed-size Rust arrays and the tests instead.

Source declarations/definitions, architecture CRC helpers, LZO back-pointer
copy, MD5 update/final, and callers in libavutil/libavcodec/libavformat were
inspected. No C source or authored shim changed. Bindgen's existing static-fn
wrapping supplies the three UUID inline C references in equivalence tests.
The prebuilt libraries use build.json version 3 and unchanged C revision
1019f8f036; recorded compiler/instrumentation identities were inspected. All
instrumented generated shims and Rust outputs are private to this batch.

## Validation

Agent-unique logs/outputs: `/tmp/crustify-crypto-b10/`.

- `cargo check --workspace` and `cargo clippy --workspace` pass; an additional
  `clippy --workspace --all-targets` is clean for this changeset.
- `cargo test --workspace` passes with the prepared ASan/UBSan C libraries,
  instrumented generated shims and sanitizer link/runtime flags. The full
  compile-fail gate used `TRYBUILD=overwrite` to accommodate compiler diagnostic
  wording; unrelated pre-existing snapshot changes were not included.
- Extra nightly Rust ASan instrumentation: all 21 new UB-safe tests, all 19
  new equivalence tests, and all 5 new unit tests pass. ASan leak detection and
  UBSan halt-on-error are enabled.
- TSan: all 40 new integration tests pass, with an instrumented Rust standard
  library and matching TSan C libraries. Concurrent static borrows, shared CRC
  tables, independent streaming operations, and repeated mutable reborrows run.
- BSan: six newly authored tests pass, each by exact name only: adler32's
  `batch10_stateless_threads_and_reborrows`, base64's
  `batch10_roundtrips_truncation_and_reused_buffers`, crc's
  `batch10_tables_clone_share_and_stream_unaligned_data`, lzo's
  `batch10_hidden_padding_errors_and_reused_destinations`, md5's
  `batch10_digest_boundary_lengths`, and uuid's
  `batch10_copy_nil_equality_parse_and_reborrow`. No workspace/suite BSan run.
- Miri: the three exact Rust-only unit tests pass (base64 size checking,
  initialized LZO padding and UUID inline equivalents/range rejection).
  Foreign calls are not executed under Miri.
- Seven new compile-fail programs reject output-view mutation, output lifetime
  escape, CRC table mutation/lifetime escape, LZO input/output aliasing, UUID
  copy aliasing and an undersized UUID destination.
- `crustify scan-unsafe --json --sites all`: no smell sites in the authored
  workset; no public unsafe/raw API was introduced.

Nightly-only full-workspace doctests additionally expose three pre-existing
error-code expectations (E0599 vs the new compiler's E0308 in eval/mem/opt).
Stable full-workspace doctests pass. These unrelated docs were not changed.

## Coverage

Coverage prebuild and Rust `-C instrument-coverage` run the new tests, with
GCOV_PREFIX/LLVM_PROFILE_FILE redirected into private output directories; the
shared C builds are immutable. Generated inline C references are also gcov
instrumented in the private Cargo build.

- Selected C function bodies: all executable lines reached for adler32,
  base64, crc init/update (generic and accelerated), hash names, LZO decode
  and its helpers, MD5 sum/update/final, and UUID parsing/formatting.
- The whole C files show adler32/base64/LZO/uuid at 100%; crc 61.67%, hash 2.83%
  and md5 96.83%. Their remaining lines belong to unscheduled get-table,
  registry/context operations or the MD5 allocator, not this batch.
- Before concurrent type integrations, the symbol-batch Rust TU totals
  (unit modules included) reach 100% except base64 48/52
  and LZO 38/39 lines. Remaining paths are C's impossible post-prevalidation
  encode failure and length conversions requiring multi-gigabyte slices.
  Pure arithmetic overflow/size rejection is independently unit tested.
- LZO's signed length-accumulator guard is reached by an 8.4MB malformed input.
  A separately isolated child tests the CRC scalar fallback without changing
  another test's CPU flags.

No full C baseline rebuild is required for this bindgen/Rust-only wrap batch.
