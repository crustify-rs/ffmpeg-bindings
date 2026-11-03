# Annotation pass: `src/libavutil/libavutil/pixdesc.rs`

The conventions' Anchors section has changed since this file was bound. Its bindings
still carry the earlier form, `/// Wraps: <name>`, `/// Replaces: <name>` and
`/// Field: <name>.<field>`, and no `#[doc(alias)]`. Bring every annotation in the file
to the current section: the kind-qualified anchor (`fn`, `ty`, `fld`, `var`) in backticks
and the alias beside it, on the item the What is annotated section names. Your
`conventions` objective covers the Anchors section only, this time: report any other
deviation you notice in your final message, without fixing it.

## Checklist

These are the C items the schedule homes in this file, all bound by landed `wrap` batches.
Each must end up with its annotations, on every stand-in the Several stand-ins section
names; any annotation for a C item not listed (a routine of another module that a
lifecycle strategy calls, for example) follows the same rules. Name a field by its C
name, as the C header spells it, even where bindgen renamed it (`str` for bindgen's
`str_`).

| C item | scheduled as | defined in | fields scheduled |
|---|---|---|---|
| `AVAlphaMode` | type | `libavutil/pixfmt.h` |  |
| `AVChromaLocation` | type | `libavutil/pixfmt.h` |  |
| `AVColorPrimaries` | type | `libavutil/pixfmt.h` |  |
| `AVColorRange` | type | `libavutil/pixfmt.h` |  |
| `AVColorSpace` | type | `libavutil/pixfmt.h` |  |
| `AVColorTransferCharacteristic` | type | `libavutil/pixfmt.h` |  |
| `AVComponentDescriptor` | type | `libavutil/pixdesc.h` | `plane`, `offset`, `depth`, `shift`, `step` |
| `AVPixelFormat` | type | `libavutil/pixfmt.h` |  |
| `av_chroma_location_from_name` | symbol | `libavutil/pixdesc.c` |  |
| `av_color_primaries_from_name` | symbol | `libavutil/pixdesc.c` |  |
| `av_color_range_from_name` | symbol | `libavutil/pixdesc.c` |  |
| `av_color_space_from_name` | symbol | `libavutil/pixdesc.c` |  |
| `av_color_transfer_from_name` | symbol | `libavutil/pixdesc.c` |  |
| `AVPixFmtDescriptor` | type | `libavutil/pixdesc.h` | `name`, `flags`, `nb_components`, `log2_chroma_w`, `log2_chroma_h`, `comp`, `alias` |
| `av_alpha_mode_from_name` | symbol | `libavutil/pixdesc.c` |  |
| `av_alpha_mode_name` | symbol | `libavutil/pixdesc.c` |  |
| `av_chroma_location_enum_to_pos` | symbol | `libavutil/pixdesc.c` |  |
| `av_chroma_location_name` | symbol | `libavutil/pixdesc.c` |  |
| `av_chroma_location_pos_to_enum` | symbol | `libavutil/pixdesc.c` |  |
| `av_color_primaries_name` | symbol | `libavutil/pixdesc.c` |  |
| `av_color_range_name` | symbol | `libavutil/pixdesc.c` |  |
| `av_color_space_name` | symbol | `libavutil/pixdesc.c` |  |
| `av_color_transfer_name` | symbol | `libavutil/pixdesc.c` |  |
| `av_find_best_pix_fmt_of_2` | symbol | `libavutil/pixdesc.c` |  |
| `av_get_pix_fmt` | symbol | `libavutil/pixdesc.c` |  |
| `av_get_pix_fmt_loss` | symbol | `libavutil/pixdesc.c` |  |
| `av_get_pix_fmt_name` | symbol | `libavutil/pixdesc.c` |  |
| `av_get_pix_fmt_string` | symbol | `libavutil/pixdesc.c` |  |
| `av_pix_fmt_count_planes` | symbol | `libavutil/pixdesc.c` |  |
| `av_pix_fmt_get_chroma_sub_sample` | symbol | `libavutil/pixdesc.c` |  |
| `av_pix_fmt_swap_endianness` | symbol | `libavutil/pixdesc.c` |  |
| `av_get_bits_per_pixel` | symbol | `libavutil/pixdesc.c` |  |
| `av_get_padded_bits_per_pixel` | symbol | `libavutil/pixdesc.c` |  |
| `av_pix_fmt_desc_get` | symbol | `libavutil/pixdesc.c` |  |
| `av_pix_fmt_desc_get_id` | symbol | `libavutil/pixdesc.c` |  |
| `av_pix_fmt_desc_next` | symbol | `libavutil/pixdesc.c` |  |
| `av_read_image_line` | symbol | `libavutil/pixdesc.c` |  |
| `av_read_image_line2` | symbol | `libavutil/pixdesc.c` |  |
| `av_write_image_line` | symbol | `libavutil/pixdesc.c` |  |
| `av_write_image_line2` | symbol | `libavutil/pixdesc.c` |  |

A type-generating macro scheduled as a type is annotated as a `ty`, under the macro's
name. ffibox's policy macros (`impl_cdrop!` and the others) now take outer attributes
ahead of their arguments and put them on the trait method they mint, as `define_ctype!`
does: annotate a lifecycle routine there, not on the private adapter a macro is given.
The `/// Handwritten` marker that ffibox's skill asks for is not an annotation: leave it
as it is.

## Departures for this run

- **No reports.** File nothing under `crustify/reviews/`. A deviation you fix needs no
  report, and the batch lands one commit, its change to `pixdesc.rs`, rather than the
  reports commit and the fix commit the Landing section of the review how-to names.
- **No sanitizer, Miri or BSan gates, and no new tests.** The change is to doc comments
  and `#[doc(alias)]` attributes only, which change no compiled behaviour. Run the
  Regressions gates' check, format, lint and test commands, and
  `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p ffmpeg-bindings`, which rejects an
  alias where rustdoc admits none. Change no code beyond the annotations; if an annotation
  cannot be placed without one, leave it and say why in your final message.
- **No dashboard row and no ffimap scan.** This batch has no entry in `schedule.json`.
- **Do not commit `Cargo.lock`** or any file outside `pixdesc.rs`.
