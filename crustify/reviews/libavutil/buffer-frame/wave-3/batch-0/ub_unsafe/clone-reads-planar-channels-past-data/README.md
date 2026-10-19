# UB (unsafe API): a clone reads planar channels past the frame's `data`

- Items: `AVFrame` (`FrameRef::try_clone`, safe, over `av_frame_clone`; and
  `FrameMut::set_data_ptr`, unsafe), homed in
  `crustify/rust/ffmpeg/libavutil/frame.rs`.
- Affected commit: the wave tip `4f959b26a4` (translation batch 0).
- Instrument: ASan (SEGV on a wild read). BSan reports the same SEGV. Both
  traces are kept here as `asan.trace` and `bsan.trace`.
- Status: fixed in `frame.rs` (`FrameRef::try_clone` now refuses such a
  frame, through `copy_reads_past_extended_data`). Regression test:
  `tests/ub_unsafe/frame.rs::frame_type::clones_refuse_channels_past_data`.
  Unit test of the guard:
  `frame.rs::unit_tests::frame_copies_reading_past_extended_data`.

## Finding

The type's invariant allows `extended_data` to be the frame's own `data`
array whatever the channel count. Only a *separate* array must hold
`nb_channels` entries. `try_clone`'s safety comment relied on "a frame that
is not reference counted has NULL planes or planes whose setter vouched for
them".

A frame without `buf[0]` is not reference counted. For such a frame,
`av_frame_ref` copies the data (`frame.c:300-310`): `av_frame_get_buffer`,
then `av_frame_copy`. For audio that is `frame_copy_audio`
(`frame.c:691-709`), which reads `src->extended_data[i]` for every channel
of a planar format, then `av_samples_copy`s each one. It never checks
whether `extended_data` is the frame's 8-entry `data`. With 9 channels,
entry 8 is the next field of `AVFrame`, `linesize[0..2]`, read as a
pointer. With `linesize[0] = 64` (C's convention for planar audio:
`linesize[0]` is each plane's size), C copies 64 bytes from address `0x40`.

The reproducer lends eight planes through `set_data_ptr`. Each one is valid
for every byte C reads from it, for the frame's whole life, as the
`# Safety` asks. Everything else is safe setters: `U8P`, 64 samples,
`linesize[0] = 64`, a 9-channel layout (accepted, because `extended_data`
is not separate). The ninth "plane" is no pointer the caller ever stored,
so the contract does not cover it. The over-read is a defect of
`try_clone`, not of the caller.

No safe-only path reaches it: without a lent plane, `data[0]` is NULL and C
fails before channel 8. A reference-counted frame is cloned by reference,
with no copy. The same over-read is in every C routine that walks planar
channels through `extended_data`: `av_frame_copy`,
`av_frame_make_writable` (through `av_frame_copy`) and
`av_frame_get_plane_buffer` (plane < channels). Wave 4 wraps those, and
their wrappers need the same check. The `Frame` docs now say so.

## Reproducer

Built for ASan in this directory, the binary exits 1 (`main` re-runs the
scenario in a child with `halt_on_error=1`):

```sh
CRUSTIFY_FFMPEG_VARIANT=asan-ubsan RUSTFLAGS=-Zsanitizer=address \
  cargo +nightly build -Zbuild-std --target x86_64-unknown-linux-gnu --offline
ASAN_SYMBOLIZER_PATH=/usr/lib/llvm-18/bin/llvm-symbolizer \
LD_LIBRARY_PATH=../../../../../../../../builds/asan-ubsan/libavutil \
  target/x86_64-unknown-linux-gnu/debug/clone-reads-planar-channels-past-data
```

The trace from the affected commit, abridged (full text in `asan.trace`):

```
cloning a 9-channel planar frame whose extended_data is data
==ERROR: AddressSanitizer: SEGV on unknown address 0x000000000040
==The signal is caused by a READ memory access.
    #0 in __memcpy_avx_unaligned_erms_rtm
    #1 in __asan_memcpy
    #2 in av_samples_copy   libavutil/samplefmt.c:237:13
    #3 in frame_copy_audio   libavutil/frame.c:705:5
    #4 in av_frame_copy   libavutil/frame.c:720:16
    #5 in av_frame_ref   libavutil/frame.c:305:15
    #6 in av_frame_clone   libavutil/frame.c:490:9
    #7 in <ffmpeg::libavutil::frame::FrameRef>::try_clone   frame.rs:3565:33
    #8 in clone_reads_planar_channels_past_data::scenario   main.rs:50:31
SUMMARY: AddressSanitizer: SEGV ... in __memcpy_avx_unaligned_erms_rtm
```

BSan (`CRUSTIFY_FFMPEG_VARIANT=bsan cargo bsan build`, binary under
`target/bsan/x86_64-unknown-linux-gnu/debug/`, `LD_LIBRARY_PATH` at
`builds/bsan/libavutil`) stops at the same read: `BorrowSanitizer: SEGV on
unknown address 0x000000000040`.

On the fixed tree it prints `no report: the clone was refused` and exits 0,
under ASan and under BSan.

## Fix

`try_clone` returns `None` for a frame that is not reference counted when C's
copy would read past `extended_data`. That is the audio branch of
`av_frame_copy` (not `width > 0 && height > 0`), with a planar sample format
and more channels than `extended_plane_count()`. C has no defined behaviour
there, so nothing observable is lost. With an array of its own
(`alloc_extended_data` and `set_extended_data_ptr` for each channel), the
clone copies every channel, as before. The new equivalence test
`equiv/frame.rs::frame_type::clones_copy_external_channels_alike` checks it
against C for 1, 2, 8, 9 and 13 channels.
