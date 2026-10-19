# Unsafe contract: `replace_hw_frames_context` let C write bytes Rust lends as `&[u8]`

- Item: `AVFrame.hw_frames_ctx` (`FrameMut::replace_hw_frames_context`,
  unsafe; `FrameRef::hw_frames_context`, safe), homed in
  `crustify/rust/ffmpeg/libavutil/frame.rs`.
- Affected commit: the wave tip `4f959b26a4` (translation batch 0).
- Status: `# Safety` fixed (documentation only). No reproducer: this build
  has no hardware device type, so no frames context can be made
  (`av_hwframe_ctx_alloc` needs a device), and no prepared instrument can
  run the path.

## Finding

The safe getter `FrameRef::hw_frames_context` returns the reference as a
`BufferReferenceRef<'a>`. Its safe `data()` lends the context's bytes, an
`AVHWFramesContext`, to Rust as a `&'a [u8]`. The unsafe setter's contract
only asked for "a reference to a hardware frames context (one
`av_hwframe_ctx_alloc` made, or a reference to it)".

That allows a context that is still being set up. FFmpeg's documented
sequence is `av_hwframe_ctx_alloc`, then the caller writes `format`,
`sw_format`, `width`, `height` and `initial_pool_size` through `ref->data`,
then `av_hwframe_ctx_init`. Init writes the struct too: `ctx->pool`
(`hwcontext.c:373-374`) and the device-specific `frames_init`. The buffer
is created `AV_BUFFER_FLAG_READONLY` (`hwcontext.c:286-288`), but C writes
it through the data pointer regardless. So, under the old contract, another
holder of the context could write those bytes while safe code read them
through a frame's `hw_frames_context().unwrap().data()`. That is a data
race on memory Rust holds a shared reference to.

## Fix

The `# Safety` of `replace_hw_frames_context` now also requires an
initialised context (`av_hwframe_ctx_init` succeeded). Nothing may write its
bytes while the frame, or a copy of it, holds the reference. It names the
safe byte view as the reason. In libavutil's `hwcontext*.c`, no write to the
public struct follows a successful init, until the last reference releases
it (device-specific state lives behind `hwctx` and the internal struct,
outside the buffer's bytes). The typed `AVHWFramesContext`
wrapper that replaces this seam (hwcontext sub-campaign) should give
read-only field access with the same precondition.
