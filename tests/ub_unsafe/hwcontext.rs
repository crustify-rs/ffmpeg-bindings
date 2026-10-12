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

/// `FrameRef::transfer_formats` on hardware frames of initialised (fake)
/// contexts, which only the unsafe adoption of the fixture's frames makes
/// (`ub_safe` has no hardware frame to list for): the lists are the
/// caller's alone and outlive the frame and its context, each failure the
/// backend reports frees nothing it did not hand over, and listing runs on
/// several threads at once.
mod frame_ref_transfer_formats {
    use std::thread;

    use ffmpeg::libavutil::hwcontext::TransferDirection;
    use ffmpeg::libavutil::pixfmt::PixelFormat;

    use super::fixture::{context, hw_frame, inert_context};
    use super::hwframes::{EMPTY, GRAY_FIRST, LISTING_FAILS, YUV};

    const DIRS: [TransferDirection; 2] = [TransferDirection::From, TransferDirection::To];

    #[test]
    fn lists_outlive_the_frame_and_context() {
        let ctx = context(&YUV);
        let frame = hw_frame(&ctx, 0);
        let from = frame.as_ref().transfer_formats(TransferDirection::From);
        drop((frame, ctx));
        let from = from.unwrap();
        assert_eq!(from.as_slice(), [PixelFormat::YUV420P, PixelFormat::GRAY8]);
        let copy = from.clone();
        drop(from);
        assert_eq!(copy.as_slice()[1], PixelFormat::GRAY8);
    }

    #[test]
    fn every_frame_of_a_context_lists_alike() {
        let ctx = context(&GRAY_FIRST);
        let frame = hw_frame(&ctx, 0);
        let other = ctx.as_ref().try_clone().unwrap();
        let other_frame = hw_frame(&other, 1);
        let copy = frame.as_ref().try_clone();
        for dir in DIRS {
            let a = frame.as_ref().transfer_formats(dir).unwrap();
            let b = other_frame.as_ref().transfer_formats(dir).unwrap();
            assert_eq!(a.as_slice(), b.as_slice());
            if let Some(copy) = &copy {
                let c = copy.as_ref().transfer_formats(dir).unwrap();
                assert_eq!(a.as_slice(), c.as_slice());
            }
        }
    }

    #[test]
    fn failures_and_empty_lists_free_nothing_twice() {
        let (empty, failing, inert) = (context(&EMPTY), context(&LISTING_FAILS), inert_context());
        let frames = [
            hw_frame(&empty, 0),
            hw_frame(&failing, 0),
            hw_frame(&inert, 0),
        ];
        drop((empty, failing, inert));
        for dir in DIRS {
            for _ in 0..3 {
                let listed = frames[0].as_ref().transfer_formats(dir);
                assert!(listed.unwrap().as_slice().is_empty());
                assert!(frames[1].as_ref().transfer_formats(dir).is_err());
                assert!(frames[2].as_ref().transfer_formats(dir).is_err());
            }
        }
    }

