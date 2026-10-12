//! UB tests for `libavutil/hwcontext.h`'s unsafe API, each call within its
//! `# Safety` contract, on the fake hardware frames context of
//! `tests/common/hwframes.rs` (the campaign builds compile no hwcontext
//! backend). Its transfers trust the software frame as CUDA's do, so a frame
//! the wrapper failed to refuse is overrun under ASan.

use ffmpeg::libavutil::ffi;

#[path = "../common/hwframes.rs"]
mod hwframes;

/// The fixture's contexts and hardware frames, adopted by the crate's
/// owners; software frames come from the safe API.
mod fixture {
    use core::ffi::c_int;

    use ffmpeg::libavutil::buffer::BufferReferenceBox;
    use ffmpeg::libavutil::frame::{Frame, FrameBox};
    use ffmpeg::libavutil::pixfmt::PixelFormat;

    use super::ffi;
    use super::hwframes::{self as fx, Behaviour, INERT, YUV, pattern};

    pub const W: c_int = 16;
    pub const H: c_int = 8;

    /// A fake context of YUV 4:2:0 surfaces at `W` x `H`.
    pub fn context(behaviour: &'static Behaviour) -> BufferReferenceBox {
        // SAFETY: a fresh reference the fixture handed over; its bytes, the
        // zeroed context, are initialised.
        unsafe { BufferReferenceBox::from_c(fx::yuv_context(behaviour, W, H)) }.unwrap()
    }

    /// A fake context of a backend without transfers.
    pub fn inert_context() -> BufferReferenceBox {
        let buf = fx::new_context(
            &INERT,
            &YUV,
            ffi::AVPixelFormat::AV_PIX_FMT_MEDIACODEC,
            ffi::AVPixelFormat::AV_PIX_FMT_YUV420P,
            W,
            H,
        );
        // SAFETY: as in `context`.
        unsafe { BufferReferenceBox::from_c(buf) }.unwrap()
    }

    /// A hardware frame of `ctx`, its surface holding `pattern(seed)`.
    pub fn hw_frame(ctx: &BufferReferenceBox, seed: u8) -> FrameBox {
        // SAFETY: a live fake context; the fresh frame satisfies `Frame`'s
        // invariant: its context reference is one, and `data[3]` is the start
        // of `buf[0]`'s window.
        unsafe { FrameBox::from_c(fx::new_hw_frame(ctx.as_c_ptr(), pattern(seed))) }.unwrap()
    }

    /// The image of the surface of `frame`, a hardware frame of a fake
    /// context.
    pub fn surface(frame: &FrameBox) -> Vec<u8> {
        // SAFETY: frames from `hw_frame`.
        unsafe { fx::surface_image(frame.as_c_ptr()) }
    }

    /// A software frame of `format` at `w` x `h`, given zeroed buffers.
    pub fn software(format: PixelFormat, w: c_int, h: c_int) -> FrameBox {
        let mut f = Frame::new().unwrap();
        let mut m = f.as_mut();
        m.set_pixel_format(format);
        m.set_width(w);
        m.set_height(h);
        m.alloc_buffers(0).unwrap();
        f
    }

    /// The `W` x `H` YUV 4:2:0 image of a software frame, row by row.
    pub fn image(frame: &FrameBox) -> Vec<u8> {
        let r = frame.as_ref();
        let ls = r.linesizes();
        let mut out = Vec::new();
        for (i, (w, h)) in [(W, H), (W / 2, H / 2), (W / 2, H / 2)]
            .into_iter()
            .enumerate()
        {
            let plane = r.plane(i).unwrap();
            for row in 0..h as usize {
                let at = row * ls[i] as usize;
                out.extend_from_slice(&plane[at..at + w as usize]);
            }
        }
        out
    }
}

/// `hwcontext::transfer_formats` on references to initialised (fake)
/// contexts: the lists are the caller's alone and outlive the context, each
/// failure the backend reports frees nothing it did not hand over, and
/// listing runs on several threads at once.
mod transfer_formats {
    use std::thread;

    use ffmpeg::libavutil::hwcontext::{TransferDirection, transfer_formats};
    use ffmpeg::libavutil::pixfmt::PixelFormat;

