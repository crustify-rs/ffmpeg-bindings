//! Equivalence of `libavutil/hwcontext.h`'s [`DeviceType`] and
//! [`TransferDirection`] with the C enums, as C observes them.
//!
//! [`DeviceType`] is observed through C's name table
//! (`av_hwdevice_get_type_name`, `av_hwdevice_find_type_by_name`), its backend
//! walk (`av_hwdevice_iterate_types`) and C-allocated storage holding the
//! values. [`TransferDirection`] is compared with the C enumerators, and
//! with what its only C reader, `av_hwframe_transfer_get_formats`, lists for
//! each direction in the `transfer` comparisons.
//!
//! The campaign builds compile no hwcontext backend, so C can make no
//! hardware frames context; the transfers (`transfer`) run on the fake one
//! of `tests/common/hwframes.rs`.

use core::ffi::{CStr, c_uint, c_void};
use core::mem::size_of;

use ffmpeg::libavutil::ffi;
use ffmpeg::libavutil::hwcontext::{DeviceType, TransferDirection};
use ffmpeg::libavutil::mem::{HeapVec, alloc_array_zeroed};

use ffi::AVHWDeviceType as C;

/// Every named constant beside its C enumerator and the name C gives it.
const NAMED: [(DeviceType, C, Option<&str>); 15] = [
    (DeviceType::NONE, C::AV_HWDEVICE_TYPE_NONE, None),
    (DeviceType::VDPAU, C::AV_HWDEVICE_TYPE_VDPAU, Some("vdpau")),
    (DeviceType::CUDA, C::AV_HWDEVICE_TYPE_CUDA, Some("cuda")),
    (DeviceType::VAAPI, C::AV_HWDEVICE_TYPE_VAAPI, Some("vaapi")),
    (DeviceType::DXVA2, C::AV_HWDEVICE_TYPE_DXVA2, Some("dxva2")),
    (DeviceType::QSV, C::AV_HWDEVICE_TYPE_QSV, Some("qsv")),
    (
        DeviceType::VIDEOTOOLBOX,
        C::AV_HWDEVICE_TYPE_VIDEOTOOLBOX,
        Some("videotoolbox"),
    ),
    (
        DeviceType::D3D11VA,
        C::AV_HWDEVICE_TYPE_D3D11VA,
        Some("d3d11va"),
    ),
    (DeviceType::DRM, C::AV_HWDEVICE_TYPE_DRM, Some("drm")),
    (
        DeviceType::OPENCL,
        C::AV_HWDEVICE_TYPE_OPENCL,
        Some("opencl"),
    ),
    (
        DeviceType::MEDIACODEC,
        C::AV_HWDEVICE_TYPE_MEDIACODEC,
        Some("mediacodec"),
    ),
    (
        DeviceType::VULKAN,
        C::AV_HWDEVICE_TYPE_VULKAN,
        Some("vulkan"),
    ),
    (
        DeviceType::D3D12VA,
        C::AV_HWDEVICE_TYPE_D3D12VA,
        Some("d3d12va"),
    ),
    (DeviceType::AMF, C::AV_HWDEVICE_TYPE_AMF, Some("amf")),
    (
        DeviceType::OHCODEC,
        C::AV_HWDEVICE_TYPE_OHCODEC,
        Some("ohcodec"),
    ),
];

/// Values a user may store in `AVHWDeviceContext.type` though C names none
/// of them: just past the table, and the `unsigned` extremes a negative `int`
/// written there reads back as.
const UNNAMED: [c_uint; 7] = [
    15,
    16,
    64,
    1 << 20,
    i32::MAX as c_uint,
    i32::MIN as c_uint,
    c_uint::MAX,
];

/// What C calls `t`: `av_hwdevice_get_type_name`, NULL as `None`.
fn c_name(t: C) -> Option<&'static str> {
    // SAFETY: `av_hwdevice_get_type_name` takes the enum by value and
    // bounds-checks it against its name table.
    let name = unsafe { ffi::av_hwdevice_get_type_name(t) };
    if name.is_null() {
        return None;
    }
    // SAFETY: a non-NULL result is a string literal in `hwcontext.c`'s static
    // `hw_type_names` table.
    Some(unsafe { CStr::from_ptr(name) }.to_str().unwrap())
}

