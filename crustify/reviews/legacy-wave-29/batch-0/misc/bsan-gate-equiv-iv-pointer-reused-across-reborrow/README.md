# Misc: BSan gate stopped on aliasing UB in four wave-1 equivalence tests

- Files: `crustify/rust/ffmpeg/tests/equiv/{aes,blowfish,camellia,cast5}.rs`,
  test `buffer_sequences_match_c` in each
- Affected commit: `72aa992e7f` (crypto wave-1 batch 0)
- Kind: aliasing UB in test code (the raw C reference side), found by the
  sub-campaign's BSan gate; not a defect of the safe API

## What the gate saw

On the wave tip `0896e41211`, before any change, the `equiv` binary stopped
at its first report (`bsan-trace.txt`); BSan has no recover mode, so every
`equiv` test after it went ungated on the tip. `lib` (752 tests,
single-threaded), `ub_safe` (642), `ub_unsafe` (95) and the seven `-sys`
binaries passed, with the options sub-campaign's known `strto*` false
positives skipped (its `skip-list.txt`).

```text
error: Undefined Behavior: write access through <25037776>(unprotected) at alloc427395[0x0] is forbidden
   --> /target/crustify/builds/bsan/src/libavutil/blowfish.c:419:17
419 | memcpy(iv, dst, 8);
    = help: the accessed tag <25037776>(unprotected) has state Frozen which forbids this child write access
help: the accessed tag <25037776>(unprotected) was created here, in the initial state Reserved
   --> .../core/src/slice/mod.rs:762:0   pub const fn as_mut_ptr(&mut self) -> *mut T {
help: ... later transitioned to Frozen due to a reborrow (acting as a foreign read access) at offsets [0x0..0x8]
   --> tests/equiv/blowfish.rs:103:21   assert_eq!(ia, ir);
stack backtrace:
0: av_blowfish_crypt
```

## Triage: true positive, in the test

The test drives the raw C reference twice per step, first out of place and
then in place, with an `iv` raw pointer taken once with `ir.as_mut_ptr()`:

```rust
let iv = if cbc { ir.as_mut_ptr() } else { core::ptr::null_mut() };
unsafe { ffi::av_blowfish_crypt(ctx, r.as_mut_ptr().add(1), src, n, iv, dec) };
assert_eq!(a, r);
assert_eq!(ia, ir);              // shared reborrow of `ir`: freezes the `iv` tag
// ...
unsafe { ffi::av_blowfish_crypt(ctx, r.as_mut_ptr().add(1), r.as_mut_ptr().add(1), n, iv, dec) };
//                                          C writes the IV through the frozen tag ^^
```

Under Tree Borrows a pointer from `as_mut_ptr` loses write permission once
`ir` is reborrowed for the assertion, so C's CBC IV update through the reused
`iv` is UB. `aes.rs`, `camellia.rs` and `cast5.rs` (same batch, same
template) reuse `iv` the same way; the gate never reached them. Each test run
alone on the tip's BSan `equiv` binary (`--exact <tu>::buffer_sequences_match_c
--test-threads=1`): `blowfish`, `camellia` and `cast5` report the same "write
access through <tag>(unprotected) ... is forbidden" and exit 1; `aes` passes,
because on this host AES-NI assembly, which BSan does not instrument, writes
the IV. All four are clean under ASan, since the bytes are live.
The in-place call also derived two pointers for the one aliased buffer with
two `r.as_mut_ptr()` calls.

## Fix

Each test now takes a fresh IV pointer, and a single `place = r.as_mut_ptr()`
for both aliased buffers, right before the in-place C call, with a comment
saying why. Also in test code, for consistency with
`../../internal/xtea-in-place-source-pointer-taken-before-exclusive-retag`
(sound under Tree Borrows but not Stacked Borrows): `equiv/xtea.rs`'s
`reference_call` derived its in-place source with `as_ptr()` before
`as_mut_ptr()`, and `equiv/des.rs`'s in-place reference passed
`as_mut_ptr()` and `as_ptr()` of one buffer; both now pass one pointer.

The gate on the landed tree runs the whole `equiv` binary (see the batch
README).
