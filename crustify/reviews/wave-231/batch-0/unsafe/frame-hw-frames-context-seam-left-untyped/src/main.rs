//! `FrameMut::replace_hw_frames_context`'s contract, honoured, lets the safe
//! `FrameRef::try_clone` make C call a backend transfer routine on a frame
//! that is none of its frames context's surfaces.
//!
//! The contract asks only for a reference to an initialised frames context
//! whose bytes nothing writes. A frame that is not reference counted (no
//! `buf[0]`) but has a software format and a size is copied by
//! `av_frame_clone` -> `av_frame_ref`: `av_frame_get_buffer` allocates the
//! copy, then `av_frame_copy` sees the source's `hw_frames_ctx` and hands
//! the frame to `av_hwframe_transfer_data` (`frame.c:676`), which calls the
//! backend's `transfer_data_from` unchecked (`hwcontext.c:490`). The
//! MediaCodec and OpenHarmony backends have none (a NULL call, reproduced
//! here with the test backend B, which has none either); the others read
//! the frame's planes as surfaces (here: NULL).
//!
//! The campaign builds compile no backend, so the device comes from the
//! crate's own test backend (`tests/common/hwbackend.rs`), built exactly as
//! `av_hwdevice_ctx_alloc` builds a device; the frames context is made by
//! the safe `DeviceReferenceRef::create_frames`.
//!
//! `cargo run` (ASan/UBSan build) exits non-zero on the affected commit:
//! AddressSanitizer reports a SEGV at pc 0 (the NULL `transfer_data_from`).

use ffmpeg::libavutil::libavutil::buffer::BufferReferenceBox;
use ffmpeg::libavutil::libavutil::ffi;
use ffmpeg::libavutil::libavutil::frame::Frame;
use ffmpeg::libavutil::libavutil::hwcontext::DeviceReferenceBox;
use ffmpeg::libavutil::libavutil::pixfmt::PixelFormat;

#[path = "../../../../../../../tests/common/hwbackend.rs"]
mod hwbackend;

use hwbackend::Kind;

fn main() {
    // SAFETY: the only reference to an initialised test device context,
    // built as `av_hwdevice_ctx_alloc` builds one.
    let device = unsafe { DeviceReferenceBox::from_c(hwbackend::device(Kind::B)) }.unwrap();
    let frames = device
        .as_ref()
        .create_frames(PixelFormat::from_raw(Kind::B.format()), PixelFormat::NV12, 64, 32, 0)
        .expect("an initialised frames context");
    // SAFETY: a reference to an initialised frames context, handed over;
    // its data is the context's public struct, which nothing writes once
    // initialised (no Rust handle writes it, and the backend does not).
    let reference = unsafe { BufferReferenceBox::from_c(frames.into_c()) }.unwrap();

    let mut frame = Frame::new().unwrap();
    let mut m = frame.as_mut();
    m.set_pixel_format(PixelFormat::NV12);
    m.set_width(64);
    m.set_height(32);
    // SAFETY: `replace_hw_frames_context`'s contract: `reference` is a
    // reference to a hardware frames context `av_hwframe_ctx_alloc` made,
    // initialised (`av_hwframe_ctx_init` succeeded in `create_frames`), and
    // nothing writes its bytes while the frame or a copy holds it.
    let old = unsafe { m.replace_hw_frames_context(Some(reference)) };
    assert!(old.is_none());

    // Safe from here on.
    let copy = frame.as_ref().try_clone();
    eprintln!("try_clone: {}", if copy.is_some() { "Some" } else { "None" });
}