/// The type C finds for `name`: `av_hwdevice_find_type_by_name`.
fn c_find(name: &CStr) -> DeviceType {
    // SAFETY: `name` is a valid NUL-terminated string for the call, which
    // only compares it with the table's names.
    DeviceType::from_raw(unsafe { ffi::av_hwdevice_find_type_by_name(name.as_ptr()) })
}

/// The next usable type after `prev`: `av_hwdevice_iterate_types`.
fn c_next(prev: C) -> DeviceType {
    // SAFETY: `av_hwdevice_iterate_types` takes the enum by value and only
    // compares it with the compiled-in backends' types.
    DeviceType::from_raw(unsafe { ffi::av_hwdevice_iterate_types(prev) })
}

/// Whether the wrapper names `t`: its `Debug` is a constant's name, not the
/// raw tuple form.
fn rust_names(t: DeviceType) -> bool {
    !format!("{t:?}").starts_with("DeviceType(")
}

#[test]
fn constants_are_the_c_enumerators() {
    for (t, c, name) in NAMED {
        assert_eq!(t.into_raw(), c);
        assert_eq!(DeviceType::from_raw(c), t);
        assert_eq!(DeviceType::from(c), t);
        assert_eq!(C::from(t), c);
        assert_eq!(c_name(t.into_raw()), c_name(c));
        assert_eq!(c_name(t.into_raw()), name, "{t:?}");
    }
    assert_eq!(DeviceType::default().into_raw(), C::AV_HWDEVICE_TYPE_NONE);
}

#[test]
fn the_wrapper_names_exactly_what_c_names_and_none() {
    for raw in (0..=48).chain(UNNAMED) {
        let t = DeviceType::from_raw(C(raw));
        let c_names = c_name(C(raw)).is_some() || C(raw) == C::AV_HWDEVICE_TYPE_NONE;
        assert_eq!(rust_names(t), c_names, "raw {raw}");
        assert_eq!(t.into_raw().0, raw);
    }
}

#[test]
fn c_name_lookups_round_trip_through_the_constants() {
    for (t, _, name) in NAMED {
        let Some(name) = name else { continue };
        let name = std::ffi::CString::new(name).unwrap();
        // C's reverse lookup lands on the wrapper's constant, and the
        // constant's encoding reads back as the same name.
        let found = c_find(&name);
        assert_eq!(found, t);
        assert_eq!(c_name(found.into_raw()), name.to_str().ok());
    }
    for unknown in [c"", c"none", c"CUDA", c"vaapi ", c"vulkan2", c"hw"] {
        assert_eq!(c_find(unknown), DeviceType::NONE, "{unknown:?}");
        assert_eq!(c_find(unknown), DeviceType::default());
    }
}

#[test]
fn backend_walk_agrees_for_rust_and_c_encodings() {
    // Start from every named and unnamed value, then follow C's walk from
    // the Rust side until it reports NONE; every step must be a type the
    // wrapper names, and strictly after the previous one.
    let starts = NAMED
        .iter()
        .map(|&(t, c, _)| (t, c))
        .chain(UNNAMED.map(|r| (DeviceType::from_raw(C(r)), C(r))));
    for (t, c) in starts {
        assert_eq!(c_next(t.into_raw()), c_next(c));
    }
    let mut prev = DeviceType::NONE;
    let mut walked = Vec::new();
    for _ in 0..=NAMED.len() {
        let next = c_next(prev.into_raw());
        if next == DeviceType::NONE {
            break;
        }
        assert!(rust_names(next), "{next:?}");
        assert!(next.into_raw().0 > prev.into_raw().0);
        assert!(c_name(next.into_raw()).is_some());
        walked.push(next);
        prev = next;
    }
    assert!(walked.len() < NAMED.len());
}

/// A C block of `ffi::AVHWDeviceType`, freed with `av_free` on drop.
struct CTypes(*mut C, usize);

impl CTypes {
    fn zeroed(len: usize) -> Self {
        // SAFETY: `av_calloc` takes its sizes by value.
        let ptr = unsafe { ffi::av_calloc(len, size_of::<C>()) };
        assert!(!ptr.is_null());
        Self(ptr.cast(), len)
    }

