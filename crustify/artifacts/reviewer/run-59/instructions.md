# Annotation pass: `src/libavutil/libavutil/log.rs`

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
| `AVClassCategory` | type | `libavutil/log.h` |  |
| `AVClassStateFlags` | type | `libavutil/log.h` |  |
| `av_default_item_name` | symbol | `libavutil/log.c` |  |
| `av_log` | symbol | `libavutil/log.c` |  |
| `av_log_default_callback` | symbol | `libavutil/log.c` |  |
| `av_log_format_line` | symbol | `libavutil/log.c` |  |
| `av_log_format_line2` | symbol | `libavutil/log.c` |  |
| `av_log_get_flags` | symbol | `libavutil/log.c` |  |
| `av_log_get_level` | symbol | `libavutil/log.c` |  |
| `av_log_once` | symbol | `libavutil/log.c` |  |
| `av_log_set_callback` | symbol | `libavutil/log.c` |  |
| `av_log_set_flags` | symbol | `libavutil/log.c` |  |
| `av_log_set_level` | symbol | `libavutil/log.c` |  |
| `av_vlog` | symbol | `libavutil/log.c` |  |
| `AVClass` | type | `libavutil/log.h` | `option`, `version`, `category`, `item_name`, `get_category`, `state_flags_offset`, `child_class_iterate`, `child_next`, `query_ranges`, `parent_log_context_offset`, `log_level_offset_offset`, `class_name` |
| `av_default_get_category` | symbol | `libavutil/log.c` |  |

A type-generating macro scheduled as a type is annotated as a `ty`, under the macro's
name. ffibox's policy macros (`impl_cdrop!` and the others) now take outer attributes
ahead of their arguments and put them on the trait method they mint, as `define_ctype!`
does: annotate a lifecycle routine there, not on the private adapter a macro is given.
The `/// Handwritten` marker that ffibox's skill asks for is not an annotation: leave it
as it is.

## Departures for this run

- **No reports.** File nothing under `crustify/reviews/`. A deviation you fix needs no
  report, and the batch lands one commit, its change to `log.rs`, rather than the
  reports commit and the fix commit the Landing section of the review how-to names.
- **No sanitizer, Miri or BSan gates, and no new tests.** The change is to doc comments
  and `#[doc(alias)]` attributes only, which change no compiled behaviour. Run the
  Regressions gates' check, format, lint and test commands, and
  `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p ffmpeg-bindings`, which rejects an
  alias where rustdoc admits none. Change no code beyond the annotations; if an annotation
  cannot be placed without one, leave it and say why in your final message.
- **No dashboard row and no ffimap scan.** This batch has no entry in `schedule.json`.
- **Do not commit `Cargo.lock`** or any file outside `log.rs`.
