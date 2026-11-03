# Annotation pass: `src/libavutil/libavutil/tea.rs`

This file's bindings predate the conventions' current Anchors section. Bring every
annotation in it to that section. Your `conventions` objective covers the Anchors section
only, this time: report any other deviation you notice in your final message, without
fixing it.

## Checklist

The C items the schedule homes in this file, all bound by landed `wrap` batches:

| C item | scheduled as | defined in | fields scheduled |
|---|---|---|---|
| `AVTEA` | type | `libavutil/tea.c` |  |
| `av_tea_size` | symbol | `libavutil/tea.c` |  |
| `av_tea_alloc` | symbol | `libavutil/tea.c` |  |
| `av_tea_crypt` | symbol | `libavutil/tea.c` |  |
| `av_tea_init` | symbol | `libavutil/tea.c` |  |

A type-generating macro scheduled as a type is annotated as a `ty`, under the macro's
name.

The `/// Handwritten` marker is retired: remove each one in the file, and the empty doc
line it leaves at the end of a doc comment.

## Departures for this run

- **No reports.** File nothing under `crustify/reviews/`, and land one commit, your change
  to `tea.rs`, rather than the reports commit and the fix commit.
- **No sanitizer, Miri or BSan gates, and no new tests**: the change is to doc comments and
  `#[doc(alias)]` attributes only. Add `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p
  ffmpeg-bindings` to your gates: it rejects an alias where rustdoc admits none. Change no
  code beyond the annotations; if an annotation cannot be placed without one, leave it and
  say why in your final message.
- **No dashboard row and no ffimap scan**: this batch has no entry in `schedule.json`.
- **Commit nothing outside `tea.rs`**, `Cargo.lock` included.
