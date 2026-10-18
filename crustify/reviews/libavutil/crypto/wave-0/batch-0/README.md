# Review: libavutil/crypto wave 0, batch 0 (the wave's types)

- Items (19): `AVAES` (`aes::Aes`, `AesBox`), `AVAESCTR` (`aes_ctr::AesCtr`,
  `AesCtrBox`), `AVBlowfish` (`blowfish::Blowfish`, fields `p`, `s`),
  `AVCAMELLIA` (`camellia::Camellia`), `AVCAST5` (`cast5::Cast5`), `AVCRCId`
  (`crc::CrcId`), `AVDES` (`des::Des`, fields `triple_des`, `round_keys`),
  `AVHMAC` (`hmac::Hmac`), `AVHMACType` (`hmac::HmacType`), `AVHashContext`
  (`hash::HashContext`), `AVMD5` (`md5::Md5`), `AVMurMur3`
  (`murmur3::Murmur3`), `AVRC4` (`rc4::Rc4`, fields `x`, `y`, `state`),
  `AVRIPEMD` (`ripemd::Ripemd`), `AVSHA` (`sha::Sha`), `AVSHA512`
  (`sha512::Sha512`), `AVTEA` (`tea::Tea`), `AVTWOFISH` (`twofish::Twofish`),
  `AVXTEA` (`xtea::Xtea`, field `key`).
