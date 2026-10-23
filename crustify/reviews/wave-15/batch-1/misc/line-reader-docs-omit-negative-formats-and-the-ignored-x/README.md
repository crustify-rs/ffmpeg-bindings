# `PixelFormat::descriptor` and the line readers' docs left two C behaviours unstated

Kind: misc (documentation). Items: `av_pix_fmt_desc_get`
(`PixelFormat::descriptor`), `av_read_image_line2` / `av_write_image_line2`
(`pixdesc::read_image_line`, `write_image_line`). Home:
`crustify/rust/ffmpeg/libavutil/pixdesc.rs`. Affected commit: `57c0cc8756`
(wave-2 batch 1), reviewed at `ac34a06476`.

## Finding

1. `PixelFormat::descriptor` documented `None` "for `NONE` and every value
   past the last format these bindings know (`AV_PIX_FMT_NB`)". C's
   `av_pix_fmt_desc_get` returns NULL for every `pix_fmt < 0`, so
   `PixelFormat::from_raw(ffi::AVPixelFormat(-3)).descriptor()` is `None`
   too (the equivalence test `descriptor_table::descriptor_is_cs_table_entry`
   already checks `-3..0`); the doc read as if only `NONE` were excluded.

2. `read_image_line` says it reads "the pixels from `x` on of row `y`".
   For the two bitstream formats of C's table whose component step exceeds
   8 bits, `xv30be` and `v30xbe` (`comp.step == 32`,
   `AV_PIX_FMT_FLAG_BITSTREAM`), C's `av_read_image_line2` and
   `av_write_image_line2` take the "all channels packed into a 32-bit value"
   path (`pixdesc.c:52-65` and `:134-146`), whose pointer is
   `data[plane] + y * linesize[plane]`: `x` is never used, and the line
   starts at the row's first word whatever `x` is. The wrappers mirror this
   exactly (`line_plan`'s `bitstream && step > 8` arm spans
   `row..row + 4 * w`, and the equivalence sweeps compare both formats at
   `x` 0, 1 and 3 against C), but a consumer reading the doc would expect
   `x` to matter.

Neither is a code defect: the wrappers' behaviour equals C's. Both are doc
omissions a consumer would trip over.

## Fix

`pixdesc.rs`: `PixelFormat::descriptor` now says `None` for `NONE` and
every other negative value, and from `AV_PIX_FMT_NB` on; `read_image_line`
gains a paragraph on the 32-bit-word bitstream formats (naming `XV30BE` and
`V30XBE`) and `write_image_line` refers to it. `RUSTDOCFLAGS=-D warnings
cargo doc -p ffmpeg --no-deps` passes.

No test change: the behaviour was already asserted (equivalence sweeps over
every table format including `xv30be`/`v30xbe` at three `x` positions, and
the `-3..NB + 3` lookup sweep).
