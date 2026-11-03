# Annotation pass: `src/libavutil/libavutil/murmur3.rs`

This file's bindings predate the conventions' current Anchors section. Bring every
annotation in it to that section. Your `conventions` objective covers the Anchors section
only, this time: report any other deviation you notice in your final message, without
fixing it.

## Checklist

The C items the schedule homes in this file, all bound by landed `wrap` batches:

| C item | scheduled as | defined in | fields scheduled |
|---|---|---|---|
| `AVMurMur3` | type | `libavutil/murmur3.c` |  |
| `av_murmur3_alloc` | symbol | `libavutil/murmur3.c` |  |
| `av_murmur3_final` | symbol | `libavutil/murmur3.c` |  |
| `av_murmur3_init` | symbol | `libavutil/murmur3.c` |  |
| `av_murmur3_init_seeded` | symbol | `libavutil/murmur3.c` |  |
| `av_murmur3_update` | symbol | `libavutil/murmur3.c` |  |

A type-generating macro scheduled as a type is annotated as a `ty`, under the macro's
name.

The `/// Handwritten` marker is retired: remove each one in the file, and the empty doc
line it leaves at the end of a doc comment.

## Departures for this run

- **No reports.** File nothing under `crustify/reviews/`, and land one commit, your change
  to `murmur3.rs`, rather than the reports commit and the fix commit.
- **No sanitizer, Miri or BSan gates, and no new tests**: the change is to doc comments and
  `#[doc(alias)]` attributes only. Add `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p
  ffmpeg-bindings` to your gates: it rejects an alias where rustdoc admits none. Change no
  code beyond the annotations; if an annotation cannot be placed without one, leave it and
  say why in your final message.
- **No dashboard row and no ffimap scan**: this batch has no entry in `schedule.json`.
- **Commit nothing outside `murmur3.rs`**, `Cargo.lock` included.
