# Crypto wave 1, batch 1 — symbol / wrap

## Surface and ownership

The homogeneous symbol route was verified before editing. All 25 scheduled
symbols now have filled anchors in their assigned homes. Fifteen already had
completed wrappers from the type wave; those lifecycle policies and the MD5 and
Murmur3 APIs were reused rather than duplicated. This batch adds ten distinct
symbol wrappers (with three intentional ownership/aliasing variants of DES crypt).

- DES operations only read the context's flag and key schedules, despite C's
  non-const signature, so operations live on `DesRef`. Checked slice lengths
  represent whole blocks; CBC uses `Option<&mut [u8; 8]>`. Separate disjoint,
  exact-in-place and zero-input APIs exclude partial overlap and null arithmetic.
  The actual `AV_RB64`/`AV_WB64` operations support unaligned byte buffers.
  MAC returns an independent eight-byte value; empty input yields its zero IV.
- Generic hashes borrow input transiently and exclusively mutate the context and
  its algorithm allocation. Finalization allocates its binary result using the
  runtime digest size, or uses checked caller-owned output buffers for binary,
  hex and Base64. Binary truncation/zero-padding, text truncation, unchanged tails
  and post-final state match C. No pointer is retained by any backend.
- HMAC methods use exclusive handles, checked unsigned-int lengths and independent
  Rust buffers. C copies/hashes keys and retains only its private state. A short
  final leaves state unchanged; a short calculate still keys and updates, exactly
  as C does. No runtime ownership state, handwritten owner, unsafe public method,
  or Send/Sync assertion was added. Existing ffibox owners supply release policies.
- Source declarations, implementations, backend closures, field touchers, callers
  in OMA/ASF, generic hash muxers, shared URI hashing, RTMP and SRTP, and existing
  Rust consumers were inspected. Wavefront records were queried and ownership
  findings submitted through `--update`, not by editing its store.
- Only the affected libavutil-sys allowlist was extended: DES MAC, formatted hash
  finals and HMAC calc. Bindings regenerated; sys check/test passed. No C or
  compiled shim source changed; no FATE baseline is required for this wrap batch.

## Deliberately excluded C defects

`av_des_crypt_mac` increments a NULL source and dereferences the resulting pointer
on a later block. Zero-input crypt uses real zero-filled in-place data instead.
`av_hash_final_b64` writes at `dst[size-1]` when size is zero; safe Rust rejects
empty output before any mutation. Hex final writes nothing for capacities zero
and one despite its documented NUL-termination guarantee; Rust rejects these
capacities. Unit tests verify rejection leaves buffers and hash state unchanged,
and equivalence tests retain all unaffected behavior. No defective C behavior is
copied into Rust.

## Regression evidence

Agent-unique builds, logs, scan and coverage: `/tmp/crustify-crypto-b1/`.

- `cargo check --workspace`, `cargo clippy --workspace --all-targets`, and
  `cargo test --workspace` pass. The full workspace runtime/doctest gate uses
  the private ASan/UBSan C library and matching Clang runtime link flags.
- All five worklist TU suites (unit, ub_safe, equivalence and applicable doctests)
  pass with Rust ASan / C ASan+UBSan and separately with Rust/C TSan. Leak
  detection and halt-on-UB are enabled for ASan/UBSan.
- BSan runs only the exact newly authored tests
  `des::crypt_foreign_writes_and_borrowed_inputs`,
  `hash::hashing_foreign_writes_and_reborrows`, and
  `hmac::hashing_foreign_writes_and_reborrows`, each alone; all pass.
- Miri passes the safe DES layout-projection integration test, the new Rust-only
  DES validation unit test, and HMAC's length-conversion unit test. Foreign calls
  cannot run under Miri. A new compile-fail doctest rejects overlapping DES
  slices; existing owner-lifetime/reborrow doctests remain passing.
- Static `crustify scan-unsafe --json --sites ...` reports no worklist public raw
  or unsafe smells, wrapped-reference smells, out-of-impl field projections, or
  undocumented unsafe sites. All-target Clippy confirms safety-comment discipline.
- New tests: three unit tests, six ub_safe tests, three equivalence tests and one
  compile-fail doctest. Existing equivalence tests were switched from raw calls
  on the wrapped context to its new public safe methods. No new public unsafe
  seam means no new ub_unsafe suite.

## Build provenance and coverage

C is unchanged from campaign revision `1019f8f036`; build.json version 3 is
unchanged. Installed GCC 13.3 and Clang 18.1.3 differ from the GCC 14/Clang 20
reusable builds, so private coverage, ASan/UBSan and TSan libraries were built
with the configured instrumentation/shared/pthread/zlib options. Programs and
unrelated codec/filter components were disabled, pixelutils enabled for the
workspace gate. The reusable BSan build matches its explicit Clang toolchain
and pass plugin. Reusable builds remain immutable.

The worklist unit, safe-UB and equivalence workloads together cover every
instrumented C line and every authored non-test Rust line in the five TUs:

| TU | C lines | Rust lines |
|---|---:|---:|
| des | 90/90 | 115/115 |
| hash | 106/106 | 69/69 |
| hmac | 115/115 | 70/70 |
| md5 | 63/63 | 37/37 |
| murmur3 | 84/84 | 30/30 |
| Total | 458/458 | 321/321 |

Coverage uses private GCOV_PREFIX outputs and LLVM_PROFILE_FILE profiles. Branch
inspection leaves only DES's deliberately avoided NULL-input branch, invalid
hash-algorithm switch defaults (unreachable through the constructor), and hash
freep's NULL-context branch (unreachable from a non-null owner). Multi-gigabyte
slice overflow paths are covered through the checked length helpers rather than
allocating such buffers. No nondeterministic OOM injection is needed: existing
isolated allocation-cap scenarios cover constructor and child cleanup failures.