    fn get(&self, i: usize) -> C {
        assert!(i < self.1);
        // SAFETY: `i` is in bounds of the live, initialised block.
        unsafe { self.0.add(i).read() }
    }

    fn set(&mut self, i: usize, t: C) {
        assert!(i < self.1);
        // SAFETY: `i` is in bounds of the live block, which we own.
        unsafe { self.0.add(i).write(t) }
    }
}

impl Drop for CTypes {
    fn drop(&mut self) {
        // SAFETY: `self.0` is a live `av_calloc` block we own.
        unsafe { ffi::av_free(self.0.cast::<c_void>()) }
    }
}

/// The C values a Rust array holds, read back through its C-typed pointer.
fn rust_as_c(a: &HeapVec<DeviceType>) -> Vec<C> {
    let ptr = a.as_ptr().cast::<C>();
    (0..a.len())
        // SAFETY: `DeviceType` is `#[repr(transparent)]` over
        // `ffi::AVHWDeviceType`, and `i` is in bounds of the live,
        // initialised array `a` borrows.
        .map(|i| unsafe { ptr.add(i).read() })
        .collect()
}

#[test]
fn storage_holds_the_same_values_and_names_after_each_write() {
    let values: Vec<C> = NAMED
        .iter()
        .map(|&(_, c, _)| c)
        .chain(UNNAMED.map(C))
        .collect();
    let len = values.len();
    let mut rust = alloc_array_zeroed::<DeviceType>(len).unwrap();
    let mut c = CTypes::zeroed(len);
    // Zeroed storage reads as NONE on both sides.
    assert_eq!(
        rust_as_c(&rust),
        (0..len).map(|i| c.get(i)).collect::<Vec<_>>()
    );
    assert!(rust.as_slice().iter().all(|&t| t == DeviceType::NONE));
    // Fill both from opposite ends, comparing every slot and its C name
    // after each step.
    for (step, &v) in values.iter().enumerate() {
        let i = (step * 7) % len;
        rust.as_mut_slice()[i] = DeviceType::from_raw(v);
        c.set(i, v);
        let r = rust_as_c(&rust);
        for (j, &rv) in r.iter().enumerate() {
            assert_eq!(rv, c.get(j), "step {step}, slot {j}");
            assert_eq!(c_name(rv), c_name(c.get(j)));
        }
    }
}

#[test]
fn transfer_directions_are_the_c_enumerators() {
    use ffi::AVHWFrameTransferDirection as D;
    assert_eq!(
        TransferDirection::From.into_raw(),
        D::AV_HWFRAME_TRANSFER_DIRECTION_FROM
    );
    assert_eq!(
        TransferDirection::To.into_raw(),
        D::AV_HWFRAME_TRANSFER_DIRECTION_TO
    );
    assert_eq!(
        D::from(TransferDirection::From),
        D::AV_HWFRAME_TRANSFER_DIRECTION_FROM
    );
    assert_eq!(
        D::from(TransferDirection::To),
        D::AV_HWFRAME_TRANSFER_DIRECTION_TO
    );
}

/// The fake hardware frames context backend the transfer comparisons run
/// on.
#[path = "../common/hwframes.rs"]
mod hwframes;

/// `av_hwframe_transfer_get_formats` and `av_hwframe_transfer_data` against
/// [`transfer_formats`], [`FrameRef::transfer_formats`] and
/// [`FrameMut::transfer_data_from`] (hwcontext wave 1, batch 2), over
/// independent fake contexts and frames built alike for each side.
///
/// [`transfer_formats`]: ffmpeg::libavutil::hwcontext::transfer_formats
/// [`FrameRef::transfer_formats`]: ffmpeg::libavutil::frame::FrameRef::transfer_formats
/// [`FrameMut::transfer_data_from`]: ffmpeg::libavutil::frame::FrameMut::transfer_data_from
mod transfer {
    use core::ffi::c_int;
    use core::ptr;

