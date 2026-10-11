# Review: libavutil/crypto wave 1, batch 0 (the wave's symbols)

- Items (79): every symbol the four wave-1 translation batches scheduled,
  homed in `crustify/rust/ffmpeg/libavutil/{aes,aes_ctr,blowfish,camellia,cast5,crc,des,hash,hmac,md5,murmur3,rc4,ripemd,sha,sha512,tea,twofish,xtea}.rs`
  (plus `random_seed.rs`'s seed lock, which `av_aes_ctr_set_random_iv` takes).
  The lifecycle routines among them (`*_alloc`, `*_init`, `*_free`) were
  emitted by wave 0's type runs and reviewed there; they were re-read here
  against the cipher and hash wrappers that build on them.
- Reviewed commit: the wave tip `e71508dc3c` (batches 3, 0, 2, 1:
  `828f6bdbf7`, `5f76ee7dca`, `71c9f8836e`, `e71508dc3c`).
- Wave 1 is the crypto sub-campaign's last wave (`schedule.json` lists waves
  0-1), so this review ran the sub-campaign's BSan gate, on the tip and on the
  landed tree.
- Reports:
  - `equiv/hash-and-hmac-reject-outputs-beyond-c-ranges` (reproducer)
  - `internal/xtea-in-place-source-pointer-taken-before-exclusive-retag`
  - `wavefront/hash-and-hmac-output-sizes-not-recorded`
  - `conventions/cipher-direction-and-error-vocabularies-diverge`
  - `conventions/cast5-ecb-safety-comments-describe-cbc`
  - `misc/bsan-gate-equiv-iv-pointer-reused-across-reborrow`
  - `misc/translator-notes-in-orchestrator-record-dirs` (reported, not fixed)

## Verdicts by step

| step | finding |
|---|---|
| lifecycle | Not applicable: the worklist has symbols only. The alloc/init/free wrappers it lists match wave 0's policies (C-sized allocation, paired release, keyed constructors). |
| wavefront | All 79 records submitted and structurally right (borrowed call-scoped buffers, nullable IVs, DES/RC4 nullable sources, CRC and hash-name static returns, CTR IV borrowed from its context, the lifecycle roles). The hash and HMAC outputs carried a generic note and did not record how many bytes C writes or that the size is a caller-chosen upper bound, the gap behind the equivalence defect; notes corrected and resubmitted for 6 symbols (`wavefront/hash-and-hmac-output-sizes-not-recorded`). |
| unsafe boundary | No public `unsafe fn` and no raw pointer in a public signature in the workset; `scan-unsafe` finds no smell site and no undocumented block in the homes, before or after. Shared handles are used only where C reads the context (Blowfish, Camellia under its `SP` lock, CAST5, DES, TEA, Twofish, XTEA); AES, AES-CTR and RC4, whose C writes the context, take exclusive handles. CAST5's ECB SAFETY comments described an IV and CBC (`conventions/cast5-ecb-safety-comments-describe-cbc`). |
| ub_safe / ub_unsafe | No UB reachable through the safe API. Read against the C: block counts and `int`/`unsigned` lengths, empty-slice paths (C never offsets a dangling pointer: early returns or live stand-in bytes), in-place aliasing (C reads each block before writing it, IVs updated after the source is read), DES's defective NULL source avoided by `crypt_zeros`, `av_hash_final_b64`'s `dst[-1]` and `final_hex`'s unterminated sizes refused, the CLMUL CRC assembly's short-input copies (no over-read), Camellia's global `SP` and the random-seed fallback serialized by locks. No public unsafe API in the workset, so no `ub_unsafe` suite. One aliasing-model issue in a private helper: XTEA's in-place path derived its source pointer before the exclusive retag, unsound under Stacked Borrows only; BSan (Tree-Borrows-like) is clean, so it is filed as internal with a Miri regression. |
| equiv | Hash `finalize_bin/hex/b64` refused outputs over `INT_MAX` and HMAC `finalize`/`calculate` over `UINT_MAX` where C, offered the largest size it accepts, succeeds (`equiv/hash-and-hmac-reject-outputs-beyond-c-ranges`). Documented, deliberate divergences kept: `finalize_hex` refuses fewer than 2 bytes and `finalize_b64` an empty output without finalizing (C writes nothing / writes `dst[-1]`); data lengths beyond C's per-call types are refused (C callers split them). |
| internal | `xtea::process` pointer derivation (above). Private helpers `buffer_count`, `crypt_count`, `key_len`, `c_capacity` checked at their boundaries. |
| conventions | Three vocabularies for cipher direction and buffer errors across the wave (`aes::Direction`/`CryptError`; XTEA's duplicates; `decrypt: bool` and an invented `-22` in DES, TEA, Twofish, RC4): all now use the shared `aes` types (`conventions/cipher-direction-and-error-vocabularies-diverge`). Anchors: one `Wraps:` per wrapped symbol, duplicated only on the deliberate in-place / zero-input variants. |
| misc | The tip gate stopped on aliasing UB in the raw C side of four batch-0 equiv tests (a reused IV pointer), fixed (`misc/bsan-gate-equiv-iv-pointer-reused-across-reborrow`). The translators again committed notes under `crustify/campaigns/` (reported for the orchestrator). |

## Coverage

Details in `coverage.json`. Rust: 99.33% of the 2395 lines and 100% of the
functions in the 19 files. C: 100% of the lines of all 18 workset `.c` files;
every untaken branch is an input the wrappers refuse before C or a
CPU-feature-selected CRC path. The equivalence fix added the multi-GiB
`beyond_int_max` / `beyond_uint_max` tests; no other coverage gap reachable
through the safe API remained.

## BSan gate

Run per the sub-campaign policy: one invocation per test binary in parallel,
each under `timeout 7200`, `BSAN_OPTIONS=gc_hang_threshold_ms=600000`, `lib`
with `--test-threads=1`, skipping `compile_fail` and the 48 known `strto*`
end-pointer false positives listed in
`../../../options/wave-2/batch-0/misc/bsan-gate-strto-end-pointer-false-positives/skip-list.txt`.

- Wave tip `e71508dc3c`, before any change: `lib` 752 passed (380 s),
  `ub_safe` 642 passed (3170 s), `ub_unsafe` 95 passed (163 s), the seven
  `-sys` binaries passed; `equiv` stopped at its first report, a true
  positive in test code (`misc/bsan-gate-equiv-iv-pointer-reused-across-reborrow`).
- Landed tree (the tip plus this review; the wave branch did not move): every
  binary passed, `equiv` 591 (2328 s), `ub_safe` 642 (4248 s), `lib` 753
  (431 s), `ub_unsafe` 95 (198 s), the seven `-sys` binaries. The new
  multi-GiB equivalence regressions return early under BSan, as the base64
  one does.

## Validation at the landed tree

- `cargo check --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `RUSTDOCFLAGS=-D warnings cargo doc -p ffmpeg --no-deps`: clean.
- `cargo test --workspace` (plain, trybuild `compile_fail` included): all pass.
- ASan/UBSan and TSan (`-Zbuild-std`, `--lib --tests --skip compile_fail`):
  all binaries pass, no `runtime error:` and no sanitizer report.
- Miri: the FFI-free unit tests touched here (`buffer_count`, `crypt_count`,
  `c_capacity`, XTEA's `in_place_pointers_share_one_exclusive_reborrow`)
  pass; the XTEA regression fails with the old derivation.
- The equivalence reproducer exits 1 on the tip and 0 on the landed tree.
