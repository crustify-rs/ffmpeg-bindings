# Misc: `copy_props_from` documented a failed copy as copying every other property

- Item: `av_frame_copy_props` (`FrameMut::copy_props_from`,
  `crustify/rust/ffmpeg/libavutil/frame.rs`).
- Kind: documentation defect. The behaviour is C's and is unchanged.
- Status: doc fixed. The failure state is now pinned by
  `libavutil::frame::unit_tests::frame_routine_failures_under_allocation_limits`.

The `# Errors` section said that after `AVERROR(ENOMEM)` from a failed side
data copy, the frame has lost its side data and has "its other properties
copied". `frame_copy_props` (`frame.c:220-276`) copies the plain fields and
merges the metadata first. When `av_frame_new_side_data` fails, it frees
`dst`'s side data and returns. It never reaches the last two steps:

```c
    av_refstruct_replace(&dst->private_ref, src->private_ref);
    return av_buffer_replace(&dst->opaque_ref, src->opaque_ref);
```

The frame therefore keeps its own private and user data references
(`opaque_ref`). A caller who trusts the old wording would expect `src`'s
`opaque_ref` after the error. The doc now describes this state. The
allocation-limit unit test asserts it: a new frame copying from a source that
has an `opaque_ref` still has none after the failure.