    use super::fixture::{context, hw_frame, inert_context};
    use super::hwframes::{EMPTY, GRAY_FIRST, LISTING_FAILS, YUV};

    const DIRS: [TransferDirection; 2] = [TransferDirection::From, TransferDirection::To];

    #[test]
    fn lists_outlive_the_context() {
        let ctx = context(&YUV);
        // SAFETY: a live reference to a fake context.
        let from = unsafe { transfer_formats(ctx.as_ref(), TransferDirection::From) }.unwrap();
        drop(ctx);
        assert_eq!(from.as_slice(), [PixelFormat::YUV420P, PixelFormat::GRAY8]);
        let copy = from.clone();
        drop(from);
        assert_eq!(copy.as_slice()[1], PixelFormat::GRAY8);
    }

    #[test]
    fn every_reference_to_a_context_lists_alike() {
        let ctx = context(&GRAY_FIRST);
        let frame = hw_frame(&ctx, 0);
        let other = ctx.as_ref().try_clone().unwrap();
        for dir in DIRS {
            // SAFETY: live references to one fake context.
            let (a, b) = unsafe {
                (
                    transfer_formats(ctx.as_ref(), dir).unwrap(),
                    transfer_formats(other.as_ref(), dir).unwrap(),
                )
            };
            assert_eq!(a.as_slice(), b.as_slice());
            let c = frame.as_ref().transfer_formats(dir).unwrap();
            assert_eq!(a.as_slice(), c.as_slice());
        }
    }

    #[test]
    fn failures_and_empty_lists_free_nothing_twice() {
        let empty = context(&EMPTY);
        let failing = context(&LISTING_FAILS);
        let inert = inert_context();
        for dir in DIRS {
            for _ in 0..3 {
                // SAFETY: live references to fake contexts.
                unsafe {
                    assert!(
                        transfer_formats(empty.as_ref(), dir)
                            .unwrap()
                            .as_slice()
                            .is_empty()
                    );
                    assert!(transfer_formats(failing.as_ref(), dir).is_err());
                    assert!(transfer_formats(inert.as_ref(), dir).is_err());
                }
            }
        }
    }

    #[test]
    fn lists_on_many_threads() {
        let ctx = context(&YUV);
        let frame = hw_frame(&ctx, 0);
        thread::scope(|s| {
            for t in 0..8 {
                let (ctx, frame) = (&ctx, &frame);
                s.spawn(move || {
                    for i in 0..50 {
                        let dir = DIRS[(t + i) % 2];
                        // SAFETY: a live reference to a fake context, which
                        // nothing writes.
                        let a = unsafe { transfer_formats(ctx.as_ref(), dir) }.unwrap();
                        let b = frame.as_ref().transfer_formats(dir).unwrap();
                        assert_eq!(a.as_slice(), b.as_slice());
                    }
                });
            }
        });
    }
}

/// `FrameMut::transfer_data_from` with every hardware frame one of its
/// (fake) context: software frames whose planes do not hold the surfaces'
/// image, whose buffers are shared or read-only, display sizes past the
/// surfaces, unlisted formats and backends without transfers are all
/// refused before the trusting backend overruns anything; frames receiving
/// new buffers outlive their source and release what they held; parents
/// dropped first; transfers on several threads; Rust views of planes held
/// across a transfer that reads them and taken after one that writes them.
mod frame_mut_transfer_data_from {
    use core::ffi::c_int;
    use std::thread;

    use ffmpeg::libavutil::buffer::BufferReference;
    use ffmpeg::libavutil::dict::Dict;
    use ffmpeg::libavutil::frame::{Frame, FrameBox};
    use ffmpeg::libavutil::pixfmt::PixelFormat;

    use super::ffi;
    use super::fixture::{H, W, context, hw_frame, image, inert_context, software, surface};
    use super::hwframes::YUV;

    const EINVAL: c_int = -(ffi::EINVAL as c_int);
    const ENOSYS: c_int = -(ffi::ENOSYS as c_int);

    /// `dst.transfer_data_from(src)`, every hardware frame here being one of
    /// its fake context.
    fn transfer(dst: &mut FrameBox, src: &FrameBox) -> Result<(), c_int> {
        // SAFETY: every hardware frame in this suite comes from `hw_frame`.
        unsafe { dst.as_mut().transfer_data_from(src.as_ref()) }
    }

