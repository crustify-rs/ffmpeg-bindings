# Crypto wave 0, batch 3 (wrap, type route)

## Ownership and API

- `AVDES`: public integer flag and three 16-element integer key schedules;
  no pointer fields or child resources. Inline `Des` and `DesBox` use ffibox
  handles. The existing `Heap` policy releases `av_des_alloc` storage through
  `av_free`; its `CNew<Des>` implementation binds allocation. Safe keyed
  constructors initialize every schedule, including inactive rows. Reinit
  preserves C's inactive-row semantics; invalid lengths leave state unchanged.
- `AVHMAC`: public header is opaque. Its private `hash` is one owned MD5,
  SHA or SHA512 allocation; its callbacks are static borrowed code. C copies
  or hashes keys and retains no input pointer. `HmacBox` pairs construction
  with the stateless `av_hmac_free` policy, releasing both allocations. Safe
  construction also keys the hash, since allocation alone leaves hash state
  uninitialized. No byte-copy clone, inline value, `Send` or `Sync` is offered.
- Rebased onto batch 4's `HmacType` and replaced the temporary enum dependency
  seam. `Hmac::new` accepts that safe typed value; no raw pointer or unsafe
  obligation is exposed. Existing enum and hash wrappers/tests are preserved.
- Submitted findings for both types and their five constructor/init/release
  routines using Wavefront `--update`; no ownership-store edits by hand.
- No new public unsafe API beyond ffibox's existing owner adoption seams.
  Adoption round-trips are internal unit tests; no new `ub_unsafe` suite.

## Validation

Logs and private outputs are agent-unique under `/tmp/crypto-w0-b3`.

- `cargo check --workspace` and `cargo clippy --workspace` passed; the stricter
  `cargo clippy --workspace --all-targets` also passed.
- Full `cargo test --workspace` reaches three unrelated trybuild snapshot
  mismatches. `intfloat_mut_handle_sent_past_value` and
  `intreadwrite_view_sent_past_bytes` still correctly fail with E0597;
  Rust 1.99 adds the `F: Send + 'static` source excerpt to their std note.
  The subsequently landed `hash_context_no_rust_sized_allocation` still fails
  with E0277, but batch 0's AES/AES-CTR `CAlloc` implementations extend its
  diagnostic implementor list. These tests are outside this worklist and
  their snapshots were not edited. The
  workspace runtime/doctest gate passes with `-- --skip compile_fail`; all new
  borrow-checker doctests pass separately.
- Worklist unit, `ub_safe` and equivalence suites pass against private
  ASan/UBSan and TSan C builds. Miri passes the internal Rust-only DES test
  and `ub_safe::des::rust_only_layout_projection` (no foreign calls).
- Four compile-fail doctests reject owner-outliving shared handles and
  conflicting shared/exclusive or exclusive/exclusive DES reborrows.
- BSan runs only the exact new `ub_safe` tests
  `des::inline_heap_fields_and_reborrows` and `hmac::keyed_owners_and_reborrows`;
  both pass. No suite-wide BSan invocation.
- Allocation-cap scenarios run in an isolated subprocess through safe APIs;
  they cover failure of DES allocation, HMAC parent allocation, and HMAC's
  child hash allocation, followed by successful construction after recovery.
- Static `crustify scan-unsafe` enumerates no changed public-unsafe/raw-pointer,
  wrapped-reference, or out-of-impl projection smell sites. Handwritten unsafe
  blocks all carry specific safety comments; clippy's deny gate passes.

## Build provenance and coverage

Original C sources match campaign revision `1019f8f036`; `build.json` version
3 is unchanged. Installed GCC 13.3 / Clang 18.1.3 differ from recorded GCC 14 /
Clang 20 prebuilds, so private coverage, ASan/UBSan and TSan replacements were
built from this worktree. They retain the configured shared-library, pthread,
zlib and instrumentation options, disabling programs and unrelated codec/filter
components; pixelutils is enabled in the full-workspace ASan build. The BSan
prebuild matches its unchanged explicit Clang 23.1.3-rust compiler and pass
plugin. Reusable builds were not modified. No C or compiled shim source changed,
so this wrap batch needs no full FATE baseline.

Coverage uses private GCC/gcov C libraries plus Rust `-C instrument-coverage`.
C line coverage is 100% for `av_des_alloc`, `av_des_init`, `av_hmac_alloc`,
`av_hmac_init`, and `av_hmac_free`. Both HMAC allocation-failure paths are covered.
All authored Rust functions execute. Final Rust line coverage is 100% in
`des.rs` and 98.25% in the combined `hmac.rs` (one 32-bit-host-only unit-test
line excluded on this target); combined line coverage is 99.21%. The null-input branch of `av_hmac_free`
is unreachable from the non-null owner policy. DES MAC and HMAC calc are
separately scheduled non-lifecycle symbols, not this type run's public surface.
Miri cannot execute HMAC's foreign lifecycle. Rust's 32-bit-host-only length
conversion alternative cannot execute on this 64-bit campaign target.
