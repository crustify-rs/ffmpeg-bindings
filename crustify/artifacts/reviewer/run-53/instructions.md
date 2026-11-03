# Annotation pass: `src/libavutil/libavutil/opt.rs`

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
| `AVOption` | type | `libavutil/opt.h` | `name`, `flags`, `unit`, `type`, `min`, `max`, `offset`, `help`, `default_val` |
| `AVOptionRanges` | type | `libavutil/opt.h` | `nb_components`, `range`, `nb_ranges` |
| `AVOptionArrayDef` | type | `libavutil/opt.h` | `size_max`, `sep`, `def`, `size_min` |
| `AVOptionRange` | type | `libavutil/opt.h` | `str`, `is_range`, `component_max`, `component_min`, `value_max`, `value_min` |
| `AVOptionType` | type | `libavutil/opt.h` |  |
| `av_opt_child_next` | symbol | `libavutil/opt.c` |  |
| `av_opt_copy` | symbol | `libavutil/opt.c` |  |
| `av_opt_flag_is_set` | symbol | `libavutil/opt.c` |  |
| `av_opt_free` | symbol | `libavutil/opt.c` |  |
| `av_opt_get` | symbol | `libavutil/opt.c` |  |
| `av_opt_get_array_size` | symbol | `libavutil/opt.c` |  |
| `av_opt_get_double` | symbol | `libavutil/opt.c` |  |
| `av_opt_get_image_size` | symbol | `libavutil/opt.c` |  |
| `av_opt_get_int` | symbol | `libavutil/opt.c` |  |
| `av_opt_get_key_value` | symbol | `libavutil/opt.c` |  |
| `av_opt_is_set_to_default_by_name` | symbol | `libavutil/opt.c` |  |
| `av_opt_serialize` | symbol | `libavutil/opt.c` |  |
| `av_opt_set` | symbol | `libavutil/opt.c` |  |
| `av_opt_set_bin` | symbol | `libavutil/opt.c` |  |
| `av_opt_set_defaults` | symbol | `libavutil/opt.c` |  |
| `av_opt_set_defaults2` | symbol | `libavutil/opt.c` |  |
| `av_opt_set_double` | symbol | `libavutil/opt.c` |  |
| `av_opt_set_from_string` | symbol | `libavutil/opt.c` |  |
| `av_opt_set_image_size` | symbol | `libavutil/opt.c` |  |
| `av_opt_set_int` | symbol | `libavutil/opt.c` |  |
| `av_opt_show2` | symbol | `libavutil/opt.c` |  |
| `av_set_options_string` | symbol | `libavutil/opt.c` |  |
| `av_opt_child_class_iterate` | symbol | `libavutil/opt.c` |  |
| `av_opt_get_array` | symbol | `libavutil/opt.c` |  |
| `av_opt_get_chlayout` | symbol | `libavutil/opt.c` |  |
| `av_opt_get_dict_val` | symbol | `libavutil/opt.c` |  |
| `av_opt_get_pixel_fmt` | symbol | `libavutil/opt.c` |  |
| `av_opt_get_q` | symbol | `libavutil/opt.c` |  |
| `av_opt_get_sample_fmt` | symbol | `libavutil/opt.c` |  |
| `av_opt_get_video_rate` | symbol | `libavutil/opt.c` |  |
| `av_opt_set_array` | symbol | `libavutil/opt.c` |  |
| `av_opt_set_chlayout` | symbol | `libavutil/opt.c` |  |
| `av_opt_set_dict` | symbol | `libavutil/opt.c` |  |
| `av_opt_set_dict2` | symbol | `libavutil/opt.c` |  |
| `av_opt_set_dict_val` | symbol | `libavutil/opt.c` |  |
| `av_opt_set_pixel_fmt` | symbol | `libavutil/opt.c` |  |
| `av_opt_set_q` | symbol | `libavutil/opt.c` |  |
| `av_opt_set_sample_fmt` | symbol | `libavutil/opt.c` |  |
| `av_opt_set_video_rate` | symbol | `libavutil/opt.c` |  |
| `av_opt_eval_double` | symbol | `libavutil/opt.c` |  |
| `av_opt_eval_flags` | symbol | `libavutil/opt.c` |  |
| `av_opt_eval_float` | symbol | `libavutil/opt.c` |  |
| `av_opt_eval_int` | symbol | `libavutil/opt.c` |  |
| `av_opt_eval_int64` | symbol | `libavutil/opt.c` |  |
| `av_opt_eval_q` | symbol | `libavutil/opt.c` |  |
| `av_opt_eval_uint` | symbol | `libavutil/opt.c` |  |
| `av_opt_find` | symbol | `libavutil/opt.c` |  |
| `av_opt_find2` | symbol | `libavutil/opt.c` |  |
| `av_opt_freep_ranges` | symbol | `libavutil/opt.c` |  |
| `av_opt_is_set_to_default` | symbol | `libavutil/opt.c` |  |
| `av_opt_next` | symbol | `libavutil/opt.c` |  |
| `av_opt_query_ranges` | symbol | `libavutil/opt.c` |  |
| `av_opt_query_ranges_default` | symbol | `libavutil/opt.c` |  |

A type-generating macro scheduled as a type is annotated as a `ty`, under the macro's
name. ffibox's policy macros (`impl_cdrop!` and the others) now take outer attributes
ahead of their arguments and put them on the trait method they mint, as `define_ctype!`
does: annotate a lifecycle routine there, not on the private adapter a macro is given.
The `/// Handwritten` marker that ffibox's skill asks for is not an annotation: leave it
as it is.

## Departures for this run

- **No reports.** File nothing under `crustify/reviews/`. A deviation you fix needs no
  report, and the batch lands one commit, its change to `opt.rs`, rather than the
  reports commit and the fix commit the Landing section of the review how-to names.
- **No sanitizer, Miri or BSan gates, and no new tests.** The change is to doc comments
  and `#[doc(alias)]` attributes only, which change no compiled behaviour. Run the
  Regressions gates' check, format, lint and test commands, and
  `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p ffmpeg-bindings`, which rejects an
  alias where rustdoc admits none. Change no code beyond the annotations; if an annotation
  cannot be placed without one, leave it and say why in your final message.
- **No dashboard row and no ffimap scan.** This batch has no entry in `schedule.json`.
- **Do not commit `Cargo.lock`** or any file outside `opt.rs`.
