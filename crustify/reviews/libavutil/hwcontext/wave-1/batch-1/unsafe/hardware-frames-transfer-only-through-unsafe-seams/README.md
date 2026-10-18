# Unsafe boundary: hardware frames transfer only through `unsafe` seams

- Items: `av_hwframe_transfer_data` and `av_hwframe_transfer_get_formats`
  (translation batch 2, `bf7c454c03`), next to `av_hwframe_get_buffer` and
  `av_hwframe_map` (translation batch 1, `62b80e7a1d`), all homed in
  `crustify/rust/ffmpeg/libavutil/hwcontext.rs`.
- Affected commit: the wave tip `b9dbecd340`.
- Status: fixed. The safe forms were added and the obsolete `unsafe` free
  function was removed.

## Finding

The wave's batches 1 and 2 ran in parallel. Batch 2 wrapped the transfers
before batch 1's typed owners existed, so it left two `unsafe` entry points
and said in their docs that they would be replaced "once hwcontext.h's frames
contexts and their frames are wrapped". Batch 1 then landed exactly those
types, `FramesReferenceRef`/`FramesReferenceBox` and `HardwareFrame`. Nobody
connected the two. At the wave tip:

- `hwcontext::transfer_formats(frames_context: BufferReferenceRef<'_>, dir)`
  was a public `unsafe fn`. Its only obligation was that the reference
  points at an initialised frames context. Every `FramesReferenceRef`
  carries that guarantee in its type invariant ("holds one count of a frames
  context that `av_hwframe_ctx_alloc` allocated ... and that was
  initialised"). Even so, `FramesReferenceRef` had no listing, so a safe
  caller holding a context could not ask which formats it transfers.
- `HardwareFrame`, from `FramesReferenceRef::get_buffer` and
  `HardwareFrame::map_to`, is exactly the frame
  `FrameMut::transfer_data_from`'s `# Safety` asks for ("its buffers and
  plane pointers are surfaces the context's backend allocated or mapped for
  it, and left as it made them"). Yet it lends no `FrameRef`/`FrameMut`, so
  that its surface handles are not read as bytes, and it had no transfer
  method. A hardware frame allocated through the safe API could therefore
  not be downloaded, uploaded or copied at all, safely or not. Its docs
  pointed at `av_hwframe_transfer_data` by its C name. The memory notes of
  the campaign record the gap ("a safe `HardwareFrame` transfer over
  batch 2's seam is still missing").

`scan-unsafe --sites unsafe_fns_pub_smell` lists both seams:
`hwcontext.rs:367` (`transfer_formats`) and `:554`
(`transfer_data_from`).

## Fix

- `FramesReferenceRef::transfer_formats(&self, dir)`: safe, through a
  private `hw_frames()` view of the reference. The view is a
  `BufferReferenceRef` over the same `AVBufferRef`. The context is C's
  wholly initialised `av_mallocz` block, and nothing writes it after init.
  The unsafe free `hwcontext::transfer_formats` is removed: its one
  precondition is now a type invariant. Hardware frames reach the listing
  through `HardwareFrame::frames().transfer_formats(..)`, and plain frames
  through the existing safe `FrameRef::transfer_formats`.
- `HardwareFrame::download_to(&self, FrameMut<'_>)`,
  `HardwareFrame::upload_from(&mut self, FrameRef<'_>)` and
  `HardwareFrame::copy_from(&mut self, &HardwareFrame)`: safe. Each runs
  `transfer_data_from`'s checks and its allocating-download path. The
  hardware side's obligation comes from `HardwareFrame`'s invariant. A
  software side that holds a frames context is refused with
  `AVERROR(EINVAL)`, because it cannot be vouched for (`Frame` cannot tell
  a hardware frame from a software frame given a context). A
  `compile_fail,E0502` doctest pins that a frame cannot copy itself.
- `FrameMut::transfer_data_from` stays `unsafe`, for hardware frames that C
  hands over as plain `Frame`s (hardware decoders and filters, which
  later wrappers should hand out as `HardwareFrame`). Its `# Safety` now
  names the safe forms. The module docs were rewritten to match.

## Evidence and regressions

- The test backend `tests/common/hwbackend.rs` kind A (VAAPI-like) gained
  transfers. `transfer_get_formats` lists the context's software format.
  `transfer_data_to`/`_from` copy surface to surface, or run over the
  software frame's image at the context's size, unchecked, as CUDA trusts
  it. A configurable `transfer_ret` and an `Event::Transfer` log entry
  were added. Kind B still transfers nothing.
- Unit tests (`hwcontext::unit_tests::contexts`):
  `frames_list_their_backends_transfer_formats`,
  `hardware_frames_upload_download_and_copy` (checked against a model of
  the backend) and `hardware_frame_transfers_refuse_before_c`. The last one
  covers a software side holding a context, unlisted formats, display sizes
  past the surfaces, a shared destination, kind B's NULL transfers (C would
  call through NULL), the backend's error leaving a buffer-less frame
  unchanged, and C's refusal of copies into or out of a derived context.
- Equivalence (`tests/equiv/hwcontext.rs::hardware_frame_transfers`):
  `format_listings_match_c`, `transfer_sequences_match_c` (three rounds of
  upload, download into buffers and into a fresh frame, hardware copy and
  download of the copy, comparing results, images, shapes and event logs
  after each step) and `backend_failures_and_derived_copies_match_c`.
- The four `ub_unsafe` tests of the removed function now list through
  `FrameRef::transfer_formats` on the fixture's hardware frames
  (`ub_unsafe/hwcontext.rs::frame_ref_transfer_formats`), threads included.
  The equivalence listing test does the same.
- `unsafe_fns_pub_smell` 24 -> 23 crate-wide. The remaining public `unsafe`
  sites in the home are `transfer_data_from`, `UninitDevice::init`,
  `DeviceContextRef::hwctx` and `DeviceContextMut::into_hwctx`, all with
  obligations that no type can express yet.
