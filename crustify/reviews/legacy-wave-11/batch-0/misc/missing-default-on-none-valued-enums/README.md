# Four of the wave's twelve enum newtypes had no `Default`

Affected base: `17c012afcd0e1e6fab63a97ac3e622e16db2cfba` (the wave tip; the
types landed in `41844f9750`, `653bf3f6d0`, `0c753e50ee` and `b39657734d`).
Homes: `crustify/rust/ffmpeg/libavutil/{pixfmt,samplefmt,channel_layout}.rs`.

`ColorSpace`, `ColorTransfer`, `AlphaMode`, `ColorPrimaries`, `ColorRange`,
`ChromaLocation` and `ChannelOrder` implement `Default` as the value C gives
an unset field. `PixelFormat`, `SampleFormat`, `Channel` and
`MatrixEncoding` did not, although C has the same notion for each:

- `PixelFormat` / `SampleFormat`: `av_frame_alloc` (`get_frame_defaults`,
  `libavutil/frame.c:41`) writes `-1` into `AVFrame.format`, which is
  `AV_PIX_FMT_NONE` and `AV_SAMPLE_FMT_NONE`; the codec context's `pix_fmt`
  and `sample_fmt` options default to the same values.
- `Channel`: `av_channel_from_string` (`libavutil/channel_layout.c:153`)
  and `av_channel_layout_channel_from_index` answer anything they cannot
  name or index with `AV_CHAN_NONE`.
- `MatrixEncoding`: `AV_MATRIX_ENCODING_NONE` is the zeroed value and the
  default of libswresample's `matrix_encoding` option (asserted by the
  existing `tests/equiv/channel_layout.rs::matrix_encoding::
  option_default_and_range_span_the_named_values`).

Without `Default`, a Rust struct holding one of these (a frame-parameter
builder, say) cannot derive `Default`, while one holding any of the other
seven can; the asymmetry is an ergonomic defect, not a soundness one.

Fix: `impl Default` as `NONE` on all four, with the choice stated in each
type's doc. Regression tests: inline
`pixel_format_default_is_none`, `samplefmt::unit_tests::default_is_none`,
`channel_default_is_none`, `matrix_encoding_default_is_none_the_zeroed_value`;
against C, `tests/equiv/pixfmt.rs::frame_defaults::
every_default_is_what_c_writes_into_a_fresh_frame` (all seven frame-stored
defaults plus `format` against `av_frame_alloc`),
`tests/equiv/samplefmt.rs::default_is_the_format_c_gives_a_fresh_frame`,
`tests/equiv/channel_layout.rs::channel_default_is_what_c_returns_for_an_unknown_channel`,
and the `MatrixEncoding::default()` assertion added to the swr option test.