    /// A `W` x `H` YUV 4:2:0 frame whose planes lie back to back, with line
    /// sizes of their row widths, in one zeroed buffer of `size` bytes.
    fn packed(size: usize) -> FrameBox {
        let mut f = Frame::new().unwrap();
        let mut m = f.as_mut();
        m.set_pixel_format(PixelFormat::YUV420P);
        m.set_width(W);
        m.set_height(H);
        m.set_buffer(0, Some(BufferReference::alloc_zeroed(size).unwrap()));
        m.set_linesizes([W, W / 2, W / 2, 0, 0, 0, 0, 0]);
        let luma = (W * H) as usize;
        for (plane, offset) in [(0, 0), (1, luma), (2, luma + luma / 4)] {
            m.set_plane(plane, 0, offset).unwrap();
        }
        f
    }

    const EXACT: usize = (W * H + W * H / 2) as usize;

    #[test]
    fn planes_one_byte_short_of_the_image_are_refused_both_ways() {
        let ctx = context(&YUV);
        let mut hw = hw_frame(&ctx, 1);
        let mut exact = packed(EXACT);
        transfer(&mut exact, &hw).unwrap();
        assert_eq!(image(&exact), surface(&hw));
        let mut short = packed(EXACT - 1);
        assert_eq!(transfer(&mut short, &hw), Err(EINVAL));
        assert_eq!(transfer(&mut hw, &short), Err(EINVAL));
        transfer(&mut hw, &exact).unwrap();
    }

    #[test]
    fn line_sizes_reaching_past_the_buffer_are_refused() {
        let ctx = context(&YUV);
        let mut hw = hw_frame(&ctx, 2);
        for (plane, linesize) in [
            (0, 2 * W),
            (1, 2 * W),
            (2, W),
            (0, -W),
            (1, 0),
            (0, c_int::MAX),
        ] {
            let mut f = packed(EXACT);
            f.as_mut().set_linesize(plane, linesize);
            assert_eq!(transfer(&mut f, &hw), Err(EINVAL), "{plane}: {linesize}");
            assert_eq!(transfer(&mut hw, &f), Err(EINVAL), "{plane}: {linesize}");
        }
    }

    #[test]
    fn missing_planes_are_refused() {
        let ctx = context(&YUV);
        let hw = hw_frame(&ctx, 3);
        for plane in 0..3 {
            let mut f = software(PixelFormat::YUV420P, W, H);
            f.as_mut().clear_plane(plane);
            assert_eq!(transfer(&mut f, &hw), Err(EINVAL), "{plane}");
        }
    }

    #[test]
    fn shared_and_read_only_destinations_are_refused_and_left_alone() {
        let ctx = context(&YUV);
        let hw = hw_frame(&ctx, 4);
        let mut dst = software(PixelFormat::YUV420P, W, H);
        let other = dst.as_ref().try_clone().unwrap();
        let seen = other.as_ref().plane(0).unwrap();
        assert_eq!(transfer(&mut dst, &hw), Err(EINVAL));
        assert!(seen.iter().all(|&b| b == 0));
        // The same planes read for an upload are fine.
        let mut up = hw_frame(&ctx, 0);
        transfer(&mut up, &dst).unwrap();
        drop(other);
        transfer(&mut dst, &hw).unwrap();

        let mut ro = packed(EXACT);
        let bytes = vec![0u8; EXACT];
        ro.as_mut()
            .set_buffer(0, Some(BufferReference::from_read_only(bytes).unwrap()));
        let luma = (W * H) as usize;
        for (plane, offset) in [(0, 0), (1, luma), (2, luma + luma / 4)] {
            ro.as_mut().set_plane(plane, 0, offset).unwrap();
        }
        assert_eq!(transfer(&mut ro, &hw), Err(EINVAL));
        assert!(ro.as_ref().plane(0).unwrap().iter().all(|&b| b == 0));
        transfer(&mut up, &ro).unwrap();
    }