    use ffmpeg::libavutil::buffer::BufferReferenceBox;
    use ffmpeg::libavutil::frame::{Frame, FrameBox};
    use ffmpeg::libavutil::hwcontext::{TransferDirection, transfer_formats};
    use ffmpeg::libavutil::pixfmt::PixelFormat;

    use super::ffi;
    use super::hwframes::{
        self as fx, Behaviour, DECLINES, EMPTY, GRAY_FIRST, INERT, LISTING_FAILS, TRANSFER_FAILS,
        YUV, pattern,
    };

    use ffi::AVPixelFormat as Fmt;

    const W: c_int = 16;
    const H: c_int = 8;

    /// An `AVFrame` C owns, freed on drop.
    struct CFrame(*mut ffi::AVFrame);

    impl Drop for CFrame {
        fn drop(&mut self) {
            // SAFETY: a frame this test owns.
            unsafe { ffi::av_frame_free(&mut self.0) };
        }
    }

    /// An `AVBufferRef` C owns, released on drop.
    struct CBuf(*mut ffi::AVBufferRef);

    impl Drop for CBuf {
        fn drop(&mut self) {
            // SAFETY: a reference this test owns.
            unsafe { ffi::av_buffer_unref(&mut self.0) };
        }
    }

    /// The same fake context for each side: C's reference and Rust's owner.
    fn contexts(behaviour: &'static Behaviour, w: c_int, h: c_int) -> (CBuf, BufferReferenceBox) {
        let c = CBuf(fx::yuv_context(behaviour, w, h));
        assert!(!c.0.is_null());
        // SAFETY: a fresh reference the fixture handed over; its bytes, the
        // zeroed context, are initialised.
        let r = unsafe { BufferReferenceBox::from_c(fx::yuv_context(behaviour, w, h)) }.unwrap();
        (c, r)
    }

    /// The contexts of a backend without transfers.
    fn inert_contexts() -> (CBuf, BufferReferenceBox) {
        let make = || {
            fx::new_context(
                &INERT,
                &YUV,
                Fmt::AV_PIX_FMT_MEDIACODEC,
                Fmt::AV_PIX_FMT_YUV420P,
                W,
                H,
            )
        };
        let c = CBuf(make());
        // SAFETY: as in `contexts`.
        let r = unsafe { BufferReferenceBox::from_c(make()) };
        (c, r.unwrap())
    }

    /// A hardware frame of each side's context, its surface holding
    /// `pattern(seed)`.
    fn hw_frames(ctx: &(CBuf, BufferReferenceBox), seed: u8) -> (CFrame, FrameBox) {
        // SAFETY: live fake contexts; the Rust frame is adopted fresh, and
        // satisfies `Frame`'s invariant: its context reference is one, and
        // `data[3]` is the start of `buf[0]`'s window.
        unsafe {
            let c = CFrame(fx::new_hw_frame(ctx.0.0, pattern(seed)));
            let r = FrameBox::from_c(fx::new_hw_frame(ctx.1.as_c_ptr(), pattern(seed))).unwrap();
            assert!(!c.0.is_null());
            (c, r)
        }
    }

    /// A software frame for each side: C's from `av_frame_get_buffer`,
    /// Rust's from [`FrameMut::alloc_buffers`], both zeroed and then holding
    /// `pattern(seed)`.
    ///
    /// [`FrameMut::alloc_buffers`]: ffmpeg::libavutil::frame::FrameMut::alloc_buffers
    fn sw_frames(format: Fmt, w: c_int, h: c_int, seed: u8) -> (CFrame, FrameBox) {
        let c = CFrame(fx::new_sw_frame(format, w, h, pattern(seed)));
        assert!(!c.0.is_null());
        let mut r = Frame::new().unwrap();
        let mut m = r.as_mut();
        m.set_pixel_format(PixelFormat::from_raw(format));
        m.set_width(w);
        m.set_height(h);
        m.alloc_buffers(0).unwrap();
        let fill = pattern(seed);
        let lines = m.as_ref().linesizes();
        let count = PixelFormat::from_raw(format)
            .plane_count()
            .unwrap_or(0)
            .min(4);
        for i in 0..count {
            // SAFETY: a pure query of C's descriptor table.
            let len = unsafe { ffi::av_image_get_linesize(format, w, i as c_int) } as usize;
            let rows = if i == 1 || i == 2 { (h + 1) / 2 } else { h } as usize;
            let plane = m.plane_mut(i).unwrap();
            for row in 0..rows {
                for col in 0..len {
                    plane[row * lines[i] as usize + col] = fill(i, row, col);
                }
            }
        }
        (c, r)
    }

