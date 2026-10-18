# Unsafe boundary: `AVFrame.hw_frames_ctx` kept its untyped seam after `AVHWFramesContext` was wrapped, and its contract let safe code reach a NULL call

- Items: `AVHWFramesContext` (this wave's type, `FramesContext{,Ref,Mut}` and
  `FramesReference` in `crustify/rust/ffmpeg/libavutil/hwcontext.rs`) and
  the seams in `crustify/rust/ffmpeg/libavutil/frame.rs` that were waiting
  for it: `FrameRef::hw_frames_context`, `FrameMut::replace_hw_frames_context`
  and `FrameMut::take_hw_frames_context` (buffer-frame wave 3, batch 0,
  `2a232726c8`).
- Affected commit: the wave tip `126a47005b`.
- Kind: a raw seam the type run should have replaced (cut SCC), plus a
  safety contract that did not cover what the safe API then does.
- Status: fixed. The accessors are typed and safe, and copies that would
  make C read a frame through its context's backend are refused.
  Reproducer: `src/main.rs` (`asan.trace`).

## Finding

### 1. The seam was not replaced

The buffer-frame wave-3 type run wrapped `AVFrame.hw_frames_ctx` as a
temporary seam, with a note to replace it once `AVHWFramesContext` was
wrapped:

- `FrameRef::hw_frames_context() -> Option<BufferReferenceRef<'a>>`, a byte
  view of the context struct ("A typed handle replaces this byte view once
  hwcontext.h's AVHWFramesContext is wrapped");
- `unsafe fn replace_hw_frames_context(Option<BufferReferenceBox>)` ("A safe
  setter taking a typed frames context replaces this seam once hwcontext.h's
  AVHWFramesContext is wrapped");
- `take_hw_frames_context() -> Option<BufferReferenceBox>`.

This wave wrapped the context (`FramesReferenceBox`, `FramesReferenceRef`,
`FramesContextRef`) but left all three as they were. The type route asks a
run to replace lower-layer temporary raw references its wrapper makes
obsolete. `crustify scan-unsafe` lists the setter among the
`unsafe_fns_pub_smell` sites (`frame.rs:4666`).

### 2. The setter's contract was incomplete

The contract asked only that `with` be a reference to an initialised frames
context whose bytes nothing writes. But C treats a frame holding such a
reference as a hardware frame of that context. `av_frame_copy` checks for a
frames context before it looks at the planes, and hands the frame to
`av_hwframe_transfer_data` (`frame.c:676`). That routine calls the backend's
`transfer_data_from` without checking it (`hwcontext.c:490`). Two of
FFmpeg's backends have none: MediaCodec and OpenHarmony
(`hwcontext_mediacodec.c`, `hwcontext_oh.c`), so the call goes through NULL.
Every other backend reads the frame's planes as its surfaces.

`av_frame_ref` and `av_frame_clone` run `av_frame_copy` themselves for a
frame that is not reference counted (no `buf[0]`), and so does
`av_frame_replace` when such a frame has planes. The safe
`FrameRef::try_clone`, `FrameMut::try_clone_from` and `FrameMut::assign`
wrap those routines. Before calling C they checked buffer counts, the
`extended_data` length and the sample rounding, but not the frames context.

So a caller can stay within the contract (an initialised context, nothing
writing it), attach the context to a frame with a software format, a size
and no buffers, and then call the safe `try_clone()`. C then calls the
backend's transfer. The reproducer does exactly this with the crate's test
backend B, which, like MediaCodec, has no transfer routine:

```
==3302399==ERROR: AddressSanitizer: SEGV on unknown address 0xfffffffffffffff8 (pc 0x7e222f21c2da bp 0x7ffc3bb26340 sp 0x7ffc3bb26280 T0)
==3302399==The signal is caused by a READ memory access.
    #0 0x7e222f21c2da in av_hwframe_transfer_data src/libavutil/hwcontext.c:490:19
    #1 0x7e222f1f0b8f in av_frame_ref src/libavutil/frame.c:305:15
    #2 0x7e222f1f7fed in av_frame_clone src/libavutil/frame.c:490:9
    #3 0x60b8bc58b66a in <ffmpeg::libavutil::frame::FrameRef>::try_clone crustify/rust/ffmpeg/libavutil/frame.rs:3673:33
    #4 0x60b8bc5680e1 in frame_hw_frames_context_seam_left_untyped::main src/main.rs:61:31
```

(full trace: `asan.trace`). With backend A, C would read the frame's NULL
planes as a surface instead.

A stronger contract ("the frame's buffers must be the context's surfaces")
would not have helped. The safe setters change a frame's format and buffers
freely afterwards (`set_pixel_format`, `take_buffer(0)`, which clears the
planes), and that leads to the same state. `FrameMut::transfer_data_from`'s
own contract already says that `Frame` cannot tell a hardware frame from a
software frame given a context. Holding a context reference therefore
cannot vouch for anything about the frame's buffers.

## Fix

`crustify/rust/ffmpeg/libavutil/frame.rs`:

- `FrameRef::hw_frames_context() -> Option<FramesReferenceRef<'a>>`, borrowed
  from the frame. It replaces the byte view. A private `hw_frames_buffer`
  keeps the buffer view for `may_clone`'s count check.
- `FrameMut::replace_hw_frames_context(Option<FramesReferenceBox>) ->
  Option<FramesReferenceBox>` is safe now, and
  `take_hw_frames_context() -> Option<FramesReferenceBox>`.
  `FramesReferenceBox` holds only references to initialised contexts.
- `copy_refusal` returns `AVERROR(ENOSYS)` for a frame holding a frames
  context. So `try_clone` (`None`), `try_clone_from` and `assign` refuse to
  copy a frame that is not reference counted and holds one, before C runs.
  `copy_data_from` and `make_writable` already refused every such frame
  with `ENOSYS`. Reference counted frames are still copied by reference,
  the context's reference included, as C does.
- The `Frame` invariant and the setter's docs now say that the reference
  names a context and does not make the frame's buffers its surfaces.
  `HardwareFrame` holds the frames C allocates.

`crustify/rust/ffmpeg/libavutil/hwcontext.rs`: `HwFrames::of` takes the
typed handle.

## Regression tests

- `libavutil::hwcontext::unit_tests::contexts::frame_hw_frames_contexts_are_typed_and_uncounted_copies_refused`
  (lib) runs the reproducer's scenario on both test backends: `try_clone`,
  `try_clone_from` and `assign` (with lent planes) refused, no backend event.
  It also covers `assign` without planes (C's path that only takes the
  reference), reference counted copies, take/replace, and the counts after
  each step. Without the `copy_refusal` check, the test process dies with
  SIGSEGV.
- `equiv/hwcontext.rs::frame_hw_frames_context::references_follow_c_through_copies_takes_and_releases`
  compares the context's count and which frames hold it with raw C frames
  after each step.
- `ub_safe/frame.rs::frame_type::hw_frames_context_round_trips_none` (moved
  from `ub_unsafe/frame.rs`, now that the setter is safe) and
  `ub_safe/compile_fail/frame_hw_frames_context_outlives_frame.rs`, which
  checks that the typed handle is borrowed from the frame.

## Running the reproducer

```
CRUSTIFY_FFMPEG_VARIANT=asan-ubsan RUSTFLAGS=-Zsanitizer=address \
  cargo +nightly run --offline -Zbuild-std --target x86_64-unknown-linux-gnu
```

with `LD_LIBRARY_PATH` set to the asan-ubsan build's `lib*` directories
(`crustify/builds/asan-ubsan`). It exits 1 on `126a47005b`, with the trace
above. On the fixed tree it no longer compiles, because the setter now takes
a `FramesReferenceBox` and is no longer `unsafe`. The same scenario through
the safe API is the unit test above.
