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
#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "test inputs are chosen within range, and the casts mirror the C conversions under comparison"
)]
#![expect(
    clippy::many_single_char_names,
    clippy::similar_names,
    reason = "names follow the C fields, formulas and fixtures they mirror"
)]
#![expect(
    clippy::needless_pass_by_value,
    clippy::redundant_closure_for_method_calls,
    clippy::too_many_lines,
    reason = "each test is one scenario, its helpers and steps kept where they are used"
)]
#![expect(
    clippy::assert_is_empty,
    let_underscore_drop,
    reason = "assertions keep the form of the C behaviour they check"
)]

use core::ffi::{CStr, c_uint, c_void};
use core::mem::size_of;

use ffi::AVHWDeviceType as C;
use ffmpeg_bindings::libavutil::libavutil::ffi;
use ffmpeg_bindings::libavutil::libavutil::hwcontext::{DeviceType, TransferDirection};
use ffmpeg_bindings::libavutil::libavutil::mem::{HeapVec, alloc_array_zeroed};

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
#[cfg_attr(miri, ignore = "calls C")]
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
#[cfg_attr(miri, ignore = "calls C")]
fn the_wrapper_names_exactly_what_c_names_and_none() {
    for raw in (0..=48).chain(UNNAMED) {
        let t = DeviceType::from_raw(C(raw));
        let c_names = c_name(C(raw)).is_some() || C(raw) == C::AV_HWDEVICE_TYPE_NONE;
        assert_eq!(rust_names(t), c_names, "raw {raw}");
        assert_eq!(t.into_raw().0, raw);
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
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
#[cfg_attr(miri, ignore = "calls C")]
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
        // SAFETY: as the comment below states, for this part of the operation.
        let v_0_add_2 = unsafe { self.0.add(i) };
        // SAFETY: `i` is in bounds of the live, initialised block.
        unsafe { v_0_add_2.read() }
    }

    fn set(&mut self, i: usize, t: C) {
        assert!(i < self.1);
        // SAFETY: as the comment below states, for this part of the operation.
        let v_0_add = unsafe { self.0.add(i) };
        // SAFETY: `i` is in bounds of the live block, which we own.
        unsafe { v_0_add.write(t) }
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
        .map(|i| {
            // SAFETY: `i` is in bounds of the live array `a` borrows.
            let at = unsafe { ptr.add(i) };
            // SAFETY: as above, initialised; `DeviceType` is `#[repr(transparent)]` over
            // `ffi::AVHWDeviceType`.
            unsafe { at.read() }
        })
        .collect()
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
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
#[expect(
    dead_code,
    reason = "a helper shared by several test crates; this one uses only some of it"
)]
#[path = "../common/hwframes.rs"]
mod hwframes;

/// `av_hwframe_transfer_get_formats` and `av_hwframe_transfer_data` against
/// [`FrameRef::transfer_formats`] and [`FrameMut::transfer_data_from`]
/// (hwcontext wave 1, batch 2), over independent fake contexts and frames
/// built alike for each side.
///
/// [`FrameRef::transfer_formats`]: ffmpeg_bindings::libavutil::libavutil::frame::FrameRef::transfer_formats
/// [`FrameMut::transfer_data_from`]: ffmpeg_bindings::libavutil::libavutil::frame::FrameMut::transfer_data_from
mod transfer {
    use core::ffi::c_int;
    use core::ptr;

    use ffi::AVPixelFormat as Fmt;
    use ffmpeg_bindings::libavutil::libavutil::buffer::BufferReferenceBox;
    use ffmpeg_bindings::libavutil::libavutil::frame::{Frame, FrameBox};
    use ffmpeg_bindings::libavutil::libavutil::hwcontext::TransferDirection;
    use ffmpeg_bindings::libavutil::libavutil::pixfmt::PixelFormat;

    use super::ffi;
    use super::hwframes::{
        self as fx, Behaviour, DECLINES, EMPTY, GRAY_FIRST, INERT, LISTING_FAILS, TRANSFER_FAILS,
        YUV, pattern,
    };

    const W: c_int = 16;
    const H: c_int = 8;

    /// An `AVFrame` C owns, freed on drop.
    struct CFrame(*mut ffi::AVFrame);

    impl Drop for CFrame {
        fn drop(&mut self) {
            // SAFETY: a frame this test owns.
            unsafe { ffi::av_frame_free(&raw mut self.0) };
        }
    }

    /// An `AVBufferRef` C owns, released on drop.
    struct CBuf(*mut ffi::AVBufferRef);

    impl Drop for CBuf {
        fn drop(&mut self) {
            // SAFETY: a reference this test owns.
            unsafe { ffi::av_buffer_unref(&raw mut self.0) };
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
        // For the unsafe operations below: live fake contexts; the Rust frame is adopted fresh, and
        // satisfies `Frame`'s invariant: its context reference is one, and
        // `data[3]` is the start of `buf[0]`'s window.
        {
            // SAFETY: as the comment above this block states.
            let c = unsafe { CFrame(fx::new_hw_frame(ctx.0.0, pattern(seed))) };
            // SAFETY: as the comment below states, for this part of the operation.
            let new_hw_frame_2 = unsafe { fx::new_hw_frame(ctx.1.as_c_ptr(), pattern(seed)) };
            // SAFETY: as the comment above this block states.
            let r = unsafe { FrameBox::from_c(new_hw_frame_2).unwrap() };
            assert!(!c.0.is_null());
            (c, r)
        }
    }

    /// A software frame for each side: C's from `av_frame_get_buffer`,
    /// Rust's from [`FrameMut::alloc_buffers`], both zeroed and then holding
    /// `pattern(seed)`.
    ///
    /// [`FrameMut::alloc_buffers`]: ffmpeg_bindings::libavutil::libavutil::frame::FrameMut::alloc_buffers
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
        (unsafe { fx::image(f.0.0, format, w, h) }, unsafe {
            fx::image(f.1.as_c_ptr(), format, w, h)
        })
    }

    /// Both sides' whole surface images.
    fn surfaces(f: &(CFrame, FrameBox)) -> (Vec<u8>, Vec<u8>) {
        // SAFETY: hardware frames of fake contexts.
        (unsafe { fx::surface_image(f.0.0) }, unsafe {
            fx::surface_image(f.1.as_c_ptr())
        })
    }

    /// A frame's format, width, height and line sizes.
    type Shape = (c_int, c_int, c_int, [c_int; 8]);

    /// The [`Shape`] of each side.
    fn shapes(f: &(CFrame, FrameBox)) -> (Shape, Shape) {
        // SAFETY: a live frame this test owns.
        let c = (
            unsafe { (*f.0.0).format },
            unsafe { (*f.0.0).width },
            unsafe { (*f.0.0).height },
            unsafe { (*f.0.0).linesize },
        );
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
        let ret = unsafe { ffi::av_hwframe_transfer_get_formats(raw, dir, &raw mut list, 0) };
        if ret < 0 {
            return Err(ret);
        }
        let mut out = Vec::new();
        // For the unsafe operations below: a terminated list C handed over, read to its terminator
        // and freed.
        {
            let mut p = list;
            // SAFETY: as the comment above this block states.
            while unsafe { *p } != Fmt::AV_PIX_FMT_NONE {
                // SAFETY: as the comment above this statement states.
                out.push(PixelFormat::from_raw(unsafe { *p }));
                // SAFETY: as the comment at the head of this block states.
                p = unsafe { p.add(1) };
            }
            // SAFETY: as the comment above this block states.
            unsafe { ffi::av_free(list.cast()) };
        }
        Ok(out)
    }