    /// Both sides' `width` x `height` image of `format`; a hardware frame's
    /// is its surface's.
    fn images(f: &(CFrame, FrameBox), format: Fmt, w: c_int, h: c_int) -> (Vec<u8>, Vec<u8>) {
        // SAFETY: callers pass frames whose planes (surfaces) hold that
        // image, initialised.
        unsafe {
            (
                fx::image(f.0.0, format, w, h),
                fx::image(f.1.as_c_ptr(), format, w, h),
            )
        }
    }

    /// Both sides' whole surface images.
    fn surfaces(f: &(CFrame, FrameBox)) -> (Vec<u8>, Vec<u8>) {
        // SAFETY: hardware frames of fake contexts.
        unsafe { (fx::surface_image(f.0.0), fx::surface_image(f.1.as_c_ptr())) }
    }

    /// A frame's format, width, height and line sizes.
    type Shape = (c_int, c_int, c_int, [c_int; 8]);

    /// The [`Shape`] of each side.
    fn shapes(f: &(CFrame, FrameBox)) -> (Shape, Shape) {
        // SAFETY: a live frame this test owns.
        let c = unsafe {
            (
                (*f.0.0).format,
                (*f.0.0).width,
                (*f.0.0).height,
                (*f.0.0).linesize,
            )
        };
        let r = f.1.as_ref();
        (c, (r.format(), r.width(), r.height(), r.linesizes()))
    }

    /// C's `av_hwframe_transfer_data(dst, src, 0)`.
    fn c_transfer(dst: &CFrame, src: &CFrame) -> c_int {
        // SAFETY: live frames, each hardware one a frame of its fake context,
        // every software one holding the surfaces' image as C requires.
        unsafe { ffi::av_hwframe_transfer_data(dst.0, src.0, 0) }
    }

    /// Rust's transfer, as C reports it.
    fn r_transfer(dst: &mut FrameBox, src: &FrameBox) -> c_int {
        // SAFETY: every hardware frame is one of its fake context.
        match unsafe { dst.as_mut().transfer_data_from(src.as_ref()) } {
            Ok(()) => 0,
            Err(e) => e,
        }
    }

    /// One transfer on each side, compared.
    fn transfer_both(dst: &mut (CFrame, FrameBox), src: &(CFrame, FrameBox)) -> c_int {
        let c = c_transfer(&dst.0, &src.0);
        let r = r_transfer(&mut dst.1, &src.1);
        assert_eq!(r, c);
        c
    }

    /// C's listing, as Rust reports it.
    fn c_formats(ctx: &CBuf, dir: TransferDirection) -> Result<Vec<PixelFormat>, c_int> {
        let mut list = ptr::null_mut();
        let (raw, dir) = (ctx.0, dir.into_raw());
        // SAFETY: a live fake context and a writable slot.
        let ret = unsafe { ffi::av_hwframe_transfer_get_formats(raw, dir, &mut list, 0) };
        if ret < 0 {
            return Err(ret);
        }
        let mut out = Vec::new();
        // SAFETY: a terminated list C handed over, read to its terminator
        // and freed.
        unsafe {
            let mut p = list;
            while *p != Fmt::AV_PIX_FMT_NONE {
                out.push(PixelFormat::from_raw(*p));
                p = p.add(1);
            }
            ffi::av_free(list.cast());
        }
        Ok(out)
    }

    const DIRS: [TransferDirection; 2] = [TransferDirection::From, TransferDirection::To];

