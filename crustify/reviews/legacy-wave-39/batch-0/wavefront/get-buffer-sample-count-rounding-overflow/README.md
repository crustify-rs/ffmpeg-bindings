# Wavefront: `av_frame_get_buffer`'s record missed the sample-count rounding overflow

- Records: `av_frame_get_buffer`, `av_frame_ref`, `av_frame_clone`,
  `av_frame_make_writable` (all `libavutil/frame.c`), submitted by
  translation batch 0 (`av_frame_clone` by the wave-3 type run).
- Status: resubmitted with `query symbols --name <n> --file libavutil/frame.c
  --update <n>.json`. The four files here are the submissions. Only the
  `note` of one pointer argument changed in each.

## What was wrong

The `frame` note of `av_frame_get_buffer` lists its `int` hazards as
"align > 2^27 overflows int in get_video_buffer, a non-power-of-two FFALIGN
can move a pointer before its buffer, negative align reaches
av_samples_get_buffer_size". It leaves out the overflow at the default
alignment. For an audio frame without a line size, `get_audio_buffer` calls
`av_samples_get_buffer_size(..., align)`. With `align == 0`, that rounds
`nb_samples` up with `FFALIGN(nb_samples, 32)`, i.e. `nb_samples + 32 - 1`
in `int` (`samplefmt.c:137`). The range check before it rejects only counts
above `INT_MAX - 31`.

The notes of the three routines that call `av_frame_get_buffer(dst, 0)`
themselves say only that they call it: `av_frame_ref` and `av_frame_clone`
for a source that is not reference counted, and `av_frame_make_writable` for
its copy. None names the hazard. The wrappers of those paths
(`try_clone_from`, `try_clone`, `assign`, `make_writable`) were left
unguarded. That gap is the UB defect in
`../../ub_safe/audio-copy-sample-count-rounding-overflow`, whose
`ubsan.trace` shows the overflow reached from `make_writable` and is the
evidence that the earlier finding was incomplete.

## Correction

Each note now names the `INT_MAX - 31` overflow and how the wrapper avoids
it. `alloc_buffers` and `make_writable` give C a line size. The other three
refuse a source that is not reference counted. Ownership, nullability,
mutability and lifecycle roles were correct and are resubmitted unchanged.
The remaining 14 records of the workset were checked against `frame.c` and
the wrappers and match.