    #[test]
    fn display_sizes_past_the_surfaces_are_refused() {
        let ctx = context(&YUV);
        for (w, h) in [
            (W, H + 1),
            (W + 1, H),
            (W, 4 * H),
            (-1, H),
            (W, -1),
            (W, c_int::MIN),
        ] {
            let mut hw = hw_frame(&ctx, 5);
            hw.as_mut().set_width(w);
            hw.as_mut().set_height(h);
            let mut dst = software(PixelFormat::YUV420P, W, H);
            assert_eq!(transfer(&mut dst, &hw), Err(EINVAL), "{w}x{h}");
            let mut clean = Frame::new().unwrap();
            assert_eq!(transfer(&mut clean, &hw), Err(EINVAL), "{w}x{h}");
            let mut other = hw_frame(&ctx, 6);
            assert_eq!(transfer(&mut other, &hw), Err(EINVAL), "{w}x{h}");
            let good = hw_frame(&ctx, 7);
            let mut sw = software(PixelFormat::YUV420P, W, H);
            sw.as_mut().set_width(w);
            sw.as_mut().set_height(h);
            assert_eq!(transfer(&mut hw, &sw), Err(EINVAL), "{w}x{h}");
            let mut fresh = hw_frame(&ctx, 8);
            assert_eq!(transfer(&mut fresh, &sw), Err(EINVAL), "{w}x{h}");
            drop(good);
        }
    }

    #[test]
    fn unlisted_and_unknown_formats_are_refused() {
        let ctx = context(&YUV);
        let mut hw = hw_frame(&ctx, 9);
        for format in [PixelFormat::YUV444P, PixelFormat::RGB24, PixelFormat::NV12] {
            let mut sw = software(format, W, H);
            assert_eq!(transfer(&mut sw, &hw), Err(EINVAL), "{format:?}");
            assert_eq!(transfer(&mut hw, &sw), Err(EINVAL), "{format:?}");
        }
        // Luma alone is listed for downloads only.
        let mut gray = software(PixelFormat::GRAY8, W, H);
        transfer(&mut gray, &hw).unwrap();
        assert_eq!(transfer(&mut hw, &gray), Err(EINVAL));
        for format in [
            PixelFormat::NONE,
            PixelFormat::VAAPI,
            PixelFormat::from_raw(ffi::AVPixelFormat(9999)),
        ] {
            let mut f = packed(EXACT);
            f.as_mut().set_pixel_format(format);
            assert_eq!(transfer(&mut f, &hw), Err(EINVAL), "{format:?}");
            let mut clean = Frame::new().unwrap();
            clean.as_mut().set_pixel_format(format);
            // A frame without buffers and without a format takes the first
            // listed one.
            let expected = if format == PixelFormat::NONE {
                Ok(())
            } else {
                Err(EINVAL)
            };
            assert_eq!(transfer(&mut clean, &hw), expected, "{format:?}");
        }
    }

    #[test]
    fn backends_without_transfers_are_refused() {
        let inert = inert_context();
        let ctx = context(&YUV);
        let mut src = hw_frame(&inert, 0);
        let mut sw = software(PixelFormat::YUV420P, W, H);
        let mut clean = Frame::new().unwrap();
        clean.as_mut().set_pixel_format(PixelFormat::YUV420P);
        let mut other = hw_frame(&ctx, 0);
        assert_eq!(transfer(&mut sw, &src), Err(ENOSYS));
        assert_eq!(transfer(&mut clean, &src), Err(ENOSYS));
        assert_eq!(transfer(&mut src, &sw), Err(ENOSYS));
        assert_eq!(transfer(&mut other, &src), Err(ENOSYS));
        assert_eq!(transfer(&mut src, &other), Err(ENOSYS));
    }

    #[test]
    fn new_buffers_outlive_the_source_and_its_context() {
        let ctx = context(&YUV);
        let mut hw = hw_frame(&ctx, 10);
        hw.as_mut().set_width(5);
        hw.as_mut().set_height(3);
        let expected = surface(&hw);
        let mut dst = Frame::new().unwrap();
        transfer(&mut dst, &hw).unwrap();
        drop(hw);
        drop(ctx);
        let r = dst.as_ref();
        assert_eq!((r.width(), r.height()), (5, 3));
        // The whole surface was copied: C allocates at the surfaces' size.
        r.plane(0).unwrap();
        let mut full = dst;
        full.as_mut().set_width(W);
        full.as_mut().set_height(H);
        let luma = (W * 3) as usize;
        assert_eq!(image(&full)[..luma], expected[..luma]);
    }