- Homes: `crustify/rust/ffmpeg/libavutil/{aes,aes_ctr,blowfish,camellia,cast5,crc,des,hash,hmac,md5,murmur3,rc4,ripemd,sha,sha512,tea,twofish,xtea}.rs`.
- Reviewed commit: the wave tip `7935c403e0` (types landed by batches 0-9,
  `b76d1bb41d` .. `7935c403e0`; batch 10's symbols `c51ba553a2` share the homes),
  rebased onto the symbols review (`776e95e556`) before landing.
- Not the sub-campaign's last wave (crypto has waves 0-1), so no BSan gate:
  BSan runs only on reproducers, and this review emits none.
- Reports:
  - `lifecycle/avdes`
  - `unsafe/opaque-contexts-offer-rust-sized-allocation`
  - `unsafe/hash-context-free-raw-adapter`
  - `wavefront/av-hash-alloc-constructor-not-recorded`
  - `conventions/des-init-safety-comment-wrapped`
  - `conventions/sha-equiv-tests-manual-c-str-literals`
  - `misc/trybuild-e0277-snapshot-lists-calloc-implementors`
  - `misc/stable-1-99-thread-spawn-note-drift`
  - `misc/translator-notes-in-orchestrator-record-dirs` (reported, not fixed)

## Verdicts by step

| step | finding |
|---|---|
| wavefront | Type findings match the C for all 19 types (opaque bodies, no pointer fields except the static `crypt`/`transform`/HMAC callbacks and the borrowed CRC table; Camellia's shared `SP` rewrite; `av_crc_get_table`'s range check). Lifecycle roles of all 39 crypto alloc/init/free symbols checked: every type's `--lifecycle-ops` matches its wrappers except `AVHashContext`, whose constructor `av_hash_alloc` had no `lifetime` block. Resubmitted (`wavefront/av-hash-alloc-constructor-not-recorded`). |
| lifecycle | `AVDES` lacked its by-value copy and `av_memdup` box clone, which its resource-free siblings `AVBlowfish`, `AVRC4`, `AVXTEA` have; added (`lifecycle/avdes`). Every other type is complete: C-allocated owners with the paired release (`av_free`, `av_aes_ctr_free`, `av_hmac_free`, `av_hash_freep`), keyed safe constructors that bind alloc + init, `init`/`reset` on the exclusive handle, inline values for the public resource-free layouts, no clone where C has none and the layout is private. |
| unsafe boundary | `AVSHA`, `AVSHA512`, `AVCAMELLIA`, `AVTEA`, `AVTWOFISH` are zero-sized opaque bindings, yet their boxes used `Heap`, whose generic `CAlloc` exposed `new_uninit()` with a zero-byte block whose only continuation (`assume_init` after a C init) is a heap overflow; now dedicated storage-only policies (`unsafe/opaque-contexts-offer-rust-sized-allocation`). `HashContextFree`'s private raw-pointer `unsafe fn` adapter replaced by a hand-written `CDrop` (`unsafe/hash-context-free-raw-adapter`). The workset adds no public `unsafe fn` and no raw pointer in a public signature (the `ffi` enum newtypes in `CrcId`/`HmacType::from_raw` are values); `scan-unsafe` reports no smell site in the homes afterwards. |
| ub_safe / ub_unsafe | No UB found. Read against the C: every initializer's key reads against the wrappers' length checks (AES/Camellia 16/24/32, CAST5 5..16, DES 8/24, RC4 and Blowfish non-empty and `int`-sized, Twofish `min(bits, 256) / 8`, HMAC `unsigned`), the empty-slice paths (live byte for `memcpy`/`src + 0`), CRC's `end - 3` padding and unaligned CLMUL table loads, Camellia's `SP` lock, the zero states admitted by `CZeroable` (`Des`, `Rc4`, `Xtea`, `Blowfish`), and `Rc4`'s borrowed `state` views. No public unsafe API in the workset, so no `ub_unsafe` suite. ASan/UBSan and TSan clean on the whole crate; 30 FFI-free unit tests pass under Miri. |
| equiv | No divergence found beyond the documented, deliberate ones (inputs C would over-read or that corrupt AES's `rounds` are refused before FFI). Added `equiv/des.rs::copies_and_clones_track_c_struct_copies` (clone and copy vs a C struct copy, fields and `av_des_crypt`) and `equiv/hash.rs::allocation_failures_match_c` (`HashContext::new` vs `av_hash_alloc` under 14 `av_max_alloc` limits, in an isolated child). |
| internal | Private helpers (`key_bits`/`key_len`/`validate_key` per cipher, `lock_c_access`, `Direction::as_int`) checked; no defect. |
| conventions | `des.rs`'s wrapped `let result =\n unsafe {..}` counted as undocumented (`conventions/des-init-safety-comment-wrapped`); `clippy --all-targets -D warnings` failed on `manual_c_str_literals` in the two SHA equivalence tests (`conventions/sha-equiv-tests-manual-c-str-literals`); its `items_after_test_module` errors in six homes were fixed by the symbols review (`../batch-1/conventions/symbol-wrappers-after-unit-tests-module`). Anchors: one `Wraps:` per type, a `Field:` anchor for each of the eight field anchors (on its getter; Blowfish and XTEA repeat it on the setter), `Wraps:` on lifecycle routines and release policies. |
| misc | `ub_safe`'s `compile_fail` failed at the tip: a wave-0 E0277 snapshot broke when a later batch added a `CAlloc` impl (`misc/trybuild-e0277-snapshot-lists-calloc-implementors`, now doctests), and two core snapshots predate rustc 1.99 (`misc/stable-1-99-thread-spawn-note-drift`). The translators' notes under `crustify/campaigns/.../crypto/wave-0/batch-*/` collide with the orchestrator's record paths; reported for the orchestrator, left in place (`misc/translator-notes-in-orchestrator-record-dirs`). Hand-off for wave 1: `CrcId::from_raw` keeps any value, so `av_crc_get_table`'s wrapper must expect NULL (C range-checks); Camellia crypt wrappers must take `camellia::lock_c_access`. |

## Coverage

Details in `coverage.json`. Rust: 99.05% of the 1258 lines and 100% of
the 195 functions in the 18 homes; the 12 missed lines are the negative-
status arms of C initializers that cannot fail for the inputs the wrappers
admit, and a test-only closing brace. C: every alloc/init/free routine of
the 17 struct types, `av_hash_get_name/size`, `av_hash_names`, `av_crc` and
`av_crc_init` at 100% of lines; `av_hash_alloc` rose from 92% to 100% with
`equiv/hash.rs::allocation_failures_match_c` (its child-allocation-failure
path).

## Validation at the landed tree

On the wave branch after the symbols review (`776e95e556`) plus this review:

- `cargo check --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `RUSTDOCFLAGS=-D warnings cargo doc -p ffmpeg --no-deps`: clean (clippy
  failed at the wave tip, see the conventions report).
- plain `cargo test --workspace`: 742 lib, 595 equiv, 624 ub_safe (the
  compile-fail programs included; the tip failed 3 of them), 107 ub_unsafe,
  149 doctests (the seven new `compile_fail,E0277` cases included), 0 failed.
- ASan/UBSan and TSan (`-Zbuild-std`, the matching C variants, whole crate,
  `--skip compile_fail`): 742 / 595 / 623 / 107 each, no `runtime error:`
  and no sanitizer report; equally clean on the review tree before the
  rebase (740 / 592 / 623 / 107).
- Miri: the 30 FFI-free unit tests of the workset's types, including the
  new `des::unit_tests::inline_copies_are_independent_contexts`.
- `crustify scan-unsafe --sites`: no smell site left in the 18 homes
  (`unsafe_fns_priv_smell` and `raw_ptr_priv_smell` each down by one, the
  `av_hash_freep` adapter); undocumented unsafe sites 24 -> 23 (none left in
  `des.rs`); `unsafe_impls` +5 and `unsafe_fns_seam` +5 are the five new
  storage-only `CDrop` policies.
- BSan: not run; this is not the sub-campaign's last wave and the review
  emits no reproducer.