    const DIRS: [TransferDirection; 2] = [TransferDirection::From, TransferDirection::To];

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn format_listings_match_c() {
        for behaviour in [&YUV, &GRAY_FIRST, &EMPTY, &LISTING_FAILS, &TRANSFER_FAILS] {
            let ctx = contexts(behaviour, W, H);
            let frames = hw_frames(&ctx, 0);
            for dir in DIRS {
                let c = c_formats(&ctx.0, dir);
                let r = frames.1.as_ref().transfer_formats(dir);
                assert_eq!(r.map(|l| l.as_slice().to_vec()), c, "{dir:?}");
            }
        }
        let inert = inert_contexts();
        let frames = hw_frames(&inert, 0);
        for dir in DIRS {
            let c = c_formats(&inert.0, dir);
            assert_eq!(c, Err(-(ffi::ENOSYS as c_int)));
            let r = frames.1.as_ref().transfer_formats(dir);
            assert_eq!(r.map(|l| l.as_slice().to_vec()), c);
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
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
            // For the unsafe operations below: frames this test owns.
            {
                // SAFETY: as the comment above this block states.
                unsafe { (*src.0.0).width = w };
                // SAFETY: as the comment above the previous write states.
                unsafe { (*src.0.0).height = h };
                // SAFETY: as the comment above this block states.
                unsafe { (*dst.0.0).width = w };
                // SAFETY: as the comment above the previous write states.
                unsafe { (*dst.0.0).height = h };
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
    #[cfg_attr(miri, ignore = "calls C")]
    fn uploads_match_c() {
        for (w, h) in [(W, H), (9, 3), (0, 0)] {
            let ctx = contexts(&YUV, W, H);
            let mut dst = hw_frames(&ctx, 0);
            let mut src = sw_frames(Fmt::AV_PIX_FMT_YUV420P, W, H, 2);
            // SAFETY: frames this test owns; a write of its `width`.
            unsafe { (*src.0.0).width = w };
            // SAFETY: frames this test owns; a write of its `height`.
            unsafe { (*src.0.0).height = h };
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
    #[cfg_attr(miri, ignore = "calls C")]
    fn uploads_from_planes_lent_to_a_frame_match_c() {
        let ctx = contexts(&YUV, W, H);
        let mut dst = hw_frames(&ctx, 0);
        let mut mem: Vec<u8> = (0..(W * H * 3 / 2) as usize).map(|i| i as u8).collect();
        let base = mem.as_mut_ptr();
        let offsets = [0, (W * H) as usize, (W * H + W * H / 4) as usize];
        let c = CFrame(
            // SAFETY: a fresh frame lent `mem`'s planes, which outlive it.
            {
                // SAFETY: as the comment above this block states.
                let f = unsafe { ffi::av_frame_alloc() };
                // SAFETY: as the comment above this block states.
                unsafe { (*f).format = Fmt::AV_PIX_FMT_YUV420P.0 };
                // SAFETY: as the comment above this block states.
                unsafe { (*f).width = W };
                // SAFETY: as the comment above the previous write states.
                unsafe { (*f).height = H };
                for (i, off) in offsets.into_iter().enumerate() {
                    // SAFETY: as the comment above this block states: plane `i`'s offset into
                    // the block.
                    let plane = unsafe { base.add(off) };
                    // SAFETY: as the comment above this block states.
                    unsafe { (*f).data[i] = plane };
                    // SAFETY: as the comment above this block states.
                    unsafe { (*f).linesize[i] = if i == 0 { W } else { W / 2 } };
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
            // SAFETY: as the comment below states, for this part of the operation.
            let base_add = unsafe { base.add(off) };
            // SAFETY: `mem` outlives the frame and holds each plane; nothing
            // writes it meanwhile.
            unsafe { m.set_data_ptr(i, base_add) };
        }
        let src = (c, r);
        assert_eq!(transfer_both(&mut dst, &src), 0);
        let (c, r) = surfaces(&dst);
        assert_eq!(r, c);
        drop(src);
        drop(mem);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
    fn derived_contexts_are_refused_between_hardware_frames_as_c_does() {
        for derived_side in [0, 1] {
            let (sc, dc) = (contexts(&YUV, W, H), contexts(&YUV, W, H));
            let source = contexts(&YUV, W, H);
            let target = if derived_side == 0 { &sc } else { &dc };
            // For the unsafe operations below: live references to fake contexts.
            {
                // SAFETY: as the comment above this block states.
                unsafe { fx::derive(target.0.0, source.0.0) };
                // SAFETY: as the comment above this block states.
                unsafe { fx::derive(target.1.as_c_ptr(), source.1.as_c_ptr()) };
            }
            let src = hw_frames(&sc, 5);
            let mut dst = hw_frames(&dc, 6);
            assert_eq!(transfer_both(&mut dst, &src), -(ffi::ENOSYS as c_int));
            let (c, r) = surfaces(&dst);
            assert_eq!(r, c);
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
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
            // SAFETY: a frame this test owns; a write of its `width`.
            unsafe { (*src.0.0).width = w };
            // SAFETY: a frame this test owns; a write of its `height`.
            unsafe { (*src.0.0).height = h };
            src.1.as_mut().set_width(w);
            src.1.as_mut().set_height(h);
            // Fresh frames hold nothing C would leak.
            // For the unsafe operations below: a fresh frame, given a format.
            let c = CFrame({
                // SAFETY: as the comment above this block states.
                let f = unsafe { ffi::av_frame_alloc() };
                // SAFETY: as the comment above this block states.
                unsafe { (*f).format = format.0 };
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
            // SAFETY: a frame C gave buffers: reads of plane `i` and its line size.
            let (plane, ls) = (unsafe { (*down.0.0).data[i] }, unsafe {
                (*down.0.0).linesize[i]
            });
            // SAFETY: as above: byte 1 of row 1 lies inside the plane.
            let at = unsafe { plane.add(ls as usize + 1) };
            // SAFETY: as above.
            unsafe { *at ^= 0xFF };
            down.1.as_mut().plane_mut(i).unwrap()[line as usize + 1] ^= 0xFF;
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

#[expect(
    dead_code,
    reason = "a helper shared by several test crates; this one uses only some of it"
)]
#[path = "../common/hwbackend.rs"]
mod backend;

/// Wave 1, batch 1: the name table, the backend walk, and the device and
/// frames contexts and hardware frames, driven in C and through the
/// wrappers over independent contexts of the test backends of
/// `tests/common/hwbackend.rs`, comparing results, frames, reference counts
/// and the backends' event logs after each step. The campaign builds have no
/// backend of their own: `av_hwdevice_ctx_alloc` and `av_hwdevice_ctx_create`
/// are compared on their failures, and the Rust side adopts the test
/// devices through `DeviceReferenceBox::from_c`.
mod contexts {
    use core::ffi::{CStr, c_int};
    use core::ptr::{self, null_mut};

    use ffmpeg_bindings::libavutil::libavutil::dict::Dict;
    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::hwcontext::{
        DeviceReference, DeviceReferenceBox, DeviceType, FramesReferenceBox, HardwareFrame,
        MapFlags,
    };
    use ffmpeg_bindings::libavutil::libavutil::pixfmt::PixelFormat;

    use super::backend::{self as fx, Event, FramesHw, FramesHwctx, HWCONFIG_SIZE, Kind, averror};
    use super::{C, NAMED, UNNAMED, c_find, c_name, c_next};

    const ENOSYS: c_int = averror(ffi::ENOSYS);

    /// The count of `buf`'s buffer.
    fn count(buf: *const ffi::AVBufferRef) -> c_int {
        // SAFETY: callers pass live references.
        unsafe { fx::ref_count(buf) }
    }

    /// A C-side test device of `kind`, initialised.
    fn c_device(kind: Kind) -> *mut ffi::AVBufferRef {
        fx::device(kind)
    }

    /// A Rust-side test device of `kind`, initialised by C and adopted.
    fn rust_device(kind: Kind) -> DeviceReferenceBox {
        // SAFETY: the only reference to an initialised test device context,
        // built as `av_hwdevice_ctx_alloc` builds one.
        unsafe { DeviceReferenceBox::from_c(fx::device(kind)) }.unwrap()
    }

    /// Fills a fresh frames context for `kind` and sets its backend results.
    fn fill(
        ctx: *mut ffi::AVHWFramesContext,
        kind: Kind,
        size: (c_int, c_int),
        pool: c_int,
        hwctx: FramesHwctx,
    ) {
        // For the unsafe operations below: callers pass a fresh, uninitialised context they alone
        // hold.
        {
            // SAFETY: as the comment above this block states.
            unsafe { fx::fill_frames(ctx, kind, size.0, size.1, pool) };
            // SAFETY: as the comment above this block states.
            unsafe { fx::set_frames_hwctx(ctx, hwctx) };
        }
    }

    /// C: a frames context on `device`, filled and initialised; the
    /// reference, or C's error after releasing it.
    fn c_frames(
        device: *mut ffi::AVBufferRef,
        kind: Kind,
        size: (c_int, c_int),
        pool: c_int,
        hwctx: FramesHwctx,
    ) -> Result<*mut ffi::AVBufferRef, c_int> {
        // SAFETY: a live device reference.
        let buf = unsafe { ffi::av_hwframe_ctx_alloc(device) };
        assert!(!buf.is_null());
        // SAFETY: the fresh context `buf` refers to.
        fill(unsafe { fx::frames_ctx(buf) }, kind, size, pool, hwctx);
        // SAFETY: a fresh, filled frames context.
        let ret = unsafe { ffi::av_hwframe_ctx_init(buf) };
        if ret < 0 {
            // SAFETY: the reference `buf` holds.
            unsafe { fx::unref(buf) };
            Err(ret)
        } else {
            Ok(buf)
        }
    }

    /// Rust: the same through the wrappers.
    fn rust_frames(
        device: &DeviceReferenceBox,
        kind: Kind,
        size: (c_int, c_int),
        pool: c_int,
        hwctx: FramesHwctx,
    ) -> Result<FramesReferenceBox, c_int> {
        let mut frames = device.as_ref().alloc_frames().unwrap();
        let mut ctx = frames.context_mut();
        ctx.set_format(PixelFormat::from_raw(kind.format()));
        ctx.set_sw_format(PixelFormat::NV12);
        ctx.set_width(size.0);
        ctx.set_height(size.1);
        ctx.set_initial_pool_size(pool);
        // SAFETY: the test backends' frames struct is a `FramesHwctx`, any
        // of whose results they take.
        unsafe { ctx.into_hwctx::<FramesHw>() }.unwrap().set(hwctx);
        frames.init()
    }

    /// C: a hardware frame of `frames`, or C's error.
    fn c_get_buffer(frames: *mut ffi::AVBufferRef) -> Result<*mut ffi::AVFrame, c_int> {
        // For the unsafe operations below: allocation, then a live frames context and a fresh
        // frame.
        {
            // SAFETY: as the comment above this block states.
            let mut frame = unsafe { ffi::av_frame_alloc() };
            // SAFETY: as the comment above this block states.
            let ret = unsafe { ffi::av_hwframe_get_buffer(frames, frame, 0) };
            // SAFETY: as the comment above this block states.
            unsafe {
                if ret < 0 {
                    ffi::av_frame_free(&raw mut frame);
                    Err(ret)
                } else {
                    Ok(frame)
                }
            }
        }
    }

    /// C: `src` mapped into a fresh frame of `format` (and frames context
    /// `frames`, if any), or C's error.
    fn c_map(
        src: *const ffi::AVFrame,
        format: c_int,
        frames: *mut ffi::AVBufferRef,
        flags: MapFlags,
    ) -> Result<*mut ffi::AVFrame, c_int> {
        // For the unsafe operations below: allocation, then a live source frame and a fresh one
        // with a format and, when given, a new reference to a frames context.
        {
            // SAFETY: as the comment above this block states.
            let mut dst = unsafe { ffi::av_frame_alloc() };
            // SAFETY: as the comment above this block states.
            unsafe { (*dst).format = format };
            if !frames.is_null() {
                // SAFETY: as the comment above this block states: a live reference, to which C
                // adds one.
                let frames = unsafe { ffi::av_buffer_ref(frames) };
                // SAFETY: as the comment above this block states: the frame takes it.
                unsafe { (*dst).hw_frames_ctx = frames };
            }
            // SAFETY: as the comment above this block states.
            let ret = unsafe { ffi::av_hwframe_map(dst, src, flags.bits()) };
            if ret < 0 {
                // SAFETY: as the comment above this block states, released once.
                unsafe { ffi::av_frame_free(&raw mut dst) };
                Err(ret)
            } else {
                Ok(dst)
            }
        }
    }

    fn free_frame(mut frame: *mut ffi::AVFrame) {
        // SAFETY: callers pass a frame they own.
        unsafe { ffi::av_frame_free(&raw mut frame) }
    }

    /// The format, width and height of a C frame.
    fn c_dims(frame: *const ffi::AVFrame) -> (c_int, c_int, c_int) {
        // SAFETY: callers pass a live frame.
        let s = unsafe { fx::shape(frame) };
        (s.format, s.width, s.height)
    }

    fn rust_dims(frame: &HardwareFrame) -> (c_int, c_int, c_int) {
        (frame.format().into_raw().0, frame.width(), frame.height())
    }

    /// The frames context a C frame holds.
    fn c_frames_of(frame: *const ffi::AVFrame) -> *const u8 {
        // SAFETY: as the comment below states, for this part of the operation.
        let hw_frames_ctx_2 = unsafe { (*frame).hw_frames_ctx };
        // SAFETY: callers pass a live frame with a frames context.
        unsafe { (*hw_frames_ctx_2).data.cast_const() }
    }

    fn data(buf: *const ffi::AVBufferRef) -> *const u8 {
        // SAFETY: callers pass a live reference.
        unsafe { (*buf).data.cast_const() }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn names_and_lookups_match_c() {
        for raw in (0..=48).chain(UNNAMED) {
            let t = DeviceType::from_raw(C(raw));
            assert_eq!(
                t.name().map(|n| n.to_str().unwrap()),
                c_name(C(raw)),
                "{raw}"
            );
        }
        let unknown: [&CStr; 6] = [c"", c"none", c"CUDA", c"vaapi ", c"vulkan2", c"\xff"];
        let names = NAMED.iter().filter_map(|&(t, _, _)| t.name());
        for name in names.chain(unknown) {
            let c = c_find(name);
            let rust = DeviceType::from_name(name);
            assert_eq!(rust.unwrap_or(DeviceType::NONE), c, "{name:?}");
            assert_eq!(rust.is_none(), c == DeviceType::NONE);
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn supported_types_are_cs_walk() {
        let mut c = Vec::new();
        let mut prev = DeviceType::NONE;
        loop {
            let next = c_next(prev.into_raw());
            if next == DeviceType::NONE {
                break;
            }
            c.push(next);
            prev = next;
        }
        assert_eq!(DeviceType::supported().collect::<Vec<_>>(), c);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn allocation_and_creation_fail_as_in_c() {
        let options = Dict::from_entries([(c"connect", c"1")]).unwrap();
        let types = NAMED
            .iter()
            .map(|&(t, _, _)| t)
            .chain(UNNAMED.map(|r| DeviceType::from_raw(C(r))));
        for t in types {
            // SAFETY: by value.
            let c = unsafe { ffi::av_hwdevice_ctx_alloc(t.into_raw()) };
            assert_eq!(DeviceReference::alloc(t).is_none(), c.is_null(), "{t:?}");
            // SAFETY: NULL, or the reference C returned.
            unsafe { fx::unref(c) };
            for device in [None, Some(c"/dev/dri/renderD128"), Some(c"")] {
                for opts in [&Dict::new(), &options] {
                    let mut out = ptr::dangling_mut();
                    let raw_opts = opts.iter().next().map_or(null_mut(), |_| {
                        let mut d = null_mut();
                        let (k, v) = (c"connect".as_ptr(), c"1".as_ptr());
                        // SAFETY: a fresh dictionary slot and live strings.
                        let ret = unsafe { ffi::av_dict_set(&raw mut d, k, v, 0) };
                        assert_eq!(ret, 0);
                        d
                    });
                    // SAFETY: a local slot, NULL or live strings, a C
                    // dictionary freed below.
                    let ret = unsafe {
                        ffi::av_hwdevice_ctx_create(
                            &raw mut out,
                            t.into_raw(),
                            device.map_or(ptr::null(), CStr::as_ptr),
                            raw_opts,
                            0,
                        )
                    };
                    let mut raw_opts = raw_opts;
                    // SAFETY: the dictionary built above.
                    unsafe { ffi::av_dict_free(&raw mut raw_opts) };
                    assert!(out.is_null());
                    let rust = DeviceReference::create(t, device, opts);
                    assert_eq!(rust.err(), Some(ret), "{t:?} {device:?}");
                }
            }
        }
    }

    /// The device of a chain a derivation returned: its index in `chain`.
    fn which(found: *const u8, chain: &[*const u8]) -> usize {
        chain
            .iter()
            .position(|&d| d == found)
            .expect("a device of the chain")
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn derivations_walk_the_chain_as_in_c() {
        let options = Dict::from_entries([(c"k", c"v")]).unwrap();
        let (ca, cb) = (c_device(Kind::A), c_device(Kind::B));
        let (ra, rb) = (rust_device(Kind::A), rust_device(Kind::B));
        // For the unsafe operations below: live test devices, not derived yet.
        {
            // SAFETY: as the comment above this block states.
            unsafe { fx::set_source_device(cb, ca) };
            // SAFETY: as the comment above this block states.
            unsafe { fx::set_source_device(rb.as_c_ptr(), ra.as_c_ptr()) };
        }
        let c_chain = [data(ca), data(cb)];
        let r_chain = [data(ra.as_c_ptr()), data(rb.as_c_ptr())];
        let mut c_held = Vec::new();
        let mut r_held = Vec::new();
        let types = [
            DeviceType::VAAPI,
            DeviceType::DRM,
            DeviceType::CUDA,
            DeviceType::NONE,
        ];
        for (step, &t) in types.iter().cycle().take(12).enumerate() {
            for src in 0..2 {
                let with_options = step % 3 == 1;
                let c_src = [ca, cb][src];
                let mut out = null_mut();
                // For the unsafe operations below: a local slot and a live device reference.
                let ret = if with_options {
                    let mut d = null_mut();
                    // SAFETY: as the comment at the head of this block states.
                    unsafe { ffi::av_dict_set(&raw mut d, c"k".as_ptr(), c"v".as_ptr(), 0) };
                    // SAFETY: as the comment at the head of this block states.
                    let ret = unsafe {
                        ffi::av_hwdevice_ctx_create_derived_opts(
                            &raw mut out,
                            t.into_raw(),
                            c_src,
                            d,
                            0,
                        )
                    };
                    // SAFETY: as the comment at the head of this block states.
                    unsafe { ffi::av_dict_free(&raw mut d) };
                    ret
                } else {
                    // SAFETY: as the comment at the head of this block states.
                    unsafe {
                        ffi::av_hwdevice_ctx_create_derived(&raw mut out, t.into_raw(), c_src, 0)
                    }
                };
                let r_src = [&ra, &rb][src].as_ref();
                let rust = if with_options {
                    r_src.derive_with_options(t, &options)
                } else {
                    r_src.derive(t)
                };
                match rust {
                    Ok(dev) => {
                        assert_eq!(ret, 0, "{t:?} from {src}");
                        assert_eq!(
                            which(data(out), &c_chain),
                            which(data(dev.as_c_ptr()), &r_chain)
                        );
                        c_held.push(out);
                        r_held.push(dev);
                    }
                    Err(e) => {
                        assert_eq!(e, ret, "{t:?} from {src}");
                        assert!(out.is_null());
                    }
                }
                assert_eq!(
                    [count(ca), count(cb)],
                    [count(ra.as_c_ptr()), count(rb.as_c_ptr())],
                    "step {step}"
                );
            }
        }
        assert!(!r_held.is_empty());
        for buf in c_held {
            // SAFETY: references C returned.
            unsafe { fx::unref(buf) };
        }
        drop(r_held);
        assert_eq!(
            [count(ca), count(cb)],
            [count(ra.as_c_ptr()), count(rb.as_c_ptr())]
        );
        // For the unsafe operations below: the test's own references.
        {
            // SAFETY: as the comment above this block states.
            unsafe { fx::unref(cb) };
            // SAFETY: as the comment above this block states.
            unsafe { fx::unref(ca) };
        }
        let c_events = fx::take_events();
        drop((rb, ra));
        assert_eq!(fx::take_events(), c_events);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn frames_allocation_matches_c() {
        let c_dev = c_device(Kind::A);
        let r_dev = rust_device(Kind::A);
        // SAFETY: a live device reference.
        let c = unsafe { ffi::av_hwframe_ctx_alloc(c_dev) };
        let rust = r_dev.as_ref().alloc_frames().unwrap();
        assert_eq!(count(c_dev), count(r_dev.as_c_ptr()));
        // SAFETY: as the comment below states, for this part of the operation.
        let frames_ctx_2 = unsafe { fx::frames_ctx(c) };
        // SAFETY: the fresh context's public struct.
        let cc = unsafe { *frames_ctx_2 };
        let rc = rust.context();
        assert_eq!(
            (cc.format, cc.sw_format),
            (rc.format().into_raw(), rc.sw_format().into_raw())
        );
        assert_eq!(
            (cc.width, cc.height, cc.initial_pool_size),
            (rc.width(), rc.height(), rc.initial_pool_size())
        );
        assert!(cc.pool.is_null() && !rc.has_pool());
        // SAFETY: the test backends' frames struct is a `FramesHwctx`.
        let rust_hw = unsafe { rc.hwctx::<FramesHw>() }.map(|h| h.get());
        assert!(!cc.hwctx.is_null() && rust_hw.is_some());
        assert_eq!(data(cc.device_ref), data(c_dev));
        assert!(rc.device().ptr_eq(r_dev.as_ref()));
        assert_eq!(cc.device_ctx.cast_const().cast(), data(c_dev));
        assert_eq!(rc.device_context().device_type(), DeviceType::VAAPI);
        assert_eq!(cc.av_class.is_null(), rc.class().is_none());
        // SAFETY: the reference C returned.
        unsafe { fx::unref(c) };
        let c_events = fx::take_events();
        drop(rust);
        assert_eq!(fx::take_events(), c_events);
        assert_eq!(count(c_dev), count(r_dev.as_c_ptr()));
        // SAFETY: the test's own reference.
        unsafe { fx::unref(c_dev) };
        let c_events = fx::take_events();
        drop(r_dev);
        assert_eq!(fx::take_events(), c_events);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn frames_init_and_allocation_match_c() {
        let sizes = [
            (64, 32),
            (0, 8),
            (8, 0),
            (1, 1),
            (-1, 4),
            (16384, 16384),
            (c_int::MAX, 2),
        ];
        let results = [0, -9, ENOSYS, 3];
        let c_dev = c_device(Kind::A);
        let r_dev = rust_device(Kind::A);
        for kind in [Kind::A, Kind::B] {
            for size in sizes {
                for pool in [0, 2] {
                    for init_ret in results {
                        for get_buffer_ret in [0, -4] {
                            let hwctx = FramesHwctx {
                                init_ret,
                                get_buffer_ret,
                                ..FramesHwctx::default()
                            };
                            let c = c_frames(c_dev, kind, size, pool, hwctx);
                            let c_events = fx::take_events();
                            let rust = rust_frames(&r_dev, kind, size, pool, hwctx);
                            assert_eq!(
                                fx::take_events(),
                                c_events,
                                "{kind:?} {size:?} {pool} {init_ret}"
                            );
                            let label =
                                format!("{kind:?} {size:?} {pool} {init_ret} {get_buffer_ret}");
                            match (c, rust) {
                                (Ok(cf), Ok(rf)) => {
                                    compare_allocations(cf, &rf, &label);
                                    // SAFETY: the reference `c_frames` returned.
                                    unsafe { fx::unref(cf) };
                                    let c_events = fx::take_events();
                                    drop(rf);
                                    assert_eq!(fx::take_events(), c_events, "{label}");
                                }
                                (c, rust) => assert_eq!(c.err(), rust.err(), "{label}"),
                            }
                            assert_eq!(count(c_dev), count(r_dev.as_c_ptr()));
                        }
                    }
                }
            }
        }
        // SAFETY: the test's own reference.
        unsafe { fx::unref(c_dev) };
        let c_events = fx::take_events();
        drop(r_dev);
        assert_eq!(fx::take_events(), c_events);
    }

    /// Allocates three frames from each context, comparing every step.
    fn compare_allocations(cf: *mut ffi::AVBufferRef, rf: &FramesReferenceBox, label: &str) {
        let mut c_frames = Vec::new();
        let mut r_frames = Vec::new();
        for i in 0..3 {
            let c = c_get_buffer(cf);
            let c_events = fx::take_events();
            let rust = rf.as_ref().get_buffer();
            assert_eq!(fx::take_events(), c_events, "{label} #{i}");
            match (c, rust) {
                (Ok(c), Ok(r)) => {
                    assert_eq!(c_dims(c), rust_dims(&r), "{label} #{i}");
                    assert_eq!(c_frames_of(c), data(cf));
                    assert!(r.frames().ptr_eq(rf.as_ref()));
                    c_frames.push(c);
                    r_frames.push(r);
                }
                (c, rust) => assert_eq!(c.err(), rust.err(), "{label} #{i}"),
            }
            assert_eq!(count(cf), count(rf.as_c_ptr()), "{label} #{i}");
        }
        c_frames.into_iter().for_each(free_frame);
        drop(r_frames);
        assert_eq!(count(cf), count(rf.as_c_ptr()), "{label}");
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn mappings_to_memory_match_c() {
        let flag_sets = [
            MapFlags::empty(),
            MapFlags::READ,
            MapFlags::READ | MapFlags::WRITE,
            MapFlags::OVERWRITE | MapFlags::DIRECT,
        ];
        let formats = [
            PixelFormat::NONE,
            PixelFormat::NV12,
            PixelFormat::YUV420P,
            PixelFormat::VAAPI,
        ];
        let c_dev = c_device(Kind::A);
        let r_dev = rust_device(Kind::A);
        for map_ret in [0, ENOSYS, -3] {
            let hwctx = FramesHwctx {
                map_ret,
                ..FramesHwctx::default()
            };
            let cf = c_frames(c_dev, Kind::A, (32, 16), 0, hwctx).unwrap();
            let rf = rust_frames(&r_dev, Kind::A, (32, 16), 0, hwctx).unwrap();
            let cs = c_get_buffer(cf).unwrap();
            let rs = rf.as_ref().get_buffer().unwrap();
            fx::take_events();
            let mut c_maps = Vec::new();
            let mut r_maps = Vec::new();
            for format in formats {
                for flags in flag_sets {
                    let c = c_map(cs, format.into_raw().0, null_mut(), flags);
                    let c_events = fx::take_events();
                    let rust = rs.map(format, flags);
                    assert_eq!(fx::take_events(), c_events);
                    match (c, rust) {
                        (Ok(c), Ok(r)) => {
                            assert_eq!(c_dims(c), (r.format().into_raw().0, r.width(), r.height()));
                            c_maps.push(c);
                            r_maps.push(r);
                        }
                        (c, rust) => {
                            assert_eq!(c.err(), rust.err(), "{format:?} {flags:?} {map_ret}");
                        }
                    }
                    assert_eq!(count(cf), count(rf.as_c_ptr()));
                }
            }
            // The mappings outlive the source frame and its context's box.
            free_frame(cs);
            // SAFETY: the reference `c_frames` returned.
            unsafe { fx::unref(cf) };
            let c_events = fx::take_events();
            drop((rs, rf));
            assert_eq!(fx::take_events(), c_events);
            assert_eq!(c_events.is_empty(), !c_maps.is_empty());
            c_maps.into_iter().for_each(free_frame);
            let c_events = fx::take_events();
            drop(r_maps);
            assert_eq!(fx::take_events(), c_events);
        }
        // SAFETY: the test's own reference.
        unsafe { fx::unref(c_dev) };
        let c_events = fx::take_events();
        drop(r_dev);
        assert_eq!(fx::take_events(), c_events);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn mappings_between_contexts_and_derivations_match_c() {
        let flags = MapFlags::READ;
        let configs = [
            (0, 0, 0),
            (ENOSYS, ENOSYS, 0),
            (0, -21, 0),
            (-3, 0, 0),
            (ENOSYS, 0, -7),
        ];
        for (map_ret, derive_ret, b_map_ret) in configs {
            let label = format!("{map_ret} {derive_ret} {b_map_ret}");
            let hwctx = FramesHwctx {
                map_ret,
                derive_ret,
                ..FramesHwctx::default()
            };
            let b_hwctx = FramesHwctx {
                map_ret: b_map_ret,
                ..FramesHwctx::default()
            };
            let (ca, cb) = (c_device(Kind::A), c_device(Kind::B));
            let (ra, rb) = (rust_device(Kind::A), rust_device(Kind::B));
            let cfa = c_frames(ca, Kind::A, (16, 16), 0, hwctx).unwrap();
            let rfa = rust_frames(&ra, Kind::A, (16, 16), 0, hwctx).unwrap();
            let cfb = c_frames(cb, Kind::B, (16, 16), 0, b_hwctx).unwrap();
            let rfb = rust_frames(&rb, Kind::B, (16, 16), 0, b_hwctx).unwrap();
            let cs = c_get_buffer(cfa).unwrap();
            let rs = rfa.as_ref().get_buffer().unwrap();
            fx::take_events();
            let c_counts = || [count(ca), count(cb), count(cfa), count(cfb)];
            let r_counts = || {
                [
                    count(ra.as_c_ptr()),
                    count(rb.as_c_ptr()),
                    count(rfa.as_c_ptr()),
                    count(rfb.as_c_ptr()),
                ]
            };

            // Into B's context.
            let c = c_map(cs, Kind::B.format().0, cfb, flags);
            let c_events = fx::take_events();
            let rust = rs.map_to(rfb.as_ref(), flags);
            assert_eq!(fx::take_events(), c_events, "{label}");
            assert_eq!(c.is_ok(), rust.is_ok(), "{label}");
            if let (Ok(c), Ok(r)) = (&c, &rust) {
                assert_eq!(c_dims(*c), rust_dims(r));
                assert_eq!(c_frames_of(*c), data(cfb));
                assert!(r.frames().ptr_eq(rfb.as_ref()));
            } else {
                assert_eq!(c.err(), rust.as_ref().err().copied(), "{label}");
            }
            assert_eq!(c_counts(), r_counts(), "{label}");

            // A frames context derived from A's onto B's device.
            let mut cd = null_mut();
            // SAFETY: a local slot and live references.
            let ret = unsafe {
                ffi::av_hwframe_ctx_create_derived(
                    &raw mut cd,
                    Kind::B.format(),
                    cb,
                    cfa,
                    flags.bits(),
                )
            };
            let c_events = fx::take_events();
            let rd = rfa
                .as_ref()
                .derive(PixelFormat::DRM_PRIME, rb.as_ref(), flags);
            assert_eq!(fx::take_events(), c_events, "{label}");
            assert_eq!(
                rd.as_ref().err().copied(),
                (ret < 0).then_some(ret),
                "{label}"
            );
            assert_eq!(c_counts(), r_counts(), "{label}");
            if let Ok(rd) = &rd {
                // Allocations in the derived context, then their unmapping
                // back into A's, and the derivation back onto A's device.
                let c2 = c_get_buffer(cd);
                let c_events = fx::take_events();
                let r2 = rd.as_ref().get_buffer();
                assert_eq!(fx::take_events(), c_events, "{label}");
                assert_eq!(c2.as_ref().err(), r2.as_ref().err(), "{label}");
                if let (Ok(c2), Ok(r2)) = (c2, r2) {
                    assert_eq!(c_dims(c2), rust_dims(&r2));
                    let cu = c_map(c2, Kind::A.format().0, cfa, flags).unwrap();
                    let ru = r2.map_to(rfa.as_ref(), flags).unwrap();
                    assert_eq!(c_dims(cu), rust_dims(&ru));
                    assert_eq!(c_frames_of(cu), data(cfa));
                    assert!(ru.frames().ptr_eq(rfa.as_ref()));
                    assert_eq!(fx::take_events(), []);
                    let mut cback = null_mut();
                    // SAFETY: a local slot and live references.
                    let ret = unsafe {
                        ffi::av_hwframe_ctx_create_derived(
                            &raw mut cback,
                            Kind::A.format(),
                            ca,
                            cd,
                            0,
                        )
                    };
                    let rback = rd
                        .as_ref()
                        .derive(PixelFormat::VAAPI, ra.as_ref(), MapFlags::empty())
                        .unwrap();
                    assert_eq!(ret, 0);
                    assert_eq!(data(cback), data(cfa));
                    assert!(rback.as_ref().ptr_eq(rfa.as_ref()));
                    assert_eq!(c_counts(), r_counts(), "{label}");
                    // SAFETY: the reference C returned.
                    unsafe { fx::unref(cback) };
                    drop(rback);
                    for f in [cu, c2] {
                        free_frame(f);
                    }
                    let c_events = fx::take_events();
                    drop((ru, r2));
                    assert_eq!(fx::take_events(), c_events, "{label}");
                }
            }
            if let Ok(c) = c {
                free_frame(c);
            }
            free_frame(cs);
            // For the unsafe operations below: NULL or the references C returned, and the test's
            // own.
            {
                // SAFETY: as the comment above this block states.
                unsafe { fx::unref(cd) };
                // SAFETY: as the comment above this block states.
                unsafe {
                    for buf in [cfb, cfa, cb, ca] {
                        fx::unref(buf);
                    }
                }
            }
            let c_events = fx::take_events();
            drop((rust, rs, rd, rfb, rfa, rb, ra));
            assert_eq!(fx::take_events(), c_events, "{label}");
        }
    }

    /// The wrapper refuses to map a frame into its own context, which C
    /// would take for an unmapping when the frame's format is the
    /// context's software format and read the pool's buffer as a mapping
    /// descriptor. With the hardware format, C maps it like any other.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn mapping_into_the_own_context_is_refused() {
        let ca = c_device(Kind::A);
        let ra = rust_device(Kind::A);
        let cf = c_frames(ca, Kind::A, (8, 8), 0, FramesHwctx::default()).unwrap();
        let rf = rust_frames(&ra, Kind::A, (8, 8), 0, FramesHwctx::default()).unwrap();
        let cs = c_get_buffer(cf).unwrap();
        let rs = rf.as_ref().get_buffer().unwrap();
        fx::take_events();
        let c = c_map(cs, Kind::A.format().0, cf, MapFlags::READ).unwrap();
        assert_eq!(fx::take_events(), [Event::MapFrom(Kind::A)]);
        assert_eq!(
            rs.map_to(rf.as_ref(), MapFlags::READ).err(),
            Some(averror(ffi::EINVAL))
        );
        assert!(fx::take_events().is_empty());
        for f in [c, cs] {
            free_frame(f);
        }
        // For the unsafe operations below: the test's own references.
        {
            // SAFETY: as the comment above this block states.
            unsafe { fx::unref(cf) };
            // SAFETY: as the comment above this block states.
            unsafe { fx::unref(ca) };
        }
        fx::take_events();
        drop((rs, rf, ra));
        fx::take_events();
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn hw_configs_match_c() {
        for kind in [Kind::A, Kind::B] {
            let c_dev = c_device(kind);
            let r_dev = rust_device(kind);
            // SAFETY: a live device reference.
            let c = unsafe { ffi::av_hwdevice_hwconfig_alloc(c_dev) };
            let rust = r_dev.as_ref().alloc_hw_config();
            assert_eq!(c.is_null(), rust.is_none());
            if let Some(r) = &rust {
                assert_eq!(r.device_type().into_raw(), kind.device_type());
                // SAFETY: two blocks of the backend's configuration size.
                let (cb, rb) = (
                    unsafe { core::slice::from_raw_parts(c.cast::<u8>(), HWCONFIG_SIZE) },
                    unsafe { core::slice::from_raw_parts(r.as_ptr().cast::<u8>(), HWCONFIG_SIZE) },
                );
                assert_eq!(cb, rb);
            }
            // SAFETY: NULL or the block C returned.
            unsafe { ffi::av_free(c) };
            drop(rust);
            // SAFETY: the test's own reference.
            unsafe { fx::unref(c_dev) };
            let c_events = fx::take_events();
            drop(r_dev);
            assert_eq!(fx::take_events(), c_events);
        }
    }
}

/// [`HardwareFrame`]'s transfers ([`download_to`], [`upload_from`],
/// [`copy_from`]) and [`FramesReferenceRef::transfer_formats`] against
/// `av_hwframe_transfer_data` and `av_hwframe_transfer_get_formats` (hwcontext
/// wave-1 symbol review), over independent frames contexts of kind A of
/// `tests/common/hwbackend.rs`, whose transfers run over the software frame's
/// image unchecked; kind B transfers nothing, which C would call through NULL,
/// so only its listing runs in C. Results, downloaded images, frame shapes
/// and the backends' event logs are compared after each step.
///
/// [`HardwareFrame`]: ffmpeg_bindings::libavutil::libavutil::hwcontext::HardwareFrame
/// [`download_to`]: ffmpeg_bindings::libavutil::libavutil::hwcontext::HardwareFrame::download_to
/// [`upload_from`]: ffmpeg_bindings::libavutil::libavutil::hwcontext::HardwareFrame::upload_from
/// [`copy_from`]: ffmpeg_bindings::libavutil::libavutil::hwcontext::HardwareFrame::copy_from
/// [`FramesReferenceRef::transfer_formats`]: ffmpeg_bindings::libavutil::libavutil::hwcontext::FramesReferenceRef::transfer_formats
mod hardware_frame_transfers {
    use core::ffi::c_int;
    use core::ptr;

    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::frame::{Frame, FrameBox};
    use ffmpeg_bindings::libavutil::libavutil::hwcontext::{
        DeviceReferenceBox, FramesReferenceBox, HardwareFrame, MapFlags, TransferDirection,
    };
    use ffmpeg_bindings::libavutil::libavutil::pixfmt::PixelFormat;

    use super::backend::{self as fx, Event, FramesHw, FramesHwctx, Kind, averror};

    const W: c_int = 64;
    const H: c_int = 32;
    const ENOSYS: c_int = averror(ffi::ENOSYS);

    /// Both sides' frames context of `kind` at `W` x `H` on a device of its
    /// own, and its device: C's references, Rust's boxes.
    struct Contexts {
        c_dev: *mut ffi::AVBufferRef,
        c: *mut ffi::AVBufferRef,
        r_dev: DeviceReferenceBox,
        r: FramesReferenceBox,
    }

    impl Contexts {
        fn new(kind: Kind, hwctx: FramesHwctx) -> Self {
            let c_dev = fx::device(kind);
            // SAFETY: the only reference to an initialised test device.
            let r_dev = unsafe { DeviceReferenceBox::from_c(fx::device(kind)) }.unwrap();
            // For the unsafe operations below: a live device; then the fresh frames context it
            // made, filled and initialised.
            let c = {
                // SAFETY: as the comment above this block states.
                let c = unsafe { ffi::av_hwframe_ctx_alloc(c_dev) };
                assert!(!c.is_null());
                // SAFETY: as the comment below states, for this part of the operation.
                let frames_ctx_3 = unsafe { fx::frames_ctx(c) };
                // SAFETY: as the comment above this block states.
                unsafe { fx::fill_frames(frames_ctx_3, kind, W, H, 0) };
                // SAFETY: as the comment below states, for this part of the operation.
                let frames_ctx_2 = unsafe { fx::frames_ctx(c) };
                // SAFETY: as the comment above this block states.
                unsafe { fx::set_frames_hwctx(frames_ctx_2, hwctx) };
                // SAFETY: as the comment above this block states.
                unsafe { assert_eq!(ffi::av_hwframe_ctx_init(c), 0) };
                c
            };
            let mut r = r_dev.as_ref().alloc_frames().unwrap();
            let mut ctx = r.context_mut();
            ctx.set_format(PixelFormat::from_raw(kind.format()));
            ctx.set_sw_format(PixelFormat::NV12);
            ctx.set_width(W);
            ctx.set_height(H);
            // SAFETY: the test backends' frames struct is a `FramesHwctx`,
            // any of whose results they take.
            unsafe { ctx.into_hwctx::<FramesHw>() }.unwrap().set(hwctx);
            let r = r.init().unwrap();
            let events = fx::take_events();
            assert_eq!(events, [Event::FramesInit(kind), Event::FramesInit(kind)]);
            Self { c_dev, c, r_dev, r }
        }

        /// A hardware frame of each side's context.
        fn get_buffer(&self) -> (CFrame, HardwareFrame) {
            let c = CFrame::alloc();
            // SAFETY: a live frames context and a fresh frame.
            assert_eq!(unsafe { ffi::av_hwframe_get_buffer(self.c, c.0, 0) }, 0);
            let r = self.r.as_ref().get_buffer().unwrap();
            let events = fx::take_events();
            assert_eq!(events.len(), 2);
            assert_eq!(events[0], events[1]);
            (c, r)
        }
    }

    impl Drop for Contexts {
        fn drop(&mut self) {
            // For the unsafe operations below: the test's own references.
            {
                // SAFETY: as the comment above this block states.
                unsafe { fx::unref(self.c) };
                // SAFETY: as the comment above this block states.
                unsafe { fx::unref(self.c_dev) };
            }
        }
    }

    /// An `AVFrame` C owns, freed on drop.
    struct CFrame(*mut ffi::AVFrame);

    impl CFrame {
        fn alloc() -> Self {
            // SAFETY: allocation.
            let f = unsafe { ffi::av_frame_alloc() };
            assert!(!f.is_null());
            Self(f)
        }
    }

    impl Drop for CFrame {
        fn drop(&mut self) {
            // SAFETY: a frame this test owns.
            unsafe { ffi::av_frame_free(&raw mut self.0) };
        }
    }

    /// The byte at row `r`, column `x` of plane `i` of the image `seed`
    /// names.
    fn byte(seed: u8, i: usize, r: usize, x: usize) -> u8 {
        seed.wrapping_add((r * 31 + x * 7 + i) as u8)
    }

    /// The planes and rows of a `w` x `h` NV12 image.
    fn planes(h: c_int) -> [(usize, usize); 2] {
        [(0, h as usize), (1, (h as usize).div_ceil(2))]
    }

    /// An NV12 software frame of `w` x `h` for each side, given buffers by
    /// `av_frame_get_buffer` and [`alloc_buffers`] (zeroed), its image then
    /// holding `seed`'s bytes.
    ///
    /// [`alloc_buffers`]: ffmpeg_bindings::libavutil::libavutil::frame::FrameMut::alloc_buffers
    fn software(w: c_int, h: c_int, seed: u8) -> (CFrame, FrameBox) {
        let c = CFrame::alloc();
        // For the unsafe operations below: a fresh frame given a format, size and buffers, whose
        // image rows are then written inside its planes.
        {
            // SAFETY: as the comment above this block states.
            unsafe { (*c.0).format = ffi::AVPixelFormat::AV_PIX_FMT_NV12.0 };
            // SAFETY: as the comment above this block states.
            unsafe { (*c.0).width = w };
            // SAFETY: as the comment above this block states.
            unsafe { (*c.0).height = h };
            // SAFETY: as the comment above this block states.
            unsafe { assert_eq!(ffi::av_frame_get_buffer(c.0, 0), 0) };
            for (i, rows) in planes(h) {
                // SAFETY: as the comment above this block states: reads of plane `i` and its
                // line size.
                let (plane, ls) = (unsafe { (*c.0).data[i] }, unsafe { (*c.0).linesize[i] });
                for r in 0..rows {
                    // SAFETY: as above: row `r` of the plane.
                    let row = unsafe { plane.offset(r as isize * ls as isize) };
                    for x in 0..w as usize {
                        // SAFETY: as above: byte `x` of the row.
                        let at = unsafe { row.add(x) };
                        // SAFETY: as above.
                        unsafe { at.write(byte(seed, i, r, x)) };
                    }
                }
            }
        }
        let mut f = Frame::new().unwrap();
        let mut m = f.as_mut();
        m.set_pixel_format(PixelFormat::NV12);
        m.set_width(w);
        m.set_height(h);
        m.alloc_buffers(0).unwrap();
        let lines = m.as_ref().linesizes();
        for (i, rows) in planes(h) {
            let ls = lines[i] as usize;
            let plane = m.plane_mut(i).unwrap();
            for r in 0..rows {
                for x in 0..w as usize {
                    plane[r * ls + x] = byte(seed, i, r, x);
                }
            }
        }
        (c, f)
    }

    /// The `W` x `H` NV12 image of each side's software frame.
    fn images(c: &CFrame, r: &FrameBox) -> (Vec<u8>, Vec<u8>) {
        let mut ci = Vec::new();
        for (i, rows) in planes(H) {
            // SAFETY: a live frame: reads of plane `i` and its line size.
            let (plane, ls) = (unsafe { (*c.0).data[i] }, unsafe { (*c.0).linesize[i] });
            for row in 0..rows {
                // SAFETY: as above: row `row` of the plane.
                let p = unsafe { plane.offset(row as isize * ls as isize) };
                // SAFETY: as above: the plane holds the image, initialised.
                ci.extend_from_slice(unsafe { core::slice::from_raw_parts(p, W as usize) });
            }
        }
        let f = r.as_ref();
        let (lines, mut ri) = (f.linesizes(), Vec::new());
        for (i, rows) in planes(H) {
            let (ls, plane) = (lines[i] as usize, f.plane(i).unwrap());
            for row in 0..rows {
                ri.extend_from_slice(&plane[row * ls..row * ls + W as usize]);
            }
        }
        (ci, ri)
    }

    /// The format, width and height of each side's frame.
    fn dims(c: &CFrame, r: &FrameBox) -> ((c_int, c_int, c_int), (c_int, c_int, c_int)) {
        // SAFETY: a live frame.
        let cd = (unsafe { (*c.0).format }, unsafe { (*c.0).width }, unsafe {
            (*c.0).height
        });
        let f = r.as_ref();
        (cd, (f.format(), f.width(), f.height()))
    }

    /// C's `av_hwframe_transfer_data(dst, src, 0)`.
    fn c_transfer(dst: &CFrame, src: &CFrame) -> c_int {
        // SAFETY: live frames: each hardware one a frame of its test
        // context, each software one holding the surfaces' image.
        unsafe { ffi::av_hwframe_transfer_data(dst.0, src.0, 0) }
    }

    /// A Rust result as C reports it.
    fn code(r: Result<(), c_int>) -> c_int {
        r.err().unwrap_or(0)
    }

    /// Asserts the C and Rust steps recorded the same backend events.
    fn same_events(c_events: Vec<Event>) {
        assert_eq!(fx::take_events(), c_events);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn format_listings_match_c() {
        for kind in [Kind::A, Kind::B] {
            let ctx = Contexts::new(kind, FramesHwctx::default());
            for dir in [TransferDirection::From, TransferDirection::To] {
                let mut list = ptr::null_mut();
                // SAFETY: a live frames context and a writable slot.
                let ret = unsafe {
                    ffi::av_hwframe_transfer_get_formats(ctx.c, dir.into_raw(), &raw mut list, 0)
                };
                let c = (ret >= 0).then(|| {
                    // For the unsafe operations below: the terminated list C handed over, read to
                    // its terminator and freed.
                    {
                        let mut out = Vec::new();
                        let mut p = list;
                        // SAFETY: as the comment above this block states.
                        while unsafe { *p } != ffi::AVPixelFormat::AV_PIX_FMT_NONE {
                            // SAFETY: as the comment above this statement states.
                            out.push(PixelFormat::from_raw(unsafe { *p }));
                            // SAFETY: as the comment at the head of this block states.
                            p = unsafe { p.add(1) };
                        }
                        // SAFETY: as the comment above this block states.
                        unsafe { ffi::av_free(list.cast()) };
                        out
                    }
                });
                let r = ctx.r.as_ref().transfer_formats(dir);
                assert_eq!(r.as_ref().err().copied(), (ret < 0).then_some(ret));
                assert_eq!(r.ok().map(|l| l.as_slice().to_vec()), c, "{kind:?} {dir:?}");
            }
            if kind == Kind::B {
                let (_c_hw, r_hw) = ctx.get_buffer();
                let listed = r_hw.frames().transfer_formats(TransferDirection::From);
                assert_eq!(listed.err(), Some(ENOSYS));
            }
            drop(ctx);
            fx::take_events();
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn transfer_sequences_match_c() {
        let ctx = Contexts::new(Kind::A, FramesHwctx::default());
        let (c_hw, mut r_hw) = ctx.get_buffer();
        let (c_other, mut r_other) = ctx.get_buffer();
        let (c_dst, mut r_dst) = software(W, H, 0);
        for seed in [3, 200, 77] {
            // Upload, adding into the surface.
            let src = software(W, H, seed);
            let c = c_transfer(&c_hw, &src.0);
            let c_events = fx::take_events();
            assert_eq!(code(r_hw.upload_from(src.1.as_ref())), c);
            same_events(c_events);
            // Download into buffers, and into a frame without any.
            let c = c_transfer(&c_dst, &c_hw);
            let c_events = fx::take_events();
            assert_eq!(code(r_hw.download_to(r_dst.as_mut())), c);
            same_events(c_events);
            let (ci, ri) = images(&c_dst, &r_dst);
            assert_eq!(ci, ri, "seed {seed}");
            let (c_fresh, mut r_fresh) = (CFrame::alloc(), Frame::new().unwrap());
            let c = c_transfer(&c_fresh, &c_hw);
            let c_events = fx::take_events();
            assert_eq!(code(r_hw.download_to(r_fresh.as_mut())), c);
            same_events(c_events);
            let (cd, rd) = dims(&c_fresh, &r_fresh);
            assert_eq!(cd, rd);
            let (cf, rf) = images(&c_fresh, &r_fresh);
            assert_eq!(cf, rf);
            assert_eq!(cf, ci);
            // Copy between hardware frames, then download the copy.
            let c = c_transfer(&c_other, &c_hw);
            let c_events = fx::take_events();
            assert_eq!(code(r_other.copy_from(&r_hw)), c);
            same_events(c_events);
            let c = c_transfer(&c_dst, &c_other);
            let c_events = fx::take_events();
            assert_eq!(code(r_other.download_to(r_dst.as_mut())), c);
            same_events(c_events);
            assert_eq!(images(&c_dst, &r_dst), (ci.clone(), ci));
        }
        // Back from the copy, in the other direction.
        let c = c_transfer(&c_hw, &c_other);
        let c_events = fx::take_events();
        assert_eq!(code(r_hw.copy_from(&r_other)), c);
        same_events(c_events);
        drop((c_hw, r_hw, c_other, r_other, ctx));
        fx::take_events();
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn backend_failures_and_derived_copies_match_c() {
        let failing = FramesHwctx {
            transfer_ret: -12345,
            ..FramesHwctx::default()
        };
        let ctx = Contexts::new(Kind::A, failing);
        let (c_hw, mut r_hw) = ctx.get_buffer();
        let (c_sw, r_sw) = software(W, H, 1);
        let c = c_transfer(&c_hw, &c_sw);
        let c_events = fx::take_events();
        assert_eq!(c, -12345);
        assert_eq!(code(r_hw.upload_from(r_sw.as_ref())), c);
        same_events(c_events);
        let (c_fresh, mut r_fresh) = (CFrame::alloc(), Frame::new().unwrap());
        let c = c_transfer(&c_fresh, &c_hw);
        let c_events = fx::take_events();
        assert_eq!(code(r_hw.download_to(r_fresh.as_mut())), c);
        same_events(c_events);
        let (cd, rd) = dims(&c_fresh, &r_fresh);
        assert_eq!(cd, rd);
        // A derived context on the same device: C refuses copies between
        // hardware frames when either context is derived, after the listings
        // Rust checks first.
        let ok = Contexts::new(Kind::A, FramesHwctx::default());
        let flags = MapFlags::READ | MapFlags::WRITE;
        let fmt = ffi::AVPixelFormat::AV_PIX_FMT_VAAPI;
        let mut c_derived = ptr::null_mut();
        // SAFETY: live device and frames contexts and a writable slot.
        let ret = unsafe {
            ffi::av_hwframe_ctx_create_derived(
                &raw mut c_derived,
                fmt,
                ok.c_dev,
                ok.c,
                flags.bits(),
            )
        };
        assert_eq!(ret, 0);
        let source = ok.r.as_ref();
        let r_derived = source
            .derive(PixelFormat::VAAPI, ok.r_dev.as_ref(), flags)
            .unwrap();
        fx::take_events();
        let c_hd = CFrame::alloc();
        // SAFETY: a live frames context and a fresh frame.
        let ret = unsafe { ffi::av_hwframe_get_buffer(c_derived, c_hd.0, 0) };
        assert_eq!(ret, 0);
        let mut r_hd = r_derived.as_ref().get_buffer().unwrap();
        let c_events = fx::take_events();
        assert_eq!(c_events.len(), 4);
        assert_eq!(c_events[..2], c_events[2..]);
        let (c_plain, mut r_plain) = ok.get_buffer();
        let c = c_transfer(&c_hd, &c_plain);
        assert_eq!(c, ENOSYS);
        assert_eq!(code(r_hd.copy_from(&r_plain)), c);
        let c = c_transfer(&c_plain, &c_hd);
        assert_eq!(code(r_plain.copy_from(&r_hd)), c);
        assert!(fx::take_events().is_empty());
        // Its mapped frames transfer with software frames.
        let c = c_transfer(&c_hd, &c_sw);
        let c_events = fx::take_events();
        assert_eq!(code(r_hd.upload_from(r_sw.as_ref())), c);
        same_events(c_events);
        let (c_dst, mut r_dst) = software(W, H, 0);
        let c = c_transfer(&c_dst, &c_hd);
        let c_events = fx::take_events();
        assert_eq!(code(r_hd.download_to(r_dst.as_mut())), c);
        same_events(c_events);
        let (ci, ri) = images(&c_dst, &r_dst);
        assert_eq!(ci, ri);
        // SAFETY: the test's own reference.
        unsafe { fx::unref(c_derived) };
        drop((
            c_dst, r_dst, c_hd, r_hd, r_derived, c_plain, r_plain, ok, c_hw, r_hw, ctx,
        ));
        fx::take_events();
    }
}

#[expect(
    dead_code,
    reason = "a helper shared by several test crates; this one uses only some of it"
)]
#[path = "../common/hwdevice.rs"]
mod hwdevice;

/// [`DeviceContext`]'s getters and hooks, and initialisation after filling
/// it, against C's own routines and field reads (hwcontext wave 1, batch 0).
///
/// The campaign builds compile no backend, so both sides use the fake one of
/// `tests/common/hwdevice.rs`, on independent, equivalent contexts: C's
/// `av_hwdevice_ctx_init` on one, [`UninitDevice::init`] on the other.
///
/// [`DeviceContext`]: ffmpeg_bindings::libavutil::libavutil::hwcontext::DeviceContext
/// [`UninitDevice::init`]: ffmpeg_bindings::libavutil::libavutil::hwcontext::UninitDevice::init
mod device_context {
    use core::ffi::{CStr, c_int, c_void};
    use core::ptr::null_mut;
    use std::sync::{Arc, Mutex};

    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::hwcontext::{
        DeviceContextMut, DeviceContextRef, UninitDevice,
    };

    use super::hwdevice::{self, BARE_TYPE, Events, FakeHw};

    /// The C reference: a raw fake context with `value` and `init_ret`
    /// written by hand, counting into `events`.
    fn c_device(events: &Arc<Events>, value: i64, init_ret: c_int) -> *mut ffi::AVBufferRef {
        let raw = hwdevice::fake_device();
        // For the unsafe operations below: the only reference to a fresh fake context; the events
        // outlive it.
        {
            // SAFETY: as the comment above this block states.
            let hw = unsafe { hwdevice::hwctx(raw) };
            // SAFETY: as the comment above this block states.
            unsafe { (*hw).events = Arc::as_ptr(events) };
            // SAFETY: as the comment above this block states.
            unsafe { (*hw).value = value };
            // SAFETY: as the comment above this block states.
            unsafe { (*hw).init_ret = init_ret };
        }
        raw
    }

    /// The Rust side: the same through the wrapper.
    fn rust_device(events: &Arc<Events>, value: i64, init_ret: c_int) -> UninitDevice {
        // SAFETY: the only reference to a fresh, uninitialised fake context.
        let mut b = unsafe { UninitDevice::from_c(hwdevice::fake_device()) }.unwrap();
        // SAFETY: the fake context's hardware context is a `FakeHwctx`; the
        // events outlive it.
        let mut hw = unsafe { b.context_mut().into_hwctx::<FakeHw>() }.unwrap();
        hw.set_events(Arc::as_ptr(events));
        hw.set_value(value);
        hw.set_init_ret(init_ret);
        b
    }

    /// C's view of a context's hardware context: value and initialised flag.
    fn c_state(raw: *mut ffi::AVBufferRef) -> (i64, c_int) {
        // For the unsafe operations below: a live reference to a fake context.
        {
            // SAFETY: as the comment above this block states.
            let hw = unsafe { hwdevice::hwctx(raw) };
            // SAFETY: as the comment above this block states.
            (unsafe { (*hw).value }, unsafe { (*hw).initialised })
        }
    }

    fn rust_state(ctx: DeviceContextRef<'_>) -> (i64, c_int) {
        // SAFETY: the fake context's hardware context is a `FakeHwctx`.
        let hw = unsafe { ctx.hwctx::<FakeHw>() }.unwrap();
        (hw.value(), c_int::from(hw.initialised()))
    }

    fn c_unref(mut raw: *mut ffi::AVBufferRef) {
        // SAFETY: the caller's reference, released once.
        unsafe { ffi::av_buffer_unref(&raw mut raw) };
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn init_results_and_lifecycle_match_c() {
        for init_ret in [0, 1, 7, -1, -22, c_int::MIN] {
            let (ce, re) = (Arc::new(Events::default()), Arc::new(Events::default()));
            let c = c_device(&ce, 5, init_ret);
            let rust = rust_device(&re, 5, init_ret);
            assert_eq!(c_state(c), rust_state(rust.context()));
            // SAFETY: the only reference to an uninitialised fake context.
            let c_ret = unsafe { ffi::av_hwdevice_ctx_init(c) };
            // SAFETY: the fake backend's `device_init` reads nothing it
            // cannot take.
            let rust = unsafe { rust.init() };
            assert_eq!(c_ret, init_ret);
            match &rust {
                Ok(arc) => {
                    assert!(c_ret >= 0);
                    assert_eq!(c_state(c), rust_state(arc.as_ref().context()));
                }
                Err(e) => assert_eq!(*e, c_ret),
            }
            assert_eq!(ce.get().0, re.get().0);
            // A failed C context is the caller's to release; the wrapper
            // released its own.
            if c_ret < 0 {
                assert_eq!(re.get().1, 1);
                c_unref(c);
            } else {
                let arc = rust.unwrap();
                // SAFETY: a live reference; C adds another.
                let c2 = unsafe { ffi::av_buffer_ref(c) };
                let r2 = arc.clone();
                assert!(!c2.is_null());
                c_unref(c);
                drop(arc);
                assert_eq!((ce.get().1, re.get().1), (0, 0));
                c_unref(c2);
                drop(r2);
            }
            assert_eq!(ce.get(), re.get(), "init_ret {init_ret}");
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn getters_read_what_c_stores() {
        for (backend, t) in [
            (
                &hwdevice::FAKE_TYPE,
                ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_VAAPI,
            ),
            (&BARE_TYPE, ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_VULKAN),
            (&BARE_TYPE, ffi::AVHWDeviceType(99)),
        ] {
            let raw = hwdevice::fake_device_of(backend, t);
            // For the unsafe operations below: a live fake context, read before the builder adopts
            // it.
            let (c_type, c_class, c_hwctx) = {
                // SAFETY: as the comment above this block states.
                let ctx = unsafe { hwdevice::context(raw) };
                // SAFETY: as the comment below states, for this part of the operation.
                let type_2 = unsafe { (*ctx).type_ };
                // SAFETY: as the comment below states, for this part of the operation.
                let av_class_2 = unsafe { (*ctx).av_class };
                // SAFETY: as the comment below states, for this part of the operation.
                let class_name_2 = unsafe { (*av_class_2).class_name };
                // SAFETY: as the comment above this block states.
                (type_2, unsafe { CStr::from_ptr(class_name_2) }, unsafe {
                    (*ctx).hwctx
                })
            };
            // SAFETY: the only reference to a fresh, uninitialised context.
            let b = unsafe { UninitDevice::from_c(raw) }.unwrap();
            let ctx = b.context();
            assert_eq!(ctx.device_type().into_raw(), c_type);
            assert_eq!(ctx.class().unwrap().name(), Some(c_class));
            // SAFETY: the fake backend's hardware context type, or none.
            let hw = unsafe { ctx.hwctx::<FakeHw>() };
            assert_eq!(hw.is_some(), !c_hwctx.is_null());
            if let Some(hw) = hw {
                // SAFETY: C's own pointer to the same hardware context.
                assert_eq!(hw.value(), unsafe {
                    (*c_hwctx.cast::<hwdevice::FakeHwctx>()).value
                });
            }
            drop(b);
        }
    }

    /// The C reference's free callback: records the events at its call into
    /// the `Mutex` its `user_opaque` addresses, as a caller's C callback
    /// reads its own data.
    unsafe extern "C" fn c_free(ctx: *mut ffi::AVHWDeviceContext) {
        // For the unsafe operations below: C passes the live fake context; `user_opaque` is the
        // test's slot and `hwctx` is still allocated.
        {
            // SAFETY: as the comment above this block states: a read of `user_opaque`.
            let opaque = unsafe { (*ctx).user_opaque };
            // SAFETY: as the comment above this block states: the test's slot.
            let slot = unsafe { &*opaque.cast::<Mutex<Option<(usize, usize, usize)>>>() };
            // SAFETY: as the comment above this block states: a read of `hwctx`.
            let hwctx = unsafe { (*ctx).hwctx };
            // SAFETY: as the comment above this block states: a read of its events.
            let events = unsafe { (*hwctx.cast::<hwdevice::FakeHwctx>()).events };
            // SAFETY: as the comment above this block states: the test's live events.
            let events = unsafe { &*events };
            *slot.lock().unwrap() = Some(events.get());
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn hooks_run_where_cs_free_callback_runs() {
        let (ce, re) = (Arc::new(Events::default()), Arc::new(Events::default()));
        let c_slot = Mutex::new(None);
        let c = c_device(&ce, 1, 0);
        // For the unsafe operations below: the only reference to a fresh fake context: the caller's
        // callback and its data, which outlives the context.
        {
            // SAFETY: as the comment above this block states.
            let ctx = unsafe { hwdevice::context(c) };
            // SAFETY: as the comment above this block states.
            unsafe { (*ctx).free = Some(c_free) };
            // SAFETY: as the comment above this block states.
            unsafe { (*ctx).user_opaque = (&raw const c_slot).cast_mut().cast::<c_void>() };
        }
        let mut rust = rust_device(&re, 1, 0);
        let rust_slot = Arc::new(Mutex::new(None));
        let (slot, ev) = (rust_slot.clone(), re.clone());
        rust.context_mut()
            .set_free_callback(move |_: DeviceContextMut<'_>| {
                *slot.lock().unwrap() = Some(ev.get());
            })
            .ok()
            .unwrap();
        // SAFETY: as the comment below states, for this part of the operation.
        let context_3 = unsafe { hwdevice::context(c) };
        // SAFETY: as the comment below states, for this part of the operation.
        let free_3 = unsafe { (*context_3).free };
        // SAFETY: as the comment below states, for this part of the operation.
        let context_3 = unsafe { hwdevice::context(c) };
        // Both are visible to C the same way: a callback and its data set.
        // SAFETY: reads of a live context's fields.
        let c_fields = unsafe { (free_3.is_some(), !(*context_3).user_opaque.is_null()) };
        let rust_raw = rust.into_c();
        // SAFETY: as the comment below states, for this part of the operation.
        let context_2 = unsafe { hwdevice::context(rust_raw) };
        // SAFETY: as the comment below states, for this part of the operation.
        let free_2 = unsafe { (*context_2).free };
        // SAFETY: as the comment below states, for this part of the operation.
        let context_2 = unsafe { hwdevice::context(rust_raw) };
        // SAFETY: as above.
        let rust_fields = unsafe { (free_2.is_some(), !(*context_2).user_opaque.is_null()) };
        assert_eq!(c_fields, rust_fields);
        // SAFETY: the reference `into_c` gave up, still uninitialised.
        let rust = unsafe { UninitDevice::from_c(rust_raw) }.unwrap();
        // SAFETY: the only reference to an uninitialised fake context.
        assert_eq!(unsafe { ffi::av_hwdevice_ctx_init(c) }, 0);
        // SAFETY: as in `init_results_and_lifecycle_match_c`.
        let arc = unsafe { rust.init() }.unwrap();
        // C releases both, the Rust one through its `AVBufferRef`.
        c_unref(c);
        c_unref(arc.into_c());
        assert_eq!(*c_slot.lock().unwrap(), Some((1, 1, 0)));
        assert_eq!(*rust_slot.lock().unwrap(), *c_slot.lock().unwrap());
        assert_eq!(ce.get(), re.get());
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn taking_the_hooks_leaves_what_c_starts_with() {
        let events = Arc::new(Events::default());
        let mut rust = rust_device(&events, 0, 0);
        rust.context_mut().set_user_data(1u32).unwrap();
        rust.context_mut().set_free_callback(|_| ()).ok().unwrap();
        let _ = rust.context_mut().take_free_callback();
        let _ = rust.context_mut().take_user_data();
        let raw = rust.into_c();
        // For the unsafe operations below: a fresh C context and this one, both live, read for
        // their hook fields.
        let (c, fresh) = {
            let fresh = hwdevice::fake_device();
            // SAFETY: as the comment above this block states.
            let a = unsafe { hwdevice::context(raw) };
            // SAFETY: as the comment above this block states.
            let f = unsafe { hwdevice::context(fresh) };
            // SAFETY: as the comment above this block states.
            (
                (unsafe { (*a).free }.is_none(), unsafe { (*a).user_opaque }),
                (
                    unsafe { (*f).free }.is_none(),
                    unsafe { (*f).user_opaque },
                    fresh,
                ),
            )
        };
        assert_eq!(c, (fresh.0, fresh.1));
        assert_eq!(c.1, null_mut());
        c_unref(fresh.2);
        // SAFETY: the reference `into_c` gave up.
        drop(unsafe { UninitDevice::from_c(raw) });
    }
}

/// The safe logging-context route to a device context, `ObjectRef::from` its
/// handles, against C reading and logging in a raw context of the same fake
/// backend.
mod device_class_object {
    use core::cell::RefCell;
    use core::ffi::c_void;

    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::hwcontext::UninitDevice;
    use ffmpeg_bindings::libavutil::libavutil::log::{
        self, Callback, ClassCategory, Level, Record, default_category, default_item_name,
    };
    use ffmpeg_bindings::libavutil::libavutil::opt::{ObjectMut, ObjectRef};

    use super::hwdevice;
    use crate::log::LogState;

    thread_local! {
        /// The context and line of each message this thread logged.
        static SEEN: RefCell<Vec<(usize, Vec<u8>)>> = const { RefCell::new(Vec::new()) };
    }

    /// Records each message's context and formatted line.
    struct Seen;

    impl Callback for Seen {
        fn log(record: Record<'_>) {
            let line = record.to_line(&mut true).unwrap_or_default();
            let entry = (record.context() as usize, line.into_bytes());
            SEEN.with(|s| s.borrow_mut().push(entry));
        }
    }

    /// The line logged with the context address spelled `@ ctx`.
    fn normalized((ctx, line): (usize, Vec<u8>)) -> String {
        let line = String::from_utf8(line).unwrap();
        line.replace(&format!("{:p}", ctx as *const c_void), "ctx")
    }

    /// Runs `f` with [`Seen`] installed; returns this thread's lines.
    fn seen(f: impl FnOnce()) -> Vec<String> {
        SEEN.with(|s| s.borrow_mut().clear());
        log::set_callback::<Seen>();
        f();
        log::set_default_callback();
        SEEN.with(|s| s.take())
            .into_iter()
            .map(normalized)
            .collect()
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn device_context_is_the_object_c_logs_in() {
        let _state = LogState::lock();
        let mut buf = hwdevice::fake_device();
        // SAFETY: a live reference to a fake context.
        let raw = unsafe { hwdevice::context(buf) }.cast::<c_void>();
        // SAFETY: the only reference to a fresh, uninitialised fake context.
        let mut dev = unsafe { UninitDevice::from_c(hwdevice::fake_device()) }.unwrap();

        let obj = ObjectRef::from(dev.context());
        assert!(obj.class().ptr_eq(dev.context().class().unwrap()));
        // SAFETY: `raw` is a live context led by its class, which C only reads.
        let (c_name, c_category) = (unsafe { ffi::av_default_item_name(raw) }, unsafe {
            ffi::av_default_get_category(raw)
        });
        assert_eq!(default_item_name(obj).unwrap().as_ptr(), c_name);
        assert_eq!(default_category(obj), ClassCategory::from(c_category));
        assert_eq!(default_category(obj), ClassCategory::HWDEVICE);

        let c = seen(|| {
            // SAFETY: as above; `"%s"` consumes the one string passed.
            unsafe {
                ffi::av_log(
                    raw,
                    Level::ERROR.raw(),
                    c"%s".as_ptr(),
                    c"in device\n".as_ptr(),
                );
            }
        });
        let rust = seen(|| log::log_with_context(obj, Level::ERROR, c"in device\n"));
        assert_eq!(rust, c);
        // The default callback names the context by its backend.
        assert!(rust[0].contains("@ ctx]"), "{rust:?}");

        // The exclusive handle is the same object.
        let obj = ObjectMut::from(dev.context_mut());
        let rust = seen(|| log::log_with_context(obj.as_ref(), Level::ERROR, c"in device\n"));
        assert_eq!(rust, c);

        drop(dev);
        // SAFETY: the C side's only reference.
        unsafe { ffi::av_buffer_unref(&raw mut buf) };
    }
}

/// [`FramesConstraints`] from C's `av_hwdevice_get_hwframe_constraints`
/// against the wrapper's, on independent equivalent fake devices, then
/// edited in step by C-style writes and the setters, and released by
/// `av_hwframe_constraints_free` and the box (hwcontext wave 1, batch 0).
///
/// [`FramesConstraints`]: ffmpeg_bindings::libavutil::libavutil::hwcontext::FramesConstraints
mod frames_constraints {
    use core::ffi::c_int;
    use core::ptr::{null, null_mut};
    use core::sync::atomic::Ordering;
    use std::sync::Arc;

    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::hwcontext::{
        DeviceReferenceBox, FramesConstraints, FramesConstraintsBox, UninitDevice,
    };
    use ffmpeg_bindings::libavutil::libavutil::pixfmt::PixelFormat;

    use super::hwdevice::{self, BARE_TYPE, Backend, Events, FakeConfig, FakeHw};

    /// One query: the backend, what it lists and returns, and a
    /// configuration's hardware format.
    #[derive(Clone, Copy, Debug)]
    struct Query {
        bare: bool,
        nb_sw: c_int,
        ret: c_int,
        config: Option<ffi::AVPixelFormat>,
    }

    fn queries() -> Vec<Query> {
        let mut q = vec![];
        for bare in [false, true] {
            for nb_sw in [-1, 0, 1, 4] {
                for ret in [0, 3, -12] {
                    for config in [None, Some(ffi::AVPixelFormat::AV_PIX_FMT_CUDA)] {
                        q.push(Query {
                            bare,
                            nb_sw,
                            ret,
                            config,
                        });
                    }
                }
            }
        }
        q
    }

    fn backend(q: Query) -> &'static Backend {
        if q.bare {
            &BARE_TYPE
        } else {
            &hwdevice::FAKE_TYPE
        }
    }

    /// A raw fake device set up for `q` and initialised by C.
    fn c_device(q: Query, events: &Arc<Events>) -> *mut ffi::AVBufferRef {
        let raw = hwdevice::fake_device_of(backend(q), ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_VAAPI);
        // For the unsafe operations below: the only reference to a fresh fake context.
        {
            // SAFETY: as the comment above this block states.
            let hw = unsafe { hwdevice::hwctx(raw) };
            if !hw.is_null() {
                // SAFETY: as the comment above this block states.
                unsafe { (*hw).events = Arc::as_ptr(events) };
                // SAFETY: as the comment above this block states.
                unsafe { (*hw).nb_sw = q.nb_sw };
                // SAFETY: as the comment above this block states.
                unsafe { (*hw).constraints_ret = q.ret };
            }
            // SAFETY: as the comment above this block states.
            unsafe { assert_eq!(ffi::av_hwdevice_ctx_init(raw), 0) };
        }
        raw
    }

    /// The same device through the wrapper.
    fn rust_device(q: Query, events: &Arc<Events>) -> DeviceReferenceBox {
        let raw = hwdevice::fake_device_of(backend(q), ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_VAAPI);
        // SAFETY: the only reference to a fresh, uninitialised fake context.
        let mut b = unsafe { UninitDevice::from_c(raw) }.unwrap();
        // SAFETY: the fake backend's hardware context type, or none.
        if let Some(mut hw) = unsafe { b.context_mut().into_hwctx::<FakeHw>() } {
            hw.set_events(Arc::as_ptr(events));
            hw.set_nb_sw(q.nb_sw);
            hw.set_constraints_ret(q.ret);
        }
        // SAFETY: the fake backend's `device_init` reads nothing it cannot
        // take.
        unsafe { b.init() }.unwrap()
    }

    /// A NONE-terminated C list, read up to the terminator.
    fn c_list(list: *const ffi::AVPixelFormat) -> Option<Vec<ffi::AVPixelFormat>> {
        if list.is_null() {
            return None;
        }
        let mut v = vec![];
        loop {
            // SAFETY: a live NONE-terminated list, read up to its terminator.
            let at = unsafe { list.add(v.len()) };
            // SAFETY: as above.
            let format = unsafe { at.read() };
            if format == ffi::AVPixelFormat::AV_PIX_FMT_NONE {
                break;
            }
            v.push(format);
        }
        Some(v)
    }

    /// Everything C observes of a constraints struct.
    type View = (
        Option<Vec<ffi::AVPixelFormat>>,
        Option<Vec<ffi::AVPixelFormat>>,
        [c_int; 4],
    );

    fn c_view(c: *const ffi::AVHWFramesConstraints) -> View {
        // SAFETY: a live constraints struct.
        (
            c_list(unsafe { (*c).valid_hw_formats }),
            c_list(unsafe { (*c).valid_sw_formats }),
            [
                unsafe { (*c).min_width },
                unsafe { (*c).min_height },
                unsafe { (*c).max_width },
                unsafe { (*c).max_height },
            ],
        )
    }

    fn rust_view(c: &FramesConstraintsBox) -> View {
        let r = c.as_ref();
        let list = |l: Option<ffibox::CSlice<'_, PixelFormat>>| {
            l.map(|l| l.elems().map(PixelFormat::into_raw).collect())
        };
        (
            list(r.valid_hw_formats()),
            list(r.valid_sw_formats()),
            [r.min_width(), r.min_height(), r.max_width(), r.max_height()],
        )
    }

    fn c_free(mut c: *mut ffi::AVHWFramesConstraints) {
        // SAFETY: the caller's constraints, released once.
        unsafe { ffi::av_hwframe_constraints_free(&raw mut c) };
        assert!(c.is_null());
    }

    fn c_unref(mut raw: *mut ffi::AVBufferRef) {
        // SAFETY: the caller's reference, released once.
        unsafe { ffi::av_buffer_unref(&raw mut raw) };
    }

    /// One edit, applied to both sides.
    #[derive(Clone, Debug)]
    enum Edit {
        Sizes([c_int; 4]),
        Hw(Option<Vec<PixelFormat>>),
        Sw(Option<Vec<PixelFormat>>),
        TakeHw,
        TakeSw,
    }

    fn edits() -> Vec<Edit> {
        use Edit::*;
        let some = |v: &[PixelFormat]| Some(v.to_vec());
        vec![
            Sizes([16, 16, 1920, 1080]),
            Hw(some(&[PixelFormat::VAAPI, PixelFormat::DRM_PRIME])),
            Sw(some(&[])),
            Sw(some(&[
                PixelFormat::NV12,
                PixelFormat::NONE,
                PixelFormat::P010LE,
            ])),
            TakeHw,
            TakeHw,
            Hw(None),
            Sw(None),
            TakeSw,
            Sw(some(&[PixelFormat::YUV420P; 40])),
            Sizes([c_int::MIN, -1, 0, c_int::MAX]),
            Hw(some(&[PixelFormat::NONE])),
            TakeSw,
        ]
    }

    /// C's edit: hand-written list blocks, as a backend fills them, and
    /// `av_free` of the old ones; a taken list is read, then freed.
    fn c_edit(c: *mut ffi::AVHWFramesConstraints, e: &Edit) -> Option<Vec<ffi::AVPixelFormat>> {
        let replace = |slot: *mut *mut ffi::AVPixelFormat, with: Option<&Vec<PixelFormat>>| {
            let new = match with {
                None => null_mut(),
                Some(v) => {
                    let stored: Vec<_> = v
                        .iter()
                        .copied()
                        .take_while(|&f| f != PixelFormat::NONE)
                        .collect();
                    let n = stored.len() + 1;
                    // SAFETY: a fresh block of `n` formats.
                    let list = unsafe { ffi::av_malloc_array(n, size_of::<ffi::AVPixelFormat>()) }
                        .cast::<ffi::AVPixelFormat>();
                    assert!(!list.is_null());
                    let all = stored
                        .iter()
                        .map(|f| f.into_raw())
                        .chain([ffi::AVPixelFormat::AV_PIX_FMT_NONE]);
                    for (i, f) in all.enumerate() {
                        // SAFETY: as the comment below states, for this part of the operation.
                        let list_add = unsafe { list.add(i) };
                        // SAFETY: `i < n`.
                        unsafe { list_add.write(f) };
                    }
                    list
                }
            };
            // SAFETY: as the comment below states, for this part of the operation.
            let slot_replace = unsafe { slot.replace(new) };
            // SAFETY: the struct's own list slot; the old list is its av_malloc
            // block or NULL.
            unsafe { ffi::av_free(slot_replace.cast()) };
        };
        let take = |slot: *mut *mut ffi::AVPixelFormat| {
            // SAFETY: as above.
            let old = unsafe { slot.replace(null_mut()) };
            let v = c_list(old);
            // SAFETY: the list just detached.
            unsafe { ffi::av_free(old.cast()) };
            v
        };
        // SAFETY: field projections of the live struct.
        let (hw, sw) = (unsafe { &raw mut (*c).valid_hw_formats }, unsafe {
            &raw mut (*c).valid_sw_formats
        });
        match e {
            Edit::Sizes(s) => {
                // For the unsafe operations below: writes of the live struct's sizes.
                {
                    // SAFETY: as the comment above this block states.
                    unsafe { (*c).min_width = s[0] };
                    // SAFETY: as the comment above this block states.
                    unsafe { (*c).min_height = s[1] };
                    // SAFETY: as the comment above this block states.
                    unsafe { (*c).max_width = s[2] };
                    // SAFETY: as the comment above this block states.
                    unsafe { (*c).max_height = s[3] };
                }
                None
            }
            Edit::Hw(v) => {
                replace(hw, v.as_ref());
                None
            }
            Edit::Sw(v) => {
                replace(sw, v.as_ref());
                None
            }
            Edit::TakeHw => take(hw),
            Edit::TakeSw => take(sw),
        }
    }

    fn rust_edit(c: &mut FramesConstraintsBox, e: &Edit) -> Option<Vec<ffi::AVPixelFormat>> {
        let mut m = c.as_mut();
        let raw = |v: Option<ffmpeg_bindings::libavutil::libavutil::mem::HeapVec<PixelFormat>>| {
            v.map(|v| v.as_slice().iter().map(|f| f.into_raw()).collect())
        };
        match e {
            Edit::Sizes(s) => {
                m.set_min_width(s[0]);
                m.set_min_height(s[1]);
                m.set_max_width(s[2]);
                m.set_max_height(s[3]);
                None
            }
            Edit::Hw(v) => {
                m.set_valid_hw_formats(v.as_deref()).unwrap();
                None
            }
            Edit::Sw(v) => {
                m.set_valid_sw_formats(v.as_deref()).unwrap();
                None
            }
            Edit::TakeHw => raw(m.take_valid_hw_formats()),
            Edit::TakeSw => raw(m.take_valid_sw_formats()),
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn device_queries_and_edits_match_c() {
        for q in queries() {
            let (ce, re) = (Arc::new(Events::default()), Arc::new(Events::default()));
            let c_dev = c_device(q, &ce);
            let rust_dev = rust_device(q, &re);
            // C's configuration from its own allocator, and the wrapper's,
            // filled alike.
            let c_config = q.config.map(|hw_format| {
                // SAFETY: an initialised device; C returns a zeroed block of
                // the backend's configuration size, or NULL.
                let p = unsafe { ffi::av_hwdevice_hwconfig_alloc(c_dev) }.cast::<FakeConfig>();
                assert!(!p.is_null());
                // SAFETY: the fake backend's configuration, which the test owns.
                unsafe { (*p).hw_format = hw_format };
                p
            });
            let rust_config = q.config.map(|hw_format| {
                let mut config = rust_dev.as_ref().alloc_hw_config().unwrap();
                // SAFETY: as above, owned by `config`.
                unsafe { (*config.as_mut_ptr().cast::<FakeConfig>()).hw_format = hw_format };
                config
            });
            let config_ptr = c_config.map_or(null(), |p| p.cast_const().cast());
            // SAFETY: an initialised device and NULL or the fake backend's
            // configuration, live for the call.
            let c = unsafe { ffi::av_hwdevice_get_hwframe_constraints(c_dev, config_ptr) };
            if let Some(p) = c_config {
                // SAFETY: the av_mallocz block C allocated above, no longer used.
                unsafe { ffi::av_free(p.cast()) };
            }
            let rust = match &rust_config {
                None => rust_dev.as_ref().frames_constraints(),
                Some(config) => rust_dev.as_ref().frames_constraints_for_config(config),
            };
            assert_eq!(rust.is_some(), !c.is_null(), "{q:?}");
            assert_eq!(ce.get(), re.get(), "{q:?}");
            // For the unsafe operations below: live fake hardware contexts, when the backend has
            // one.
            let seen = |raw: *mut ffi::AVBufferRef| {
                // SAFETY: as the comment above this block states.
                let hw = unsafe { hwdevice::hwctx(raw) };
                // SAFETY: as the comment above this block states.
                unsafe {
                    (!hw.is_null()).then(|| {
                        (*hw)
                            .seen_sizes
                            .each_ref()
                            .map(|a| a.load(Ordering::SeqCst))
                    })
                }
            };
            // SAFETY: the fake backend's hardware context type, or none.
            let rust_hw = unsafe { rust_dev.as_ref().context().hwctx::<FakeHw>() };
            assert_eq!(seen(c_dev), rust_hw.map(|h| h.seen_sizes()));
            if let Some(mut rust) = rust {
                assert_eq!(rust_view(&rust), c_view(c), "{q:?}");
                for e in edits() {
                    assert_eq!(rust_edit(&mut rust, &e), c_edit(c, &e), "{q:?} {e:?}");
                    assert_eq!(rust_view(&rust), c_view(c), "{q:?} {e:?}");
                }
                c_free(c);
                drop(rust);
            }
            c_unref(c_dev);
            drop(rust_dev);
            assert_eq!(ce.get(), re.get(), "{q:?}");
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn new_constraints_are_cs_starting_point() {
        let q = Query {
            bare: false,
            nb_sw: 0,
            ret: 0,
            config: None,
        };
        let events = Arc::new(Events::default());
        let dev = c_device(q, &events);
        // SAFETY: an initialised device; NULL asks for its capabilities.
        let c = unsafe { ffi::av_hwdevice_get_hwframe_constraints(dev, null()) };
        assert!(!c.is_null());
        c_free(c);
        // SAFETY: as the comment below states, for this part of the operation.
        let hwctx_4 = unsafe { hwdevice::hwctx(dev) };
        // SAFETY: the live fake hardware context.
        let seen = unsafe { &(*hwctx_4).seen_sizes }
            .each_ref()
            .map(|a| a.load(Ordering::SeqCst));
        c_unref(dev);
        let new = FramesConstraints::new().unwrap();
        assert_eq!(rust_view(&new), (None, None, seen));
        // Both release a never-filled structure the same way.
        let mut c = new.into_c();
        // SAFETY: the structure `into_c` gave up, which C frees.
        unsafe { ffi::av_hwframe_constraints_free(&raw mut c) };
        assert!(c.is_null());
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn edited_constraints_are_freed_by_c() {
        let mut rust = FramesConstraints::new().unwrap();
        for e in edits() {
            rust_edit(&mut rust, &e);
        }
        rust.as_mut()
            .set_valid_hw_formats(Some(&[PixelFormat::CUDA]))
            .unwrap();
        let mut c = rust.into_c();
        assert_eq!(c_view(c).0, Some(vec![ffi::AVPixelFormat::AV_PIX_FMT_CUDA]));
        // SAFETY: the structure `into_c` gave up, with lists the setters
        // allocated, which C frees.
        unsafe { ffi::av_hwframe_constraints_free(&raw mut c) };
    }

    /// One step of a query sequence on one device.
    #[derive(Clone, Copy, Debug)]
    enum Step {
        /// A query without a configuration.
        Plain,
        /// A query under the configuration, its hardware format first
        /// rewritten to the one given, or left as it is.
        Config(Option<ffi::AVPixelFormat>),
        /// The backend's status for later queries.
        Status(c_int),
    }

    fn steps() -> Vec<Step> {
        use Step::*;
        use ffi::AVPixelFormat as F;
        vec![
            Plain,
            Config(None),
            Config(Some(F::AV_PIX_FMT_CUDA)),
            Plain,
            Status(-12),
            Plain,
            Config(Some(F::AV_PIX_FMT_VULKAN)),
            Status(5),
            Config(Some(F::AV_PIX_FMT_NONE)),
            Config(None),
            Status(0),
            Plain,
        ]
    }

    /// Sets the status the fake backend behind `raw` returns from its
    /// constraints callback.
    fn set_status(raw: *mut ffi::AVBufferRef, ret: c_int) {
        // SAFETY: as the comment below states, for this part of the operation.
        let hwctx_3 = unsafe { hwdevice::hwctx(raw) };
        // SAFETY: a live fake device, used on this thread alone.
        unsafe { (*hwctx_3).constraints_ret = ret };
    }

    /// Repeated queries of one device, with no configuration and under one
    /// configuration rewritten between them, failing ones among them; every
    /// result kept while later ones are made, the first edited apart from
    /// the rest, the configuration and the device released before the
    /// results, and those released in another order than made. Views, the
    /// backend's events and the sizes it found are compared after each step.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn query_sequences_on_one_device_match_c() {
        for nb_sw in [-1, 0, 4] {
            let q = Query {
                bare: false,
                nb_sw,
                ret: 0,
                config: None,
            };
            let (ce, re) = (Arc::new(Events::default()), Arc::new(Events::default()));
            let c_dev = c_device(q, &ce);
            let mut rust_dev = rust_device(q, &re);
            // SAFETY: an initialised fake device, whose backend has a
            // configuration: C returns a zeroed block of its size.
            let c_config = unsafe { ffi::av_hwdevice_hwconfig_alloc(c_dev) }.cast::<FakeConfig>();
            assert!(!c_config.is_null());
            let mut rust_config = rust_dev.as_ref().alloc_hw_config().unwrap();
            let (mut c_results, mut rust_results) = (vec![], vec![]);
            for (i, step) in steps().into_iter().enumerate() {
                let (c, rust) = match step {
                    Step::Status(ret) => {
                        set_status(c_dev, ret);
                        // The wrapper's device goes to C and back to set it.
                        let raw = rust_dev.into_c();
                        set_status(raw, ret);
                        // SAFETY: the reference `into_c` gave up, to an
                        // initialised context.
                        rust_dev = unsafe { DeviceReferenceBox::from_c(raw) }.unwrap();
                        continue;
                    }
                    Step::Plain => (
                        // SAFETY: an initialised device; NULL asks for its
                        // capabilities.
                        unsafe { ffi::av_hwdevice_get_hwframe_constraints(c_dev, null()) },
                        rust_dev.as_ref().frames_constraints(),
                    ),
                    Step::Config(format) => {
                        if let Some(f) = format {
                            // For the unsafe operations below: the fake backend's configurations,
                            // which the test owns and no query runs on.
                            {
                                // SAFETY: as the comment above this block states.
                                unsafe { (*c_config).hw_format = f };
                                // SAFETY: as the comment above this block states.
                                unsafe {
                                    (*rust_config.as_mut_ptr().cast::<FakeConfig>()).hw_format = f;
                                };
                            }
                        }
                        (
                            // SAFETY: an initialised device and its backend's
                            // configuration, live for the call.
                            unsafe {
                                ffi::av_hwdevice_get_hwframe_constraints(
                                    c_dev,
                                    c_config.cast_const().cast(),
                                )
                            },
                            rust_dev
                                .as_ref()
                                .frames_constraints_for_config(&rust_config),
                        )
                    }
                };
                assert_eq!(rust.is_some(), !c.is_null(), "{nb_sw} {i} {step:?}");
                assert_eq!(ce.get(), re.get(), "{nb_sw} {i} {step:?}");
                // SAFETY: as the comment below states, for this part of the operation.
                let hwctx_2 = unsafe { hwdevice::hwctx(c_dev) };
                // SAFETY: the live fake hardware context.
                let c_seen = unsafe { &(*hwctx_2).seen_sizes }
                    .each_ref()
                    .map(|a| a.load(Ordering::SeqCst));
                // SAFETY: the fake backend's hardware context type, whose
                // sizes are atomics.
                let rust_hw = unsafe { rust_dev.as_ref().context().hwctx::<FakeHw>() };
                assert_eq!(Some(c_seen), rust_hw.map(|h| h.seen_sizes()));
                if let Some(rust) = rust {
                    c_results.push(c);
                    rust_results.push(rust);
                }
                for (c, rust) in c_results.iter().zip(&rust_results) {
                    assert_eq!(rust_view(rust), c_view(*c), "{nb_sw} {i} {step:?}");
                }
            }
            assert_eq!(ce.get(), (1, 0, 9));
            assert_eq!(c_results.len(), 7);
            // Edits of the first result leave the others as they were.
            for e in edits() {
                assert_eq!(
                    rust_edit(&mut rust_results[0], &e),
                    c_edit(c_results[0], &e)
                );
                for (c, rust) in c_results.iter().zip(&rust_results) {
                    assert_eq!(rust_view(rust), c_view(*c), "{nb_sw} {e:?}");
                }
            }
            // The configuration, then the device, go before the results.
            // SAFETY: the block av_hwdevice_hwconfig_alloc returned, unused
            // now.
            unsafe { ffi::av_free(c_config.cast()) };
            drop(rust_config);
            c_unref(c_dev);
            drop(rust_dev);
            assert_eq!(ce.get(), re.get());
            assert_eq!(ce.get(), (1, 1, 9));
            while !c_results.is_empty() {
                let at = c_results.len() / 2;
                c_free(c_results.remove(at));
                drop(rust_results.remove(at));
                for (c, rust) in c_results.iter().zip(&rust_results) {
                    assert_eq!(rust_view(rust), c_view(*c), "{nb_sw}");
                }
            }
        }
    }
}

/// [`FramesContext`]'s getters, setters and hooks, and initialisation and
/// release after filling it, against C's own routines and field reads on
/// independent, equivalent contexts of the test backends of
/// `tests/common/hwbackend.rs` (hwcontext wave 2, batch 0): C fills its
/// context by hand, with a `free` callback and a caller pool as C code sets
/// them (NVDEC's, which uninitialises the pool from `free`); Rust through
/// [`FramesContextMut`], with its own hooks and an owned pool. Results,
/// field reads, pool allocations and the backends' event logs are compared
/// after each step.
///
/// [`FramesContext`]: ffmpeg_bindings::libavutil::libavutil::hwcontext::FramesContext
/// [`FramesContextMut`]: ffmpeg_bindings::libavutil::libavutil::hwcontext::FramesContextMut
mod frames_context {
    use core::cell::RefCell;
    use core::ffi::{c_int, c_void};
    use core::ptr;
    use core::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    use ffmpeg_bindings::libavutil::libavutil::buffer::{
        BufferPool, BufferPoolBox, BufferReference,
    };
    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::hwcontext::{
        DeviceReferenceBox, FramesContextMut, FramesContextRef, HardwareFrame, UninitFrames,
    };
    use ffmpeg_bindings::libavutil::libavutil::pixfmt::PixelFormat;

    use super::backend::{self as fx, Event, FramesHw, FramesHwctx, Kind};

    /// The public fields both sides compare: formats, size, pool size,
    /// whether a pool, a `free` and a frames struct are set.
    type Fields = (c_int, c_int, c_int, c_int, c_int, bool, bool, bool);

    fn c_fields(buf: *const ffi::AVBufferRef) -> Fields {
        // SAFETY: as the comment below states, for this part of the operation.
        let frames_ctx_4 = unsafe { fx::frames_ctx(buf) };
        // SAFETY: callers pass live frames context references.
        let c = unsafe { *frames_ctx_4 };
        (
            c.format.0,
            c.sw_format.0,
            c.width,
            c.height,
            c.initial_pool_size,
            !c.pool.is_null(),
            c.free.is_some(),
            !c.hwctx.is_null(),
        )
    }

    fn rust_fields(ctx: FramesContextRef<'_>) -> Fields {
        // SAFETY: the test backends' frames struct is a `FramesHwctx`.
        let hw = unsafe { ctx.hwctx::<FramesHw>() };
        (
            ctx.format().into_raw().0,
            ctx.sw_format().into_raw().0,
            ctx.width(),
            ctx.height(),
            ctx.initial_pool_size(),
            ctx.has_pool(),
            ctx.has_free_callback(),
            hw.is_some(),
        )
    }

    /// One way of filling a context: the formats, size, pool size and
    /// backend results.
    #[derive(Clone, Copy, Debug)]
    struct Fill {
        format: ffi::AVPixelFormat,
        sw_format: ffi::AVPixelFormat,
        width: c_int,
        height: c_int,
        pool: c_int,
        hwctx: FramesHwctx,
    }

    impl Fill {
        fn ok(kind: Kind, pool: c_int) -> Self {
            Self {
                format: kind.format(),
                sw_format: ffi::AVPixelFormat::AV_PIX_FMT_NV12,
                width: 64,
                height: 32,
                pool,
                hwctx: FramesHwctx::default(),
            }
        }

        fn c(self, buf: *mut ffi::AVBufferRef) {
            // For the unsafe operations below: callers pass the only reference to an uninitialised
            // test frames context.
            {
                // SAFETY: as the comment above this block states.
                let c = unsafe { fx::frames_ctx(buf) };
                // SAFETY: as the comment above this block states.
                unsafe { (*c).format = self.format };
                // SAFETY: as the comment above this block states.
                unsafe { (*c).sw_format = self.sw_format };
                // SAFETY: as the comment above this block states.
                unsafe { (*c).width = self.width };
                // SAFETY: as the comment above this block states.
                unsafe { (*c).height = self.height };
                // SAFETY: as the comment above this block states.
                unsafe { (*c).initial_pool_size = self.pool };
                // SAFETY: as the comment above this block states.
                unsafe { fx::set_frames_hwctx(c, self.hwctx) };
            }
        }

        fn rust(self, mut ctx: FramesContextMut<'_>) {
            ctx.set_format(PixelFormat::from_raw(self.format));
            ctx.set_sw_format(PixelFormat::from_raw(self.sw_format));
            ctx.set_width(self.width);
            ctx.set_height(self.height);
            ctx.set_initial_pool_size(self.pool);
            // SAFETY: the test backends' frames struct is a `FramesHwctx`, any
            // of whose results they take.
            unsafe { ctx.into_hwctx::<FramesHw>() }
                .unwrap()
                .set(self.hwctx);
        }
    }

    /// A C-side and a Rust-side test device of `kind`, initialised.
    fn devices(kind: Kind) -> (*mut ffi::AVBufferRef, DeviceReferenceBox) {
        let c = fx::device(kind);
        // SAFETY: the only reference to an initialised test device context,
        // built as `av_hwdevice_ctx_alloc` builds one.
        let r = unsafe { DeviceReferenceBox::from_c(fx::device(kind)) }.unwrap();
        (c, r)
    }

    /// Fresh frames contexts on both sides, compared as allocated.
    fn alloc(
        c_dev: *mut ffi::AVBufferRef,
        r_dev: &DeviceReferenceBox,
    ) -> (*mut ffi::AVBufferRef, UninitFrames) {
        // SAFETY: a live device reference.
        let c = unsafe { ffi::av_hwframe_ctx_alloc(c_dev) };
        assert!(!c.is_null());
        let r = r_dev.as_ref().alloc_frames().unwrap();
        assert_eq!(c_fields(c), rust_fields(r.context()));
        (c, r)
    }

    /// C's `av_hwframe_ctx_init`, releasing a context that failed as the
    /// wrapper does; the reference on success.
    fn c_init(c: *mut ffi::AVBufferRef) -> Result<*mut ffi::AVBufferRef, c_int> {
        // SAFETY: the only reference to a filled, uninitialised context.
        let ret = unsafe { ffi::av_hwframe_ctx_init(c) };
        if ret < 0 {
            // SAFETY: the reference `c` holds.
            unsafe { fx::unref(c) };
            Err(ret)
        } else {
            Ok(c)
        }
    }

    /// Both sides' events since the last call, equal.
    fn same_events() -> Vec<Event> {
        let events = fx::take_events();
        let half = events.len() / 2;
        assert_eq!(events.len() % 2, 0, "{events:?}");
        let (a, b) = events.split_at(half);
        assert_eq!(a, b);
        a.to_vec()
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn fields_and_init_results_match_c_after_each_step() {
        let kind = Kind::A;
        let other = Fill {
            format: Kind::B.format(),
            ..Fill::ok(kind, 0)
        };
        let fills = [
            Fill::ok(kind, 0),
            Fill::ok(kind, 3),
            Fill::ok(kind, -7),
            Fill {
                width: 1,
                height: 1,
                ..Fill::ok(kind, 1)
            },
            Fill {
                format: ffi::AVPixelFormat::AV_PIX_FMT_NONE,
                ..Fill::ok(kind, 0)
            },
            other,
            Fill {
                width: 0,
                ..Fill::ok(kind, 0)
            },
            Fill {
                width: c_int::MAX,
                height: c_int::MAX,
                ..Fill::ok(kind, 0)
            },
            Fill {
                height: -1,
                ..Fill::ok(kind, 0)
            },
            Fill {
                sw_format: ffi::AVPixelFormat(9999),
                ..Fill::ok(kind, 0)
            },
            Fill {
                hwctx: FramesHwctx {
                    init_ret: -5,
                    ..FramesHwctx::default()
                },
                ..Fill::ok(kind, 2)
            },
            Fill {
                hwctx: FramesHwctx {
                    get_buffer_ret: -12,
                    ..FramesHwctx::default()
                },
                ..Fill::ok(kind, 2)
            },
            Fill {
                hwctx: FramesHwctx {
                    no_pool: 1,
                    ..FramesHwctx::default()
                },
                ..Fill::ok(kind, 0)
            },
        ];
        let (c_dev, r_dev) = devices(kind);
        for (i, fill) in fills.iter().enumerate() {
            let (c, mut r) = alloc(c_dev, &r_dev);
            // Every fill over another first, to compare overwrites too.
            let previous = fills[(i + 1) % fills.len()];
            previous.c(c);
            previous.rust(r.context_mut());
            assert_eq!(c_fields(c), rust_fields(r.context()));
            fill.c(c);
            fill.rust(r.context_mut());
            assert_eq!(c_fields(c), rust_fields(r.context()), "{fill:?}");
            let (c, r) = (c_init(c), r.init());
            assert_eq!(c.err(), r.as_ref().err().copied(), "{fill:?}");
            same_events();
            if let (Ok(c), Ok(r)) = (c, r) {
                let ctx = r.as_ref().context();
                assert_eq!(c_fields(c), rust_fields(ctx), "{fill:?}");
                // SAFETY: both test backends' frames structs.
                let rust_hw = unsafe { ctx.hwctx::<FramesHw>() }.unwrap().get();
                // SAFETY: as the comment below states, for this part of the operation.
                let frames_ctx_2 = unsafe { fx::frames_ctx(c) };
                // SAFETY: as the comment below states, for this part of the operation.
                let hwctx_2 = unsafe { (*frames_ctx_2).hwctx };
                // SAFETY: as above, read by hand.
                let c_hw = unsafe { hwctx_2.cast::<FramesHwctx>().read() };
                assert_eq!(format!("{c_hw:?}"), format!("{rust_hw:?}"));
                // For the unsafe operations below: a live frames context and a fresh frame.
                let cf = {
                    // SAFETY: as the comment above this block states.
                    let f = unsafe { ffi::av_frame_alloc() };
                    // SAFETY: as the comment above this block states.
                    let ret = unsafe { ffi::av_hwframe_get_buffer(c, f, 0) };
                    (f, ret)
                };
                let rf = r.as_ref().get_buffer();
                assert_eq!(cf.1, rf.as_ref().err().copied().unwrap_or(0));
                same_events();
                if let Ok(rf) = &rf {
                    // SAFETY: the frame C filled.
                    let c_ctx = unsafe { (*cf.0).hw_frames_ctx };
                    assert_eq!(c_fields(c_ctx), rust_fields(rf.frames().context()));
                }
                // For the unsafe operations below: the test's frame and reference.
                {
                    // SAFETY: as the comment above this block states.
                    unsafe { ffi::av_frame_free(&mut { cf.0 }) };
                    // SAFETY: as the comment above this block states.
                    unsafe { fx::unref(c) };
                }
                drop((rf, r));
                same_events();
            }
        }
        // SAFETY: the test's device reference.
        unsafe { fx::unref(c_dev) };
        drop(r_dev);
        same_events();
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn create_frames_matches_c() {
        let kind = Kind::A;
        let (c_dev, r_dev) = devices(kind);
        for fill in [
            Fill::ok(kind, 0),
            Fill::ok(kind, 5),
            Fill::ok(kind, i32::MIN),
            Fill {
                format: ffi::AVPixelFormat::AV_PIX_FMT_NONE,
                ..Fill::ok(kind, 0)
            },
            Fill {
                width: 1 << 20,
                height: 1 << 20,
                ..Fill::ok(kind, 0)
            },
            Fill {
                width: 7,
                height: 3,
                sw_format: ffi::AVPixelFormat::AV_PIX_FMT_YUV420P,
                ..Fill::ok(kind, 1)
            },
        ] {
            // SAFETY: a live device reference.
            let c = unsafe { ffi::av_hwframe_ctx_alloc(c_dev) };
            assert!(!c.is_null());
            fill.c(c);
            let c = c_init(c);
            let r = r_dev.as_ref().create_frames(
                PixelFormat::from_raw(fill.format),
                PixelFormat::from_raw(fill.sw_format),
                fill.width,
                fill.height,
                fill.pool,
            );
            assert_eq!(c.err(), r.as_ref().err().copied(), "{fill:?}");
            same_events();
            if let (Ok(c), Ok(r)) = (c, r) {
                assert_eq!(c_fields(c), rust_fields(r.as_ref().context()), "{fill:?}");
                // SAFETY: the test's reference.
                unsafe { fx::unref(c) };
                drop(r);
                same_events();
            }
        }
        // SAFETY: the test's device reference.
        unsafe { fx::unref(c_dev) };
        drop(r_dev);
        same_events();
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn the_fixed_fields_read_what_c_stores() {
        for kind in [Kind::A, Kind::B] {
            let (c_dev, r_dev) = devices(kind);
            let (c, r) = alloc(c_dev, &r_dev);
            let ctx = r.context();
            // SAFETY: as the comment below states, for this part of the operation.
            let frames_ctx_3 = unsafe { fx::frames_ctx(c) };
            // SAFETY: a live frames context's public struct.
            let cc = unsafe { *frames_ctx_3 };
            assert!(!cc.device_ref.is_null(), "C's allocator stores the device");
            // SAFETY: both devices are live; C reads the device type.
            let c_type = unsafe { (*cc.device_ctx).type_ };
            assert_eq!(ctx.device_context().device_type().into_raw(), c_type);
            assert_eq!(ctx.device().context().device_type().into_raw(), c_type);
            assert!(ctx.device().ptr_eq(r_dev.as_ref()));
            // SAFETY: as the comment below states, for this part of the operation.
            let class_name_2 = unsafe { (*cc.av_class).class_name };
            // SAFETY: C's class pointer is its static `hwframe_ctx_class`.
            let c_class = unsafe { core::ffi::CStr::from_ptr(class_name_2) };
            assert_eq!(ctx.class().unwrap().name(), Some(c_class));
            // The frames context's own device reference.
            let r_ref = r_dev.as_c_ptr();
            // SAFETY: live references.
            let counts = (unsafe { fx::ref_count(c_dev) }, unsafe {
                fx::ref_count(r_ref)
            });
            assert_eq!(counts, (2, 2));
            let copy = ctx.device().try_clone().unwrap();
            // SAFETY: a live reference.
            assert_eq!(unsafe { fx::ref_count(r_dev.as_c_ptr()) }, 3);
            drop((copy, r));
            // For the unsafe operations below: the test's references.
            {
                // SAFETY: as the comment above this block states.
                unsafe { fx::unref(c) };
                // SAFETY: as the comment above this block states.
                let counts = (unsafe { fx::ref_count(r_dev.as_c_ptr()) }, unsafe {
                    fx::ref_count(c_dev)
                });
                assert_eq!(counts.0, counts.1);
            }
            same_events();
            // SAFETY: the test's device reference.
            unsafe { fx::unref(c_dev) };
            let c_events = fx::take_events();
            drop(r_dev);
            assert_eq!(fx::take_events(), c_events);
        }
    }

    thread_local! {
        /// What each `free` saw when C called it, in order: the backend
        /// events before it, its user data's value and whether `pool` was
        /// still set.
        static FREED: RefCell<Vec<(Vec<Event>, usize, bool)>> = const { RefCell::new(Vec::new()) };
    }

    /// The C reference's `free`: records what it sees, as the Rust one does.
    unsafe extern "C" fn c_free(ctx: *mut ffi::AVHWFramesContext) {
        // SAFETY: C passes the live context it releases; `user_opaque` is a
        // counter value, never dereferenced.
        let (value, pool) = (
            unsafe { (*ctx).user_opaque }.addr(),
            !unsafe { (*ctx).pool }.is_null(),
        );
        FREED.with(|f| f.borrow_mut().push((fx::take_events(), value, pool)));
    }

    /// The Rust side's free callback, recording as [`c_free`].
    fn rust_free(ctx: FramesContextMut<'_>) {
        let c = ctx.as_ref();
        let value = *c.user_data::<usize>().unwrap();
        FREED.with(|f| {
            f.borrow_mut()
                .push((fx::take_events(), value, c.has_pool()));
        });
    }

    fn take_freed() -> Vec<(Vec<Event>, usize, bool)> {
        FREED.with(|f| f.take())
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn free_callbacks_run_as_cs() {
        let kind = Kind::A;
        let (c_dev, r_dev) = devices(kind);
        for (n, fill) in [
            Fill::ok(kind, 2),
            Fill {
                width: 0,
                ..Fill::ok(kind, 0)
            },
            Fill {
                hwctx: FramesHwctx {
                    init_ret: -3,
                    ..FramesHwctx::default()
                },
                ..Fill::ok(kind, 0)
            },
        ]
        .into_iter()
        .enumerate()
        {
            let (c, mut r) = alloc(c_dev, &r_dev);
            fill.c(c);
            // For the unsafe operations below: the only reference to the uninitialised context,
            // whose `free` C code may set before init.
            {
                // SAFETY: as the comment above this block states.
                let p = unsafe { fx::frames_ctx(c) };
                // SAFETY: as the comment above this block states.
                unsafe { (*p).free = Some(c_free) };
                // SAFETY: as the comment above this block states.
                unsafe { (*p).user_opaque = ptr::without_provenance_mut(n + 1) };
            }
            fill.rust(r.context_mut());
            r.context_mut().set_user_data(n + 1).unwrap();
            r.context_mut().set_free_callback(rust_free).ok().unwrap();
            assert_eq!(c_fields(c), rust_fields(r.context()));
            let (c, r) = (c_init(c), r.init());
            assert_eq!(c.err(), r.as_ref().err().copied());
            if let (Ok(c), Ok(r)) = (c, r) {
                same_events();
                // The last release by a frame, after the context's own.
                // SAFETY: a live frames context and a fresh frame.
                let mut cf = unsafe { ffi::av_frame_alloc() };
                // SAFETY: as above.
                assert_eq!(unsafe { ffi::av_hwframe_get_buffer(c, cf, 0) }, 0);
                let rf: HardwareFrame = r.as_ref().get_buffer().unwrap();
                same_events();
                // SAFETY: the test's reference.
                unsafe { fx::unref(c) };
                drop(r);
                assert!(take_freed().is_empty());
                // SAFETY: the test's frame.
                unsafe { ffi::av_frame_free(&raw mut cf) };
                drop(rf);
            }
            let freed = take_freed();
            assert_eq!(freed.len(), 2, "{fill:?}");
            assert_eq!(freed[0].1, n + 1);
            assert_eq!(freed[0].0, freed[1].0);
            assert_eq!(freed[0].1, freed[1].1);
            assert_eq!(freed[0].0.last(), Some(&Event::FramesUninit(kind)));
            // C leaves `pool` pointing at the backend's freed pool there; the
            // hooks clear it first.
            assert!(!freed[1].2);
            assert_eq!(freed[0].2, fill.width > 0 && fill.hwctx.init_ret == 0);
        }
        // SAFETY: the test's device reference.
        unsafe { fx::unref(c_dev) };
        drop(r_dev);
        same_events();
    }

    /// `alloc2` for the C side's caller pool: counts into `opaque`.
    unsafe extern "C" fn c_alloc_counted(
        opaque: *mut c_void,
        size: usize,
    ) -> *mut ffi::AVBufferRef {
        // SAFETY: `opaque` is the test's live counter.
        unsafe { (*opaque.cast::<AtomicUsize>()).fetch_add(1, Ordering::SeqCst) };
        // SAFETY: allocation.
        unsafe { ffi::av_buffer_alloc(size) }
    }

    /// The C side's `free` for a caller pool, as NVDEC's: uninitialises it.
    unsafe extern "C" fn c_free_pool(ctx: *mut ffi::AVHWFramesContext) {
        // SAFETY: `ctx` is to a live object, as the block below relies on, so the
        // place of its `pool` field is in bounds; nothing is accessed here.
        let pool_ptr = unsafe { &raw mut (*ctx).pool };
        // SAFETY: C passes the live context it releases, whose `pool` is the
        // caller's.
        unsafe { ffi::av_buffer_pool_uninit(pool_ptr) };
    }

    /// The Rust side's pool, counting into `allocs`.
    fn rust_pool(allocs: &Arc<AtomicUsize>) -> BufferPoolBox {
        let a = allocs.clone();
        // SAFETY: each buffer is a fresh `size`-byte allocation; the allocator
        // never calls into the pool.
        unsafe {
            BufferPool::with_allocator(fx::SURFACE_SIZE, move |n| {
                a.fetch_add(1, Ordering::SeqCst);
                BufferReference::alloc(n)
            })
        }
        .unwrap()
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn caller_pools_serve_the_frames_as_in_c() {
        for kind in [Kind::A, Kind::B] {
            let (c_dev, r_dev) = devices(kind);
            for prealloc in [0, 1, 4] {
                let (c_allocs, r_allocs) =
                    (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
                let (c, mut r) = alloc(c_dev, &r_dev);
                Fill::ok(kind, prealloc).c(c);
                // For the unsafe operations below: the only reference to the uninitialised context,
                // and a fresh pool its `free` uninitialises; the counter outlives
                // the pool.
                {
                    // SAFETY: as the comment above this block states.
                    let p = unsafe { fx::frames_ctx(c) };
                    // SAFETY: as the comment above this block states: a pool whose opaque is the
                    // test's live counter.
                    let pool = unsafe {
                        ffi::av_buffer_pool_init2(
                            fx::SURFACE_SIZE,
                            Arc::as_ptr(&c_allocs).cast_mut().cast(),
                            Some(c_alloc_counted),
                            None,
                        )
                    };
                    assert!(!pool.is_null());
                    // SAFETY: as the comment above this block states: the context takes it.
                    unsafe { (*p).pool = pool };
                    // SAFETY: as the comment above this block states.
                    unsafe { (*p).free = Some(c_free_pool) };
                }
                Fill::ok(kind, prealloc).rust(r.context_mut());
                // SAFETY: the test backends take any surface-sized bytes.
                unsafe { r.context_mut().set_pool(rust_pool(&r_allocs)) }
                    .ok()
                    .unwrap();
                let (cf, rf) = (c_fields(c), rust_fields(r.context()));
                // C's `free` is set, Rust's hooks are not a callback.
                assert_eq!((cf.6, rf.6), (true, false));
                assert_eq!(cf.5, rf.5);
                let c = c_init(c).unwrap();
                let r = r.init().unwrap();
                same_events();
                let counts = || {
                    (
                        c_allocs.load(Ordering::SeqCst),
                        r_allocs.load(Ordering::SeqCst),
                    )
                };
                assert_eq!(counts(), (prealloc as usize, prealloc as usize));
                // Frames held at once, then returned and taken again.
                let mut held = Vec::new();
                for round in 0..3 {
                    for _ in 0..round + 2 {
                        // For the unsafe operations below: a live frames context and a fresh frame.
                        let cf = {
                            // SAFETY: as the comment above this block states.
                            let f = unsafe { ffi::av_frame_alloc() };
                            // SAFETY: as the comment above this block states.
                            unsafe { assert_eq!(ffi::av_hwframe_get_buffer(c, f, 0), 0) };
                            f
                        };
                        let rf = r.as_ref().get_buffer().unwrap();
                        // SAFETY: the frame C filled.
                        let shape = unsafe { fx::shape(cf) };
                        let dims = (rf.format().into_raw().0, rf.width(), rf.height());
                        assert_eq!((shape.format, shape.width, shape.height), dims);
                        held.push((cf, rf));
                        let (ca, ra) = counts();
                        assert_eq!(ca, ra);
                    }
                    same_events();
                    for (mut cf, rf) in held.drain(..) {
                        // SAFETY: the test's frame.
                        unsafe { ffi::av_frame_free(&raw mut cf) };
                        drop(rf);
                    }
                }
                // SAFETY: the test's reference.
                unsafe { fx::unref(c) };
                drop(r);
                same_events();
            }
            // SAFETY: the test's device reference.
            unsafe { fx::unref(c_dev) };
            drop(r_dev);
            same_events();
        }
    }
}

/// `AVFrame.hw_frames_ctx` through the typed accessors hwcontext wave 2
/// gave it once `AVHWFramesContext` was wrapped (the wave-2 type review):
/// a frame attached to a frames context, copied by reference (`av_frame_clone`,
/// `av_frame_ref`), its reference taken out and handed to another frame,
/// and every frame released, each step against the same steps on raw C
/// frames, comparing the context's count of references and which frames
/// hold it. C's copy of a frame that is not reference counted but holds a
/// context goes through the backend as from a surface, which the wrappers
/// refuse (`FrameRef::try_clone`), so it has no C reference here.
mod frame_hw_frames_context {
    use core::ffi::c_int;
    use core::ptr;

    use ffmpeg_bindings::libavutil::libavutil::buffer::BufferReference;
    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::frame::{Frame, FrameBox, FrameRef};
    use ffmpeg_bindings::libavutil::libavutil::hwcontext::{
        DeviceReferenceBox, FramesReferenceBox,
    };
    use ffmpeg_bindings::libavutil::libavutil::pixfmt::PixelFormat;

    use super::backend::{self as fx, Kind};

    fn count(buf: *const ffi::AVBufferRef) -> c_int {
        // SAFETY: callers pass live references.
        unsafe { fx::ref_count(buf) }
    }

    /// Whether the C frame `f` holds a reference to the context `data`.
    fn c_holds(f: *const ffi::AVFrame, data: *const u8) -> bool {
        // SAFETY: callers pass live frames.
        let held = unsafe { (*f).hw_frames_ctx };
        // SAFETY: NULL or a live reference the frame owns.
        !held.is_null() && ptr::eq(unsafe { (*held).data }, data)
    }

    fn rust_holds(f: FrameRef<'_>, frames: &FramesReferenceBox) -> bool {
        f.hw_frames_context()
            .is_some_and(|held| held.ptr_eq(frames.as_ref()))
    }

    /// A raw C frame of NV12 at 64 x 32 holding one zeroed 64-byte buffer.
    fn c_frame() -> *mut ffi::AVFrame {
        // For the unsafe operations below: a fresh frame and buffer; C allocates both.
        {
            // SAFETY: as the comment above this block states.
            let f = unsafe { ffi::av_frame_alloc() };
            assert!(!f.is_null());
            // SAFETY: as the comment above this block states.
            unsafe { (*f).format = ffi::AVPixelFormat::AV_PIX_FMT_NV12.0 };
            // SAFETY: as the comment above this block states.
            unsafe { (*f).width = 64 };
            // SAFETY: as the comment above this block states.
            unsafe { (*f).height = 32 };
            // SAFETY: no precondition; NULL when C cannot allocate.
            let buf = unsafe { ffi::av_buffer_allocz(64) };
            assert!(!buf.is_null());
            // SAFETY: as the comment above this block states: the frame takes it.
            unsafe { (*f).buf[0] = buf };
            f
        }
    }

    fn rust_frame() -> FrameBox {
        let mut f = Frame::new().unwrap();
        let mut m = f.as_mut();
        m.set_pixel_format(PixelFormat::NV12);
        m.set_width(64);
        m.set_height(32);
        m.set_buffer(0, Some(BufferReference::alloc_zeroed(64).unwrap()));
        f
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn references_follow_c_through_copies_takes_and_releases() {
        for kind in [Kind::A, Kind::B] {
            let c_dev = fx::device(kind);
            // SAFETY: a live initialised test device of `kind`; then the
            // context's only reference, which the test owns.
            let c_frames = unsafe { fx::frames(c_dev, kind, 64, 32) };
            // SAFETY: the only reference to an initialised test device,
            // built as `av_hwdevice_ctx_alloc` builds one.
            let r_dev = unsafe { DeviceReferenceBox::from_c(fx::device(kind)) }.unwrap();
            let format = PixelFormat::from_raw(kind.format());
            let r_frames = r_dev
                .as_ref()
                .create_frames(format, PixelFormat::NV12, 64, 32, 0)
                .unwrap();
            fx::take_events();
            // SAFETY: a live frames context reference.
            let c_data = unsafe { (*c_frames).data }.cast_const();
            let same = |step: &str| {
                assert_eq!(count(c_frames), count(r_frames.as_c_ptr()), "{step}");
            };

            // Attached: C's own field, the typed setter.
            let c_a = c_frame();
            // SAFETY: `c_frames` is a live frames context reference.
            let frames_ref = unsafe { ffi::av_buffer_ref(c_frames) };
            // SAFETY: a live frame, which owns the new reference from here.
            unsafe { (*c_a).hw_frames_ctx = frames_ref };
            let mut r_a = rust_frame();
            assert!(
                r_a.as_mut()
                    .replace_hw_frames_context(r_frames.as_ref().try_clone())
                    .is_none()
            );
            same("attach");
            assert_eq!(c_holds(c_a, c_data), rust_holds(r_a.as_ref(), &r_frames));
            let held = r_a.as_ref().hw_frames_context().unwrap().context();
            // SAFETY: as the comment below states, for this part of the operation.
            let frames_ctx_2 = unsafe { fx::frames_ctx(c_frames) };
            // SAFETY: a live frames context.
            let c_ctx = unsafe { *frames_ctx_2 };
            assert_eq!((c_ctx.width, c_ctx.height), (held.width(), held.height()));
            assert_eq!(c_ctx.format, held.format().into_raw());

            // Copied by reference.
            // SAFETY: a live, reference counted frame.
            let c_b = unsafe { ffi::av_frame_clone(c_a) };
            assert!(!c_b.is_null());
            let r_b = r_a.as_ref().try_clone().unwrap();
            same("clone");
            assert_eq!(c_holds(c_b, c_data), rust_holds(r_b.as_ref(), &r_frames));
            // SAFETY: no precondition.
            let c_c = unsafe { ffi::av_frame_alloc() };
            assert!(!c_c.is_null());
            // SAFETY: a fresh frame, and a live one to reference.
            let ret = unsafe { ffi::av_frame_ref(c_c, c_b) };
            assert_eq!(ret, 0);
            let mut r_c = Frame::new().unwrap();
            r_c.as_mut().try_clone_from(r_b.as_ref()).unwrap();
            same("ref");
            assert_eq!(c_holds(c_c, c_data), rust_holds(r_c.as_ref(), &r_frames));

            // Taken out of one frame and handed to a frame holding one.
            // For the unsafe operations below: live frames; the reference moves from `c_a` to
            // `c_c`, whose own one is released.
            {
                // SAFETY: as the comment above this block states.
                let taken = unsafe { (*c_a).hw_frames_ctx };
                // SAFETY: as the comment above this block states.
                unsafe { (*c_a).hw_frames_ctx = ptr::null_mut() };
                // SAFETY: `c_c` is to a live object, as the block below relies on, so the
                // place of its `hw_frames_ctx` field is in bounds; nothing is accessed here.
                let hw_frames_ctx_ptr = unsafe { &raw mut (*c_c).hw_frames_ctx };
                // SAFETY: as the comment above this block states.
                unsafe { ffi::av_buffer_unref(hw_frames_ctx_ptr) };
                // SAFETY: as the comment above this block states.
                unsafe { (*c_c).hw_frames_ctx = taken };
            }
            let taken = r_a.as_mut().take_hw_frames_context().unwrap();
            drop(r_c.as_mut().replace_hw_frames_context(Some(taken)));
            same("take and replace");
            assert_eq!(c_holds(c_a, c_data), rust_holds(r_a.as_ref(), &r_frames));
            assert_eq!(c_holds(c_c, c_data), rust_holds(r_c.as_ref(), &r_frames));

            // Released one by one, the context's own reference last.
            for (step, c, r) in [("a", c_a, r_a), ("b", c_b, r_b), ("c", c_c, r_c)] {
                let mut c = c;
                // SAFETY: a live frame the test owns, freed once.
                unsafe { ffi::av_frame_free(&raw mut c) };
                drop(r);
                same(step);
            }
            assert_eq!(count(c_frames), 1);
            // For the unsafe operations below: the test's references, released once.
            {
                // SAFETY: as the comment above this block states.
                unsafe { fx::unref(c_frames) };
                // SAFETY: as the comment above this block states.
                unsafe { fx::unref(c_dev) };
            }
            drop((r_frames, r_dev));
            fx::take_events();
        }
    }
}