    #[test]
    fn format_listings_match_c() {
        for behaviour in [&YUV, &GRAY_FIRST, &EMPTY, &LISTING_FAILS, &TRANSFER_FAILS] {
            let ctx = contexts(behaviour, W, H);
            let frames = hw_frames(&ctx, 0);
            for dir in DIRS {
                let c = c_formats(&ctx.0, dir);
                // SAFETY: a live reference to a fake context.
                let r = unsafe { transfer_formats(ctx.1.as_ref(), dir) };
                assert_eq!(r.map(|l| l.as_slice().to_vec()), c, "{dir:?}");
                let r = frames.1.as_ref().transfer_formats(dir);
                assert_eq!(r.map(|l| l.as_slice().to_vec()), c, "{dir:?}");
            }
        }
        let inert = inert_contexts();
        for dir in DIRS {
            let c = c_formats(&inert.0, dir);
            assert_eq!(c, Err(-(ffi::ENOSYS as c_int)));
            // SAFETY: a live reference to a fake context.
            let r = unsafe { transfer_formats(inert.1.as_ref(), dir) };
            assert_eq!(r.map(|l| l.as_slice().to_vec()), c);
        }
    }

    #[test]
    fn downloads_match_c() {
        for (format, w, h) in [
            (Fmt::AV_PIX_FMT_YUV420P, W, H),
            (Fmt::AV_PIX_FMT_YUV420P, 10, 5),
            (Fmt::AV_PIX_FMT_YUV420P, 1, 1),
            (Fmt::AV_PIX_FMT_YUV420P, 0, 0),
            (Fmt::AV_PIX_FMT_GRAY8, W, H),
            (Fmt::AV_PIX_FMT_GRAY8, 3, 7),
        ] {
            let ctx = contexts(&YUV, W, H);
            let mut src = hw_frames(&ctx, 1);
            let mut dst = sw_frames(format, W, H, 0);
            // SAFETY: frames this test owns.
            unsafe {
                ((*src.0.0).width, (*src.0.0).height) = (w, h);
                ((*dst.0.0).width, (*dst.0.0).height) = (w, h);
            }
            for f in [&mut src.1, &mut dst.1] {
                f.as_mut().set_width(w);
                f.as_mut().set_height(h);
            }
            assert_eq!(transfer_both(&mut dst, &src), 0, "{format:?} {w}x{h}");
            let (c, r) = images(&dst, format, W, H);
            assert_eq!(r, c, "{format:?} {w}x{h}");
            let (c, r) = shapes(&dst);
            assert_eq!(r, c);
        }
    }

    #[test]
    fn uploads_match_c() {
        for (w, h) in [(W, H), (9, 3), (0, 0)] {
            let ctx = contexts(&YUV, W, H);
            let mut dst = hw_frames(&ctx, 0);
            let mut src = sw_frames(Fmt::AV_PIX_FMT_YUV420P, W, H, 2);
            // SAFETY: frames this test owns.
            unsafe { ((*src.0.0).width, (*src.0.0).height) = (w, h) };
            src.1.as_mut().set_width(w);
            src.1.as_mut().set_height(h);
            assert_eq!(transfer_both(&mut dst, &src), 0);
            let (c, r) = surfaces(&dst);
            assert_eq!(r, c, "{w}x{h}");
            // The upload changed the surface where it wrote anything.
            // SAFETY: a fresh hardware frame of a fake context.
            let untouched = unsafe { fx::surface_image(hw_frames(&ctx, 0).0.0) };
            assert_eq!(c != untouched, w > 0 && h > 0);
        }
    }

    #[test]
    fn uploads_from_planes_lent_to_a_frame_match_c() {
        let ctx = contexts(&YUV, W, H);
        let mut dst = hw_frames(&ctx, 0);
        let mut mem: Vec<u8> = (0..(W * H * 3 / 2) as usize).map(|i| i as u8).collect();
        let base = mem.as_mut_ptr();
        let offsets = [0, (W * H) as usize, (W * H + W * H / 4) as usize];
        let c = CFrame(
            // SAFETY: a fresh frame lent `mem`'s planes, which outlive it.
            unsafe {
                let f = ffi::av_frame_alloc();
                (*f).format = Fmt::AV_PIX_FMT_YUV420P.0;
                ((*f).width, (*f).height) = (W, H);
                for (i, off) in offsets.into_iter().enumerate() {
                    (*f).data[i] = base.add(off);
                    (*f).linesize[i] = if i == 0 { W } else { W / 2 };
                }
                f
            },
        );
        let mut r = Frame::new().unwrap();
        let mut m = r.as_mut();
        m.set_pixel_format(PixelFormat::YUV420P);
        m.set_width(W);
        m.set_height(H);
        m.set_linesizes([W, W / 2, W / 2, 0, 0, 0, 0, 0]);
        for (i, off) in offsets.into_iter().enumerate() {
            // SAFETY: `mem` outlives the frame and holds each plane; nothing
            // writes it meanwhile.
            unsafe { m.set_data_ptr(i, base.add(off)) };
        }
        let src = (c, r);
        assert_eq!(transfer_both(&mut dst, &src), 0);
        let (c, r) = surfaces(&dst);
        assert_eq!(r, c);
        drop(src);
        drop(mem);
    }

