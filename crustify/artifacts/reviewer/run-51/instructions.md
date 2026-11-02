# Annotation pass: `src/libavutil/libavutil/bprint.rs`

The conventions' Anchors section has changed since this file was bound. Its bindings
still carry the earlier form, `/// Wraps: <name>`, `/// Replaces: <name>` and
`/// Field: <name>.<field>`, and no `#[doc(alias)]`. Bring every annotation in the file
to the current section: the kind-qualified anchor (`fn`, `ty`, `fld`, `var`) and the alias
beside it, on the item the What is annotated section names. Your `conventions` objective
covers the Anchors section only, this time: report any other deviation you notice in your
final message, without fixing it.

## Checklist

These are the C items the schedule homes in this file, all bound by landed `wrap` batches.
Each must end up with its annotations, on every stand-in the Several stand-ins section
names; any annotation for a C item not listed (a routine of another module that a
lifecycle strategy calls, for example) follows the same rules.

| C item | scheduled as | defined in | fields scheduled |
|---|---|---|---|
| `FF_PAD_STRUCTURE` | type | `libavutil/bprint.h` | |
| `AVBPrint` | type | `libavutil/bprint.h` | `len`, `str`, `size`, `reserved_padding`, `reserved_internal_buffer`, `size_max` |
| `ff_pad_helper_AVBPrint` | type | `libavutil/bprint.h` | `len`, `str`, `size`, `reserved_internal_buffer`, `size_max` |
| `av_bprint_append_data` | symbol | `libavutil/bprint.c` | |
| `av_bprint_chars` | symbol | `libavutil/bprint.c` | |
| `av_bprint_clear` | symbol | `libavutil/bprint.c` | |
| `av_bprint_escape` | symbol | `libavutil/bprint.c` | |
| `av_bprint_finalize` | symbol | `libavutil/bprint.c` | |
| `av_bprint_get_buffer` | symbol | `libavutil/bprint.c` | |
| `av_bprint_init` | symbol | `libavutil/bprint.c` | |
| `av_bprint_init_for_buffer` | symbol | `libavutil/bprint.c` | |
| `av_bprint_is_complete` | symbol | `libavutil/bprint.h` | |
| `av_bprint_strftime` | symbol | `libavutil/bprint.c` | |
| `av_bprintf` | symbol | `libavutil/bprint.c` | |
| `av_vbprintf` | symbol | `libavutil/bprint.c` | |

`FF_PAD_STRUCTURE` is a type-generating macro, scheduled as a type: decide its kind from
what it generates, as the Macros section of the binding how-to routes it.

ffibox's policy macros (`impl_cdrop!` and the others) now take outer attributes ahead
of their arguments and put them on the trait method they mint, as `define_ctype!` does:
annotate a lifecycle routine there, not on the private adapter a macro is given. The
`/// Handwritten` marker that ffibox's skill asks for is not an annotation: leave it as
it is.

## Departures for this run

- **No reports.** File nothing under `crustify/reviews/`. A deviation you fix needs no
  report, and the batch lands one commit, its change to `bprint.rs`, rather than the
  reports commit and the fix commit the Landing section of the review how-to names.
- **No sanitizer, Miri or BSan gates, and no new tests.** The change is to doc comments
  and `#[doc(alias)]` attributes only, which change no compiled behaviour. Run the
  Regressions gates' check, format, lint and test commands, and
  `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p ffmpeg-bindings`, which rejects an
  alias where rustdoc admits none. Change no code beyond the annotations; if an annotation
  cannot be placed without one, leave it and say why in your final message.
- **No dashboard row and no ffimap scan.** This batch has no entry in `schedule.json`.
- **Do not commit `Cargo.lock`** or any file outside `bprint.rs`.
