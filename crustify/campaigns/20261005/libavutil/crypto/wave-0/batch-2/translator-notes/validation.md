# Crypto wave 0, batch 2 — type wrap

## Analysis and representation

- Verified homogeneous `type` route: `AVCAST5` and `AVCRCId`.
- Read `cast5.h`, its entire implementation, C tests and `crypto_bench`
  callers; queried Wavefront records, dependency closure and lifecycle schema.
  CAST5's private body owns only `Km[17]`, `Kr[17]` and `rounds`, with no
  pointer fields. Its sole allocation is `av_mallocz`, released by `av_free`.
  Initialization copies 5–16 key bytes, retains no pointer, and selects
  12 rounds for up to 80 bits or 16 rounds otherwise. There is no published
  field, clone, refcount or field-disposal lifecycle.
- `Cast5` uses ffibox's opaque layout and shared/exclusive handles. Its
  `Cast5Free` policy deliberately offers no generic allocator: the public
  forward declaration has no usable Rust size. `Cast5::new` combines C
  allocation and initialization, so consumers never promote uninitialized
  storage. `Cast5Mut::init` rekeys exclusively and rejects invalid lengths
  before FFI. C storage and implementation remain unchanged.
- `CrcId` is a transparent resource-free enum newtype, preserving every raw
  encoding; all eight public constants are exposed, but the internal
  `AV_CRC_MAX` sentinel is not. The integer representation requires no ffibox
  owner, handles or release strategy. Threaded tests exercise its automatic
  integer-derived thread safety; CAST5 gains no Send/Sync implementation.
- CRC lookup can return either architecture-specific static data (17 words
  on x86) or ordinary tables (257/1024 words). Tests never assume one table
  length or free static storage. Both detected-CPU and isolated no-extension
  lookup paths are covered.
- Submitted type findings and CAST5 allocator/initializer pointer and
  lifecycle findings through Wavefront `--update`.

## Validation

Reusable libraries are the campaign's immutable `crustify/builds` variants
from build.json version 3. The library C sources/build files match revision
`1019f8f036`; build configuration and instrumentation were inspected. No C or
shim was changed, so a full C rebuild/FATE baseline is not required. Cargo
regenerated bindings after the four minimal allowlist additions.

Commands (logs and private outputs under `/tmp/crypto-b2`):

- `cargo check --workspace`
- `cargo clippy --workspace` and `cargo clippy --workspace --all-targets`
- `cargo test --workspace`: initially passed, including three CAST5
  compile-fail doctests. The final rerun reports two pre-existing trybuild
  snapshot mismatches (`intfloat_mut_handle_sent_past_value` and
  `intreadwrite_view_sent_past_bytes`): both correctly produce E0597, but
  rustc includes an extra standard-library `F: Send + 'static` excerpt after
  the existing note. No target diagnostic or safety behavior differs. These
  unrelated snapshots are not changed by this batch. The runtime workspace
  gate is repeated with only that compiler-snapshot test excluded; the
  workset's compile-fail doctests are run separately.
- `cargo test -p libavutil-sys`
- ASan/UBSan: matching C variant with nightly Rust `-Zsanitizer=address`,
  `-Zbuild-std`, target `x86_64-unknown-linux-gnu`; whole workspace lib and
  integration suites, skipping only the trybuild compiler test. Workset
  filters were rerun on the final tree.
- TSan: analogous matching C variant and Rust `-Zsanitizer=thread`, including
  the whole workspace lib/integration suites and final workset filters.
- BSan: only the exact test
  `cast5::keys_are_copied_and_handles_reborrow_exclusively`, under a timeout,
  with `CRUSTIFY_FFMPEG_VARIANT=bsan`; no broad BSan suite invocation.
- Miri: the two CRC safe integration tests, both CRC unit tests, and CAST5's
  input-rejection/error-formatting unit tests. Foreign context construction
  and release are covered by sanitizers, not Miri.
- `crustify scan-unsafe <workdir> --json --sites all`: no workset unsafe/raw
  smells or undocumented unsafe sites after tightening the release policy.
  The only unsafety is justified FFI/ffibox lifecycle machinery. There is no
  additional public unsafe API requiring an `ub_unsafe` suite.

## Coverage

Ran only this workset's unit, safe-UB and equivalence tests against the
coverage C prebuild and `-C instrument-coverage` Rust build. `GCOV_PREFIX`
and `LLVM_PROFILE_FILE` redirect all outputs privately; the shared prebuild
is never written. Used LLVM's matched coverage tools and privately extracted
GCC 14.2 gcov, matching the C prebuild's gcno version.

- Rust CAST5 (including inline unit facilities): 56/57 lines (98.25%), all 8 functions executed.
- Rust CRC identifiers (including inline unit facilities): 49/49 lines (100%), all 7 functions executed.
- C CAST5 TU: 96/113 lines (84.96%); allocator and valid/invalid initializer
  paths executed, both round counts observed by independently owned C
  encryption/decryption comparisons after every rekey. Uncovered lines are
  CBC/crypt2, which are later symbol work, not this type lifecycle.
- C CRC TU: 44/60 lines (73.33%). Added the isolated CPU-fallback equivalence
  test after initial coverage showed the normal table initialization path
  was missed. Lookup's range checks and every public identifier's fallback
  initialization now execute. Remaining paths include custom CRC table
  rejection and CRC calculation, scheduled as later symbols.

The sole missed Rust line is the defensive negative-C-initializer error
branch: after Rust validates a key's byte length, C's only error condition
cannot hold. Sub-byte C key lengths cannot be expressed through the safe
byte-slice API. Allocation failure is covered deterministically through an
isolated child process that sets the safe allocator limit to zero, then
restores it and verifies successful construction. No C defect or divergence
was identified.
