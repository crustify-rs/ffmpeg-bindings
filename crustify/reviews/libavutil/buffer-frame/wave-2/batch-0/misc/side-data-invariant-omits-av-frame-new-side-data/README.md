# Misc: `SideData`'s invariant named only one C routine that leaves a payload uninitialised

- File: `crustify/rust/ffmpeg/libavutil/frame.rs`, the `define_ctype!` docs
  of `SideData` (`AVFrameSideData`)
- Affected commit: `83bd5f2e64` (buffer-frame wave 2, type batch 0)

## Finding

The type's invariant says every entry a handle reaches has `size`
initialised payload bytes. The docs then said:

> Every C routine that builds or changes an entry keeps that, save one:
> `av_frame_side_data_new` leaves the payload uninitialised for its caller
> to fill.

There are two such public routines. `libavutil/frame.c` has the frame-level
variant, which takes its payload from `av_buffer_alloc` without initialising
it:

```c
AVFrameSideData *av_frame_new_side_data(AVFrame *frame,
                                        enum AVFrameSideDataType type,
                                        size_t size)
{
    AVFrameSideData *ret;
    AVBufferRef *buf = av_buffer_alloc(size);
    ret = av_frame_new_side_data_from_buf(frame, type, buf);
    ...
```

No path in this workset reaches either routine, so nothing is unsound
today. But the sentence is a guarantee that the wrappers lean on:
`SideDataRef::data` hands out `&[u8]` over every entry a frame's list lends.
Wave 4 schedules `av_frame_new_side_data`, and the frame wrappers lend a
frame's own list as a `SideDataArrayRef`. A wrapper author who trusts "save
one" could return the new entry as a `SideDataMut` without filling it. That
is a safe read of uninitialised bytes. The translator's commit message
gives the same warning for `av_frame_side_data_new` only.

## Fix

Docs only. The sentence now reads:

> Every C routine that builds or changes an entry keeps that, save the two
> that allocate its payload themselves: `av_frame_side_data_new` and the
> frame-level `av_frame_new_side_data` take it from `av_buffer_alloc` and
> leave it uninitialised for their caller to fill.

The rule for both wrappers (wave 3 and wave 4) is the same: fill the payload,
or hand it out in an uninitialised form, before a `SideDataRef` can reach
it.