    #[test]
    fn hardware_to_hardware_copies_match_c() {
        for (src_behaviour, dst_behaviour) in [
            (&YUV, &YUV),
            (&DECLINES, &YUV),
            (&DECLINES, &DECLINES),
            (&TRANSFER_FAILS, &YUV),
        ] {
            let (sc, dc) = (contexts(src_behaviour, W, H), contexts(dst_behaviour, W, H));
            let src = hw_frames(&sc, 3);
            let mut dst = hw_frames(&dc, 4);
            let ret = transfer_both(&mut dst, &src);
            let (c, r) = surfaces(&dst);
            assert_eq!(r, c);
            assert_eq!(ret == 0, c == surfaces(&src).0);
        }
    }

    #[test]
    fn derived_contexts_are_refused_between_hardware_frames_as_c_does() {
        for derived_side in [0, 1] {
            let (sc, dc) = (contexts(&YUV, W, H), contexts(&YUV, W, H));
            let source = contexts(&YUV, W, H);
            let target = if derived_side == 0 { &sc } else { &dc };
            // SAFETY: live references to fake contexts.
            unsafe {
                fx::derive(target.0.0, source.0.0);
                fx::derive(target.1.as_c_ptr(), source.1.as_c_ptr());
            }
            let src = hw_frames(&sc, 5);
            let mut dst = hw_frames(&dc, 6);
            assert_eq!(transfer_both(&mut dst, &src), -(ffi::ENOSYS as c_int));
            let (c, r) = surfaces(&dst);
            assert_eq!(r, c);
        }
    }

    #[test]
    fn downloads_into_new_buffers_match_c() {
        for (behaviour, format, w, h) in [
            (&YUV, Fmt::AV_PIX_FMT_NONE, W, H),
            (&YUV, Fmt::AV_PIX_FMT_NONE, 7, 3),
            (&YUV, Fmt::AV_PIX_FMT_GRAY8, W, H),
            (&GRAY_FIRST, Fmt::AV_PIX_FMT_NONE, 5, 8),
            (&GRAY_FIRST, Fmt::AV_PIX_FMT_YUV420P, W, H),
            (&EMPTY, Fmt::AV_PIX_FMT_NONE, W, H),
            (&LISTING_FAILS, Fmt::AV_PIX_FMT_NONE, W, H),
            (&TRANSFER_FAILS, Fmt::AV_PIX_FMT_NONE, W, H),
            (&YUV, Fmt::AV_PIX_FMT_VAAPI, W, H),
        ] {
            let ctx = contexts(behaviour, W, H);
            let mut src = hw_frames(&ctx, 7);
            // SAFETY: a frame this test owns.
            unsafe { ((*src.0.0).width, (*src.0.0).height) = (w, h) };
            src.1.as_mut().set_width(w);
            src.1.as_mut().set_height(h);
            // Fresh frames hold nothing C would leak.
            // SAFETY: a fresh frame, given a format.
            let c = CFrame(unsafe {
                let f = ffi::av_frame_alloc();
                (*f).format = format.0;
                f
            });
            let mut r = Frame::new().unwrap();
            r.as_mut().set_pixel_format(PixelFormat::from_raw(format));
            let mut dst = (c, r);
            let ret = transfer_both(&mut dst, &src);
            let (c_shape, r_shape) = shapes(&dst);
            assert_eq!(r_shape.0..r_shape.2, c_shape.0..c_shape.2, "{format:?}");
            if ret == 0 {
                // C's bytes past the copied lines are uninitialised: compare
                // the displayed image, which the transfer wrote.
                let (c, r) = images(&dst, Fmt(c_shape.0), w, h);
                assert_eq!(r, c, "{format:?} {w}x{h}");
            }
        }
    }

