# Misc: the header's standard layouts and channel masks had no Rust spelling

- Files: `crustify/rust/ffmpeg/libavutil/channel_layout.rs`,
  `crustify/rust/libavutil-sys/shim.{h,c}`,
  `crustify/rust/ffmpeg/tests/common/channel_layout.rs`,
  `crustify/rust/ffmpeg/tests/{equiv,ub_safe}/channel_layout.rs`
- Affected commit: `ac34a06476` (wave 2 batch 0, `AVChannelLayout`)
- Item: `AVChannelLayout` (`ChannelLayout`), and `AVChannel` (`Channel`)
  for the per-channel bits

## Finding

`libavutil/channel_layout.h` publishes, beside the struct and its
routines, three macro families that are how C code spells layouts:

- `AV_CH_<NAME>` (36): the native-mask bit of each named channel,
  `1ULL << AV_CHAN_<NAME>`;
- `AV_CH_LAYOUT_<NAME>` (40 non-deprecated): the standard layouts as masks,
  composed from the bits (`AV_CH_LAYOUT_5POINT1` is
  `AV_CH_LAYOUT_5POINT0 | AV_CH_LOW_FREQUENCY`);
- `AV_CHANNEL_LAYOUT_MASK(nb, m)` and `AV_CHANNEL_LAYOUT_<NAME>` (40
  non-deprecated, plus `AV_CHANNEL_LAYOUT_AMBISONIC_FIRST_ORDER`):
  `AVChannelLayout` compound literals, the initialisers C users write
  (`AVChannelLayout l = AV_CHANNEL_LAYOUT_STEREO;`) and `channel_layout.c`'s
  own `channel_layout_map` table.

None of them reached Rust. They are not wavefront nodes, so no batch is
ever scheduled for them; the translator's constant-macro rule ("use the
generated binding") has nothing to use, because bindgen cannot evaluate a
shift of an enum constant or a compound literal (adding `var AV_CH_.*` to
the allowlist produces no item, checked in this review), and the campaign's
deprecation list names the four `FF_API_CHANNEL_LAYOUT_BACK` aliases, so the
rest are in scope. The wave-0 and wave-2 translations left a consumer with
`ChannelLayout::parse(c"5.1")` (a runtime string match through C) or
`from_mask(0x60f)` (a literal the header never shows) as the only ways to
spell a standard layout, and no way at all to name a channel's bit.

Precedent: formats wave 0 wrapped the 104 `AV_PIX_FMT_NE` aliases as
`PixelFormat` constants with a `crustify_AV_PIX_FMT_NE` table shim as the
C reference; the core wave-0 review added `AV_IS_INPUT_DEVICE` /
`AV_IS_OUTPUT_DEVICE` to `ClassCategory` through shims.

## Fix

- `Channel::mask() -> Option<u64>`: `1 << id` for an id in `0..64`, which
  is every `AV_CH_<NAME>` and also C's native bit for the unnamed ids its
  readers index; `None` for `NONE`, `UNUSED`, `UNKNOWN` and the Ambisonic
  components.
- `channel_layout::masks`: the 40 `AV_CH_LAYOUT_<NAME>` masks as `u64`
  constants composed exactly as the header composes them (so a reader can
  check each line against `channel_layout.h`). A C name starting with a
  digit is not a Rust identifier, so those carry an `L`
  (`masks::L5POINT1`), as `pixfmt` spells `AV_PIX_FMT_0RGB` `XRGB`.
- `ChannelLayout::native(mask)`: `AV_CHANNEL_LAYOUT_MASK(popcount(m), m)`,
  the only way the header and FFmpeg use the macro (a mismatched `nb`
  would be a layout `av_channel_layout_check` rejects); `const`, so the
  40 `ChannelLayout::<NAME>` constants and `AMBISONIC_FIRST_ORDER` are
  built from it. They are bare `ChannelLayout` values owning nothing;
  `impl From<ChannelLayout> for ChannelLayoutVal` makes one an owner for
  the setters (`l.as_mut().set(ChannelLayout::STEREO.into())`).
- `libavutil-sys/shim.{h,c}`: `crustify_AV_CH`, `crustify_AV_CH_LAYOUT`
  and `crustify_AV_CHANNEL_LAYOUT` enumerate the three C tables by index
  with their names (the `crustify_AV_PIX_FMT_NE` shape), and
  `crustify_AV_CHANNEL_LAYOUT_MASK(nb, m, out)` is the macro itself.
  `shim.h` now includes `libavutil/channel_layout.h`; `crustify_.*` is
  already allowlisted.

## Tests

- `mod unit_tests`: `channel_mask_is_the_bit_of_ids_below_64`,
  `masks_compose_as_the_header_and_count_their_channels` (both FFI-free,
  Miri),  `standard_layouts_are_native_over_their_masks` (fields, `native`
  against `set_native`, constants as `const` items and as owners).
- `tests/equiv/channel_layout.rs::layout`:
  `channel_bits_match_the_c_av_ch_macros` (every `AV_CH_*` through the
  shim, and `1 << id` against `av_channel_layout_channel_from_index` /
  `index_from_channel` for all 64 ids),
  `layout_masks_match_the_c_av_ch_layout_macros` (every `AV_CH_LAYOUT_*`,
  and the layouts C builds from them), `standard_layouts_match_the_c_av_channel_layout_macros`
  (every compound literal as C's readers observe it, `av_channel_layout_check`,
  the name `av_channel_layout_describe` gives it parsed back by both sides
  and `av_channel_layout_compare`d; the two tables are equal name for
  name), `native_matches_the_c_av_channel_layout_mask_macro` (the macro
  and `av_channel_layout_from_mask` at the mask edges).
- `tests/ub_safe/channel_layout.rs::layout`:
  `masks_and_bits_feed_every_mask_path`, `standard_layouts_are_plain_values`
  (read, cloned, owned, overwritten, moved across threads, dropped).
- `tests/common/channel_layout.rs` carries the three name tables the
  suites share.
