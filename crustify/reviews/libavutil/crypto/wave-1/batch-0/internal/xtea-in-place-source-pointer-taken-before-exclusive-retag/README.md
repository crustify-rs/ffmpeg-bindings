# Internal: XTEA's in-place path took its source pointer before the exclusive retag

- File: `crustify/rust/ffmpeg/libavutil/xtea.rs`, private `XteaRef::process`
- Affected commit: `828f6bdbf7` ("wrap XTEA block ciphers with checked safe
  buffer APIs", crypto wave-1 batch 3)
- Public entry points reaching it: `XteaRef::crypt_in_place`,
  `XteaRef::crypt_le_in_place` (`Wraps: av_xtea_crypt` / `av_xtea_le_crypt`)
- Kind: aliasing-model-dependent pointer derivation; no sanitizer report

## Finding

For an in-place call `process` built C's two pointers from the same
`&mut [u8]` like this:

```rust
let src_ptr = src.map_or_else(|| dst.as_ptr(), <[u8]>::as_ptr); // shared reborrow
// ...
crypt(ctx, dst.as_mut_ptr(), src_ptr, count, iv, decrypt);      // exclusive reborrow
```

`dst.as_mut_ptr()` reborrows `dst` exclusively *after* `src_ptr` was derived
from a shared reborrow of it. Under Stacked Borrows (Miri's default model)
that retag invalidates `src_ptr`, so C's first read through it is UB. Under
Tree Borrows it is accepted: the retag only freezes the sibling, and C reads
every byte before it writes it (`xtea_crypt` reads a block, then writes it;
CBC's `dst[i] = src[i] ^ iv[i]` reads before it writes each byte).

Every other in-place wrapper of the wave derives one pointer and passes it
twice (`let ptr = data.as_mut_ptr(); ... ptr, ptr`).

## Evidence

- BorrowSanitizer (Tree-Borrows-like): a `#![forbid(unsafe_code)]` program
  running all four in-place combinations (BE/LE, ECB/CBC, both directions)
  on the affected commit runs clean, as does the gate's `ub_safe` binary. So
  this is not a UB finding with a sanitizer reproducer.
- Miri, Stacked Borrows: `model/` is the same derivation with C's per-byte
  "read src, write dst" done by Rust raw accesses. `cargo +nightly miri run`
  reports (`miri-stacked-borrows.txt`):

  ```text
  error: Undefined Behavior: attempting a read access using <517> at alloc201[0x0], but that tag does not exist in the borrow stack for this location
  help: <517> was created by a SharedReadOnly retag at offsets [0x0..0x10]
    --> src/main.rs:11:38   let src_ptr = src.map_or_else(|| dst.as_ptr(), <[u8]>::as_ptr);
  help: <517> was later invalidated at offsets [0x0..0x10] by a Unique function-entry retag inside this call
    --> src/main.rs:12:18   c_like_crypt(dst.as_mut_ptr(), src_ptr, dst.len());
  ```

  With `MIRIFLAGS=-Zmiri-tree-borrows` it runs to completion.

The aliasing model Rust will adopt is not settled, so the wrapper should be
sound under both.

## Fix

`process` now takes `dst.as_mut_ptr()` once and derives the in-place source
from it (`src.map_or(dst_ptr.cast_const(), <[u8]>::as_ptr)`), with a comment
saying why. No behaviour changes; the existing `equiv/xtea.rs` cipher matrix
and `ub_safe/xtea.rs` in-place tests still cover the C path.

To make the derivation testable under Miri, `process` now hands the selected
C routine to a private `process_with(crypt: CryptFn, ...)`. The inline
regression `xtea::unit_tests::in_place_pointers_share_one_exclusive_reborrow`
passes a Rust `extern "C"` model with C's in-place access pattern (read each
byte through `src`, then write it through `dst`) for in-place calls in both
directions and one separate-buffer call. Under `cargo +nightly miri test` it
passes on the fix; with the old two-line derivation restored it fails with
the same Stacked Borrows error as `model/`.