    #[test]
    fn refusals_without_contexts_match_c() {
        let mut clean = (
            // SAFETY: a fresh frame.
            CFrame(unsafe { ffi::av_frame_alloc() }),
            Frame::new().unwrap(),
        );
        let src = sw_frames(Fmt::AV_PIX_FMT_GRAY8, 4, 4, 1);
        let mut counted = sw_frames(Fmt::AV_PIX_FMT_GRAY8, 4, 4, 0);
        assert_eq!(transfer_both(&mut clean, &src), -(ffi::EINVAL as c_int));
        assert_eq!(transfer_both(&mut counted, &src), -(ffi::ENOSYS as c_int));
        let (c, r) = images(&counted, Fmt::AV_PIX_FMT_GRAY8, 4, 4);
        assert_eq!(r, c);
    }

    #[test]
    fn transfer_errors_match_c() {
        let ctx = contexts(&TRANSFER_FAILS, W, H);
        let src = hw_frames(&ctx, 8);
        let mut dst = sw_frames(Fmt::AV_PIX_FMT_YUV420P, W, H, 0);
        assert_eq!(transfer_both(&mut dst, &src), -5);
        let mut hw = hw_frames(&ctx, 9);
        let up = sw_frames(Fmt::AV_PIX_FMT_YUV420P, W, H, 1);
        assert_eq!(transfer_both(&mut hw, &up), -5);
        let (c, r) = surfaces(&hw);
        assert_eq!(r, c);
    }

    /// Upload, download into new buffers, edit, upload again, copy between
    /// hardware frames and download into existing buffers, comparing after
    /// each step.
    #[test]
    fn transfer_sequences_match_c() {
        let (a, b) = (contexts(&YUV, W, H), contexts(&YUV, W, H));
        let mut ha = hw_frames(&a, 0);
        let mut hb = hw_frames(&b, 0);
        let up = sw_frames(Fmt::AV_PIX_FMT_YUV420P, W, H, 10);
        assert_eq!(transfer_both(&mut ha, &up), 0);
        let (c, r) = surfaces(&ha);
        assert_eq!(r, c);

        let mut down = (
            // SAFETY: a fresh frame.
            CFrame(unsafe { ffi::av_frame_alloc() }),
            Frame::new().unwrap(),
        );
        assert_eq!(transfer_both(&mut down, &ha), 0);
        let (c, r) = images(&down, Fmt::AV_PIX_FMT_YUV420P, W, H);
        assert_eq!(r, c);
        assert_eq!(c, surfaces(&ha).0);

        // Edit one byte of each plane on each side.
        let lines = down.1.as_ref().linesizes();
        for (i, &line) in lines.iter().enumerate().take(3) {
            // SAFETY: plane `i` of a frame C gave buffers, row 1 inside it.
            unsafe { *(*down.0.0).data[i].add((*down.0.0).linesize[i] as usize + 1) ^= 0xff };
            down.1.as_mut().plane_mut(i).unwrap()[line as usize + 1] ^= 0xff;
        }
        assert_eq!(transfer_both(&mut ha, &down), 0);
        let (c, r) = surfaces(&ha);
        assert_eq!(r, c);

        assert_eq!(transfer_both(&mut hb, &ha), 0);
        let (c, r) = surfaces(&hb);
        assert_eq!(r, c);
        assert_eq!(c, surfaces(&ha).0);

        let mut again = sw_frames(Fmt::AV_PIX_FMT_YUV420P, W, H, 0);
        assert_eq!(transfer_both(&mut again, &hb), 0);
        let (c, r) = images(&again, Fmt::AV_PIX_FMT_YUV420P, W, H);
        assert_eq!(r, c);
        assert_eq!(c, images(&down, Fmt::AV_PIX_FMT_YUV420P, W, H).0);
    }
}
