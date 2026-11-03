# Annotation pass: `src/libavutil/libavutil/csp.rs`

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
| `av_csp_eotf_function` | callback | `libavutil/csp.h` |  |
| `av_csp_trc_function` | callback | `libavutil/csp.h` |  |
| `AVCIExy` | type | `libavutil/csp.h` | `y`, `x` |
| `AVLumaCoefficients` | type | `libavutil/csp.h` | `cb`, `cr`, `cg` |
| `av_csp_approximate_eotf_gamma` | symbol | `libavutil/csp.c` |  |
| `av_csp_approximate_trc_gamma` | symbol | `libavutil/csp.c` |  |
| `av_csp_itu_eotf` | symbol | `libavutil/csp.c` |  |
| `av_csp_itu_eotf_inv` | symbol | `libavutil/csp.c` |  |
| `av_csp_trc_func_from_id` | symbol | `libavutil/csp.c` |  |
| `av_csp_trc_func_inv_from_id` | symbol | `libavutil/csp.c` |  |
| `AVPrimaryCoefficients` | type | `libavutil/csp.h` | `b`, `g`, `r` |
| `av_csp_luma_coeffs_from_avcsp` | symbol | `libavutil/csp.c` |  |
| `AVColorPrimariesDesc` | type | `libavutil/csp.h` | `prim`, `wp` |
| `av_csp_primaries_desc_from_id` | symbol | `libavutil/csp.c` |  |
| `av_csp_primaries_id_from_desc` | symbol | `libavutil/csp.c` |  |

A type-generating macro scheduled as a type is annotated as a `ty`, under the macro's
name. ffibox's policy macros (`impl_cdrop!` and the others) now take outer attributes
ahead of their arguments and put them on the trait method they mint, as `define_ctype!`
does: annotate a lifecycle routine there, not on the private adapter a macro is given.
The `/// Handwritten` marker that ffibox's skill asks for is not an annotation: leave it
as it is.

## Departures for this run

- **No reports.** File nothing under `crustify/reviews/`. A deviation you fix needs no
  report, and the batch lands one commit, its change to `csp.rs`, rather than the
  reports commit and the fix commit the Landing section of the review how-to names.
- **No sanitizer, Miri or BSan gates, and no new tests.** The change is to doc comments
  and `#[doc(alias)]` attributes only, which change no compiled behaviour. Run the
  Regressions gates' check, format, lint and test commands, and
  `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p ffmpeg-bindings`, which rejects an
  alias where rustdoc admits none. Change no code beyond the annotations; if an annotation
  cannot be placed without one, leave it and say why in your final message.
- **No dashboard row and no ffimap scan.** This batch has no entry in `schedule.json`.
- **Do not commit `Cargo.lock`** or any file outside `csp.rs`.
