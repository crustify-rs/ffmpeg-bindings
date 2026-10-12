# Wavefront: `AVFrame.hw_frames_ctx`'s note missed C's copy through the backend

- Entity: type `AVFrame` (`libavutil/frame.h`), field `hw_frames_ctx`,
  submitted by the buffer-frame wave-3 type run.
- Related: `../../unsafe/frame-hw-frames-context-seam-left-untyped`.
- Status: resubmitted (`submission.json`, through `query types --name
  AVFrame --update`). The type's `_comment_agent` was kept verbatim, with
  one sentence appended.

## Previous finding

> NULL, or a reference the frame owns to the AVBufferRef of an
> AVHWFramesContext (...). Unref'd by av_frame_unref, ref'd by
> av_frame_ref, replaced by av_frame_replace; its data is read as
> AVHWFramesContext by av_frame_make_writable (av_hwframe_get_buffer),
> frame_copy_video (av_hwframe_transfer_data), vulkan.c and hardware codecs.
> Rust: FrameRef::hw_frames_context (byte view), take_hw_frames_context
> (safe), unsafe replace_hw_frames_context until hwcontext's
> AVHWFramesContext is wrapped.

## Why it was incomplete

The note names `frame_copy_video` as a reader of the context, but not what
follows from it. `av_frame_copy` (`frame.c:676`) passes any frame holding a
context to `av_hwframe_transfer_data` before it looks at the planes.
`av_frame_ref` and `av_frame_clone` copy every frame without `buf[0]`
through `av_frame_copy` (`frame.c:300-309`), and so does `av_frame_replace`
when such a frame has planes. So a frame that names a context without
holding that context's surfaces reaches the backend's `transfer_data_from`
from the reference routines. C calls it unchecked (`hwcontext.c:490`). It is
NULL for MediaCodec and OpenHarmony, and the other backends read the planes
as surfaces. Nothing in C ties a frame's buffers to the context it names.
The evidence is the reproducer in the related `unsafe/` report (ASan SEGV at
`hwcontext.c:490` from the safe `FrameRef::try_clone`). The note's Rust
part also described the seam this review replaced.

## Corrected finding

The ownership block is unchanged (owned, nullable, by-value scalar). The note
now records:

- the reference names an initialised context only;
- the copy path above and its NULL call;
- the typed Rust surface (`FramesReferenceRef` getter, safe
  `replace_hw_frames_context`/`take_hw_frames_context` over
  `FramesReferenceBox`);
- the refusal of copies of frames without `buf[0]` that hold a context.