    #[test]
    fn new_buffers_release_what_the_frame_held() {
        let ctx = context(&YUV);
        let hw = hw_frame(&ctx, 11);
        let mut dst = Frame::new().unwrap();
        let mut m = dst.as_mut();
        let mut dict = Dict::new();
        dict.set(c"key", c"value", Default::default()).unwrap();
        m.set_metadata(dict);
        m.set_opaque_ref(Some(BufferReference::alloc_zeroed(64).unwrap()));
        m.set_pixel_format(PixelFormat::YUV420P);
        // SAFETY: the reference is one to a fake context; the frame holds no
        // buffers, so it is no hardware frame C would read.
        let old = unsafe { m.replace_hw_frames_context(Some(ctx.as_ref().try_clone().unwrap())) };
        assert!(old.is_none());
        transfer(&mut dst, &hw).unwrap();
        let r = dst.as_ref();
        assert!(r.metadata().is_none() && r.opaque_ref().is_none());
        assert!(r.hw_frames_context().is_none());
        assert_eq!(image(&dst), surface(&hw));
    }

    #[test]
    fn contexts_dropped_before_their_frames() {
        let ctx = context(&YUV);
        let mut a = hw_frame(&ctx, 12);
        let b = hw_frame(&ctx, 13);
        drop(ctx);
        transfer(&mut a, &b).unwrap();
        assert_eq!(surface(&a), surface(&b));
        let mut sw = software(PixelFormat::YUV420P, W, H);
        transfer(&mut sw, &a).unwrap();
        drop(b);
        drop(a);
        assert_ne!(image(&sw), vec![0; image(&sw).len()]);
    }

    #[test]
    fn transfers_on_many_threads() {
        let ctx = context(&YUV);
        let src = hw_frame(&ctx, 14);
        let expected = surface(&src);
        thread::scope(|s| {
            for _ in 0..8 {
                let (src, ctx, expected) = (&src, &ctx, &expected);
                s.spawn(move || {
                    for _ in 0..20 {
                        let mut down = Frame::new().unwrap();
                        transfer(&mut down, src).unwrap();
                        assert_eq!(&image(&down), expected);
                        let mut up = hw_frame(ctx, 0);
                        transfer(&mut up, &down).unwrap();
                        assert_eq!(&surface(&up), expected);
                    }
                });
            }
        });
    }

    /// BSan: C reads the source's planes through pointers Rust handed out
    /// while a Rust view of them is alive.
    #[test]
    fn upload_reads_planes_rust_is_viewing() {
        let ctx = context(&YUV);
        let mut hw = hw_frame(&ctx, 0);
        let mut sw = software(PixelFormat::YUV420P, W, H);
        sw.as_mut().plane_mut(0).unwrap()[..4].copy_from_slice(&[1, 2, 3, 4]);
        let view = sw.as_ref().plane(0).unwrap();
        transfer(&mut hw, &sw).unwrap();
        assert_eq!(view[..4], [1, 2, 3, 4]);
        assert_eq!(surface(&hw)[..4], [1, 2, 3, 4]);
    }

    /// BSan: Rust reborrows planes C has just written, between writes.
    #[test]
    fn download_writes_then_rust_reads_and_writes_again() {
        let ctx = context(&YUV);
        let a = hw_frame(&ctx, 20);
        let b = hw_frame(&ctx, 21);
        let mut sw = software(PixelFormat::YUV420P, W, H);
        transfer(&mut sw, &a).unwrap();
        let first = sw.as_ref().plane(0).unwrap()[..W as usize].to_vec();
        sw.as_mut().plane_mut(0).unwrap()[0] ^= 0xff;
        transfer(&mut sw, &b).unwrap();
        let second = sw.as_ref().plane(0).unwrap()[..W as usize].to_vec();
        assert_eq!(first[..], surface(&a)[..W as usize]);
        assert_eq!(second[..], surface(&b)[..W as usize]);
        assert_ne!(first, second);
    }
}