    #[test]
    fn lists_on_many_threads() {
        let ctx = context(&YUV);
        let (frame, other) = (hw_frame(&ctx, 0), hw_frame(&ctx, 1));
        thread::scope(|s| {
            for t in 0..8 {
                let (frame, other) = (&frame, &other);
                s.spawn(move || {
                    for i in 0..50 {
                        let dir = DIRS[(t + i) % 2];
                        let a = frame.as_ref().transfer_formats(dir).unwrap();
                        let b = other.as_ref().transfer_formats(dir).unwrap();
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

#[path = "../common/hwdevice.rs"]
mod hwdevice;

/// UB tests for the unsafe public API the `AVHWDeviceContext` wrapper adds or
/// makes unsafe, each called within its `# Safety` contract (hwcontext wave
/// 1, batch 0): [`UninitDevice::from_c`], [`UninitDevice::init`],
/// [`DeviceContextRef::hwctx`] and [`DeviceContextMut::into_hwctx`].
///
/// The campaign builds compile no hardware backend, so every context comes
/// from the fake backend of `tests/common/hwdevice.rs`: what
/// `av_hwdevice_ctx_alloc` would return for it, with C's own
/// `av_hwdevice_ctx_init` and `av_hwdevice_get_hwframe_constraints` running
/// on it.
///
/// [`UninitDevice::from_c`]: ffmpeg::libavutil::hwcontext::UninitDevice::from_c
/// [`UninitDevice::init`]: ffmpeg::libavutil::hwcontext::UninitDevice::init
/// [`DeviceContextRef::hwctx`]: ffmpeg::libavutil::hwcontext::DeviceContextRef::hwctx
/// [`DeviceContextMut::into_hwctx`]: ffmpeg::libavutil::hwcontext::DeviceContextMut::into_hwctx
mod device_context {
    use core::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::thread;

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::hwcontext::{
        DeviceContextMut, DeviceReferenceBox, DeviceType, UninitDevice,
    };
    use ffmpeg::libavutil::pixfmt::PixelFormat;

    use super::hwdevice::{self, BARE_TYPE, Events, FakeConfig, FakeHw};

    /// A fresh fake context under construction, counting into `events`.
    fn uninit(events: &Arc<Events>) -> UninitDevice {
        // SAFETY: the only reference to a fresh, uninitialised fake context
        // with no hooks.
        let mut u = unsafe { UninitDevice::from_c(hwdevice::fake_device()) }.unwrap();
        // SAFETY: the fake context's hardware context is a `FakeHwctx`; the
        // events outlive the context, as each test keeps them.
        unsafe { u.context_mut().into_hwctx::<FakeHw>() }
            .unwrap()
            .set_events(Arc::as_ptr(events));
        u
    }

    /// An initialised fake context holding `value`.
    fn device(events: &Arc<Events>, value: i64) -> DeviceReferenceBox {
        let mut u = uninit(events);
        // SAFETY: as in `uninit`; any payload is valid for the fake backend.
        unsafe { u.context_mut().into_hwctx::<FakeHw>() }
            .unwrap()
            .set_value(value);
        // SAFETY: the fake backend's `device_init` reads nothing it cannot
        // take.
        unsafe { u.init() }.unwrap()
    }

    /// A value that counts its drops.
    struct Dropped(Arc<AtomicUsize>);
    impl Drop for Dropped {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    /// A reference handed to C, which another thread releases: C may drop a
    /// device's last reference on any thread (a codec's frame threads).
    struct CReference(*mut ffi::AVBufferRef);
    // SAFETY: C counts references atomically, and the device's release runs
    // the fake backend's thread-agnostic uninit and the `Send` Rust hooks.
    unsafe impl Send for CReference {}
    impl CReference {
        fn release(mut self) {
            // SAFETY: a reference C owns, released once.
            unsafe { ffi::av_buffer_unref(&mut self.0) };
        }
    }

    /// [`UninitDevice::from_c`]: adopting fresh contexts, with and without a
    /// hardware context or a backend's hooks, and handing them back.
    mod uninit_from_c {
        use super::*;

        #[test]
        fn adopted_contexts_release_once() {
            let events = Arc::new(Events::default());
            for _ in 0..16 {
                let u = uninit(&events);
                assert_eq!(u.context().device_type(), DeviceType::VAAPI);
                drop(u);
            }
            assert_eq!(events.get(), (0, 16, 0));
        }

        #[test]
        fn null_is_refused() {
            // SAFETY: NULL is within the contract.
            assert!(unsafe { UninitDevice::from_c(core::ptr::null_mut()) }.is_none());
        }

        #[test]
        fn round_trips_through_c_keep_the_hooks() {
            let events = Arc::new(Events::default());
            let drops = Arc::new(AtomicUsize::new(0));
            let mut u = uninit(&events);
            u.context_mut()
                .set_user_data(Dropped(drops.clone()))
                .ok()
                .unwrap();
            for _ in 0..4 {
                let raw = u.into_c();
                // SAFETY: the reference `into_c` gave up, still uninitialised,
                // with hooks this crate set.
                u = unsafe { UninitDevice::from_c(raw) }.unwrap();
            }
            assert!(u.context().user_data::<Dropped>().is_some());
            drop(u);
            assert_eq!(drops.load(Ordering::SeqCst), 1);
        }

        #[test]
        fn backend_hooks_stay_the_backends() {
            let freed = AtomicUsize::new(0);
            let raw = hwdevice::fake_device();
            // SAFETY: the only reference to a fresh fake context; `freed`
            // outlives it.
            unsafe { hwdevice::set_foreign_free(raw, &freed) };
            // SAFETY: an uninitialised fake context whose hooks a backend set,
            // as the contract allows.
            let mut u = unsafe { UninitDevice::from_c(raw) }.unwrap();
            assert!(u.context_mut().set_user_data(0u8).is_err());
            assert!(u.context().has_free_callback());
            drop(u);
            assert_eq!(freed.load(Ordering::SeqCst), 1);
        }

        #[test]
        fn bare_contexts_have_no_hardware_context() {
            let raw =
                hwdevice::fake_device_of(&BARE_TYPE, ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_CUDA);
            // SAFETY: the only reference to a fresh, uninitialised bare context.
            let mut u = unsafe { UninitDevice::from_c(raw) }.unwrap();
            assert_eq!(u.context().device_type(), DeviceType::CUDA);
            // SAFETY: there is no hardware context to mistype.
            assert!(unsafe { u.context_mut().into_hwctx::<FakeHw>() }.is_none());
            u.context_mut().set_user_data(String::from("bare")).unwrap();
            let data = u.context().user_data::<String>().map(String::as_str);
            assert_eq!(data, Some("bare"));
        }
    }

    /// [`UninitDevice::init`]: success and failure, with every hook form, the
    /// last release on other threads, and constraints outliving the device.
    mod uninit_init {
        use super::*;

        #[test]
        fn success_shares_the_filled_context() {
            let events = Arc::new(Events::default());
            let device = device(&events, 9);
            // SAFETY: the fake context's hardware context is a `FakeHwctx`,
            // which nothing writes after initialisation.
            let hw = unsafe { device.as_ref().context().hwctx::<FakeHw>() }.unwrap();
            assert!(hw.initialised());
            assert_eq!(hw.value(), 9);
            drop(device);
            assert_eq!(events.get(), (1, 1, 0));
        }

        #[test]
        fn failure_releases_the_context_and_its_hooks() {
            let events = Arc::new(Events::default());
            let drops = Arc::new(AtomicUsize::new(0));
            let mut u = uninit(&events);
            // SAFETY: as in `uninit`; the fake backend fails as told.
            unsafe { u.context_mut().into_hwctx::<FakeHw>() }
                .unwrap()
                .set_init_ret(-38);
            u.context_mut()
                .set_user_data(Dropped(drops.clone()))
                .ok()
                .unwrap();
            let d = drops.clone();
            u.context_mut()
                .set_free_callback(move |ctx: DeviceContextMut<'_>| {
                    assert!(ctx.as_ref().user_data::<Dropped>().is_some());
                    assert_eq!(d.load(Ordering::SeqCst), 0);
                })
                .ok()
                .unwrap();
            // SAFETY: the fake backend's `device_init` reads nothing it cannot
            // take.
            assert_eq!(unsafe { u.init() }.unwrap_err(), -38);
            assert_eq!(events.get(), (1, 1, 0));
            assert_eq!(drops.load(Ordering::SeqCst), 1);
        }

        #[test]
        fn bare_contexts_initialise_without_a_backend_call() {
            let raw =
                hwdevice::fake_device_of(&BARE_TYPE, ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_DRM);
            // SAFETY: the only reference to a fresh, uninitialised bare context.
            let u = unsafe { UninitDevice::from_c(raw) }.unwrap();
            // SAFETY: the bare backend has no `device_init`.
            let device = unsafe { u.init() }.unwrap();
            assert!(device.as_ref().frames_constraints().is_none());
        }

        #[test]
        fn hooks_run_once_when_c_drops_the_last_references_elsewhere() {
            let events = Arc::new(Events::default());
            for _ in 0..8 {
                let calls = Arc::new(AtomicUsize::new(0));
                let drops = Arc::new(AtomicUsize::new(0));
                let mut u = uninit(&events);
                u.context_mut()
                    .set_user_data(Dropped(drops.clone()))
                    .ok()
                    .unwrap();
                let c = calls.clone();
                u.context_mut()
                    .set_free_callback(move |ctx: DeviceContextMut<'_>| {
                        assert!(ctx.as_ref().user_data::<Dropped>().is_some());
                        c.fetch_add(1, Ordering::SeqCst);
                    })
                    .ok()
                    .unwrap();
                // SAFETY: as in `device`.
                let device = unsafe { u.init() }.unwrap();
                let refs: Vec<_> = (0..4)
                    .map(|_| CReference(device.as_ref().try_clone().unwrap().into_c()))
                    .collect();
                drop(device);
                thread::scope(|s| {
                    for r in refs {
                        s.spawn(move || r.release());
                    }
                });
                assert_eq!(calls.load(Ordering::SeqCst), 1);
                assert_eq!(drops.load(Ordering::SeqCst), 1);
            }
            assert_eq!(events.get(), (8, 8, 0));
        }

        #[test]
        fn constraints_outlive_the_device() {
            let events = Arc::new(Events::default());
            let device = device(&events, 0);
            let mut config = device.as_ref().alloc_hw_config().unwrap();
            // SAFETY: the fake backend's zeroed configuration, owned by
            // `config`.
            unsafe {
                let p = config.as_mut_ptr().cast::<FakeConfig>();
                (*p).hw_format = ffi::AVPixelFormat::AV_PIX_FMT_VULKAN;
            }
            let c = device.as_ref().frames_constraints_for_config(&config);
            let plain = device.as_ref().frames_constraints();
            drop((device, config));
            assert_eq!(events.get().1, 1);
            let hw: Vec<_> = c
                .unwrap()
                .as_ref()
                .valid_hw_formats()
                .unwrap()
                .elems()
                .collect();
            assert_eq!(hw, [PixelFormat::VULKAN]);
            assert!(plain.unwrap().as_ref().valid_hw_formats().is_some());
        }
    }

    /// [`DeviceContextRef::hwctx`]: typed shared reads from uninitialised
    /// and initialised contexts, through several handles at once.
    ///
    /// [`DeviceContextRef::hwctx`]: ffmpeg::libavutil::hwcontext::DeviceContextRef::hwctx
    mod ref_hwctx {
        use super::*;

        #[test]
        fn reads_follow_the_owner() {
            let events = Arc::new(Events::default());
            let u = uninit(&events);
            // SAFETY: the fake context's hardware context is a `FakeHwctx`.
            let hw = unsafe { u.context().hwctx::<FakeHw>() }.unwrap();
            assert_eq!((hw.value(), hw.initialised()), (0, false));
            let device = device(&events, -5);
            let copy = device.clone();
            let (a, b) = (device.as_ref().context(), copy.as_ref().context());
            // SAFETY: as above, unwritten after initialisation.
            let ha = unsafe { a.hwctx::<FakeHw>() }.unwrap();
            // SAFETY: as above.
            let hb = unsafe { b.hwctx::<FakeHw>() }.unwrap();
            assert_eq!((ha.value(), hb.value()), (-5, -5));
        }

        #[test]
        fn bare_contexts_read_none() {
            let raw =
                hwdevice::fake_device_of(&BARE_TYPE, ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_QSV);
            // SAFETY: the only reference to a fresh, uninitialised bare context.
            let u = unsafe { UninitDevice::from_c(raw) }.unwrap();
            // SAFETY: there is no hardware context to mistype.
            assert!(unsafe { u.context().hwctx::<FakeHw>() }.is_none());
        }
    }

    /// [`DeviceContextMut::into_hwctx`]: writes before initialisation that C
    /// reads, through reborrows, and inside the free callback.
    ///
    /// [`DeviceContextMut::into_hwctx`]: ffmpeg::libavutil::hwcontext::DeviceContextMut::into_hwctx
    mod mut_into_hwctx {
        use super::*;

        #[test]
        fn writes_reach_the_backend() {
            let events = Arc::new(Events::default());
            let mut u = uninit(&events);
            let mut ctx = u.context_mut();
            for v in [1, i64::MIN, i64::MAX] {
                // SAFETY: as in `uninit`; a reborrow keeps `ctx`.
                unsafe { ctx.as_mut().into_hwctx::<FakeHw>() }
                    .unwrap()
                    .set_value(v);
                // SAFETY: as above.
                let hw = unsafe { ctx.as_ref().hwctx::<FakeHw>() }.unwrap();
                assert_eq!(hw.value(), v);
            }
            // SAFETY: as in `device`.
            let device = unsafe { u.init() }.unwrap();
            // SAFETY: as in `ref_hwctx`.
            let hw = unsafe { device.as_ref().context().hwctx::<FakeHw>() }.unwrap();
            assert_eq!(hw.value(), i64::MAX);
            assert!(hw.initialised());
        }

        #[test]
        fn the_free_callback_reaches_the_hardware_context() {
            let events = Arc::new(Events::default());
            let seen = Arc::new(Mutex::new(None));
            let mut u = uninit(&events);
            let s = seen.clone();
            let ev = events.clone();
            u.context_mut()
                .set_free_callback(move |ctx: DeviceContextMut<'_>| {
                    // SAFETY: the fake context's hardware context, still
                    // allocated while the callback runs; resetting the
                    // payload is within the fake backend's rules.
                    let mut hw = unsafe { ctx.into_hwctx::<FakeHw>() }.unwrap();
                    *s.lock().unwrap() = Some((hw.as_ref().value(), ev.get()));
                    hw.set_value(0);
                })
                .ok()
                .unwrap();
            // SAFETY: as in `uninit`.
            unsafe { u.context_mut().into_hwctx::<FakeHw>() }
                .unwrap()
                .set_value(3);
            // SAFETY: as in `device`.
            let device = unsafe { u.init() }.unwrap();
            drop(device);
            assert_eq!(*seen.lock().unwrap(), Some((3, (1, 1, 0))));
        }
    }
}
