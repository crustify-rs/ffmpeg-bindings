//! A hardware frames context over a fake backend, shared by the `hwcontext`
//! suites: the crate's unit tests and the `equiv` and `ub_unsafe`
//! integration tests include it with `#[path]`, after bringing the `-sys`
//! crate into scope as `ffi`.
//!
//! The campaign builds compile no hwcontext backend, so C can make no
//! frames context. This one mirrors the private layouts C casts a frames
//! context's data to (`FFHWFramesContext` and `HWContextType`, in
//! `hwcontext_internal.h`) and plays a backend whose surfaces are software
//! frames of the context's `sw_format` at its allocated size: a hardware
//! frame keeps its surface's `AVFrame *` in `buf[0]` and `data[3]`, as VAAPI
//! keeps a surface ID. Its transfers trust both frames as CUDA's do: for
//! each plane of the software side's format (the surfaces' between two
//! hardware frames) they copy `src->height` rows, chroma-shifted, of the
//! narrower of the two line sizes, unchecked, so a frame a wrapper let
//! through unchecked is overrun under ASan.
#![allow(dead_code)]

use core::ffi::{c_int, c_void};
use core::mem::size_of;
use core::ptr;

use super::ffi;

use ffi::AVHWFrameTransferDirection as Dir;
use ffi::AVPixelFormat as Fmt;

type Unused = Option<unsafe extern "C" fn()>;
type GetFormats =
    Option<unsafe extern "C" fn(*mut ffi::AVHWFramesContext, Dir, *mut *mut Fmt) -> c_int>;
type Transfer = Option<
    unsafe extern "C" fn(
        *mut ffi::AVHWFramesContext,
        *mut ffi::AVFrame,
        *const ffi::AVFrame,
    ) -> c_int,
>;

/// `HWContextType`, a backend's table, field for field.
#[repr(C)]
pub struct HwContextType {
    kind: ffi::AVHWDeviceType,
    name: *const core::ffi::c_char,
    pix_fmts: *const Fmt,
    device_hwctx_size: usize,
    device_hwconfig_size: usize,
    frames_hwctx_size: usize,
    device_create: Unused,
    device_derive: Unused,
    device_init: Unused,
    device_uninit: Unused,
    frames_get_constraints: Unused,
    frames_init: Unused,
    frames_uninit: Unused,
    frames_get_buffer: Unused,
    transfer_get_formats: GetFormats,
    transfer_data_to: Transfer,
    transfer_data_from: Transfer,
    map_to: Unused,
    map_from: Unused,
    frames_derive_to: Unused,
    frames_derive_from: Unused,
}

// SAFETY: the tables below hold NULL and function pointers only, and are
// never written.
unsafe impl Sync for HwContextType {}

impl HwContextType {
    const fn new(kind: ffi::AVHWDeviceType, get: GetFormats, to: Transfer, from: Transfer) -> Self {
        Self {
            kind,
            name: ptr::null(),
            pix_fmts: ptr::null(),
            device_hwctx_size: 0,
            device_hwconfig_size: 0,
            frames_hwctx_size: 0,
            device_create: None,
            device_derive: None,
            device_init: None,
            device_uninit: None,
            frames_get_constraints: None,
            frames_init: None,
            frames_uninit: None,
            frames_get_buffer: None,
            transfer_get_formats: get,
            transfer_data_to: to,
            transfer_data_from: from,
            map_to: None,
            map_from: None,
            frames_derive_to: None,
            frames_derive_from: None,
        }
    }
}

/// A backend that lists formats and transfers as described above.
pub static TRUSTING: HwContextType = HwContextType::new(
    ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_VAAPI,
    Some(get_formats),
    Some(transfer),
    Some(transfer),
);

/// A backend without transfers, as MediaCodec's and OpenHarmony's are: C
/// refuses its listing with `AVERROR(ENOSYS)` and calls its NULL transfers.
pub static INERT: HwContextType = HwContextType::new(
    ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_MEDIACODEC,
    None,
    None,
    None,
);

/// `FFHWFramesContext`, the private context C casts a frames context's data
/// to.
#[repr(C)]
struct FfFramesContext {
    p: ffi::AVHWFramesContext,
    hw_type: *const HwContextType,
    pool_internal: *mut c_void,
    source_frames: *mut ffi::AVBufferRef,
    source_allocation_map_flags: c_int,
}

/// How a fake context's backend behaves; its `user_opaque` points to one.
pub struct Behaviour {
    /// The formats listed for downloads (`FROM`).
    pub from: &'static [Fmt],
    /// The formats listed for uploads (`TO`).
    pub to: &'static [Fmt],
    /// Nonzero: what listing returns instead.
    pub listing_error: c_int,
    /// Whether listing succeeds without storing a list, which C promises
    /// never happens.
    pub no_list: bool,
    /// Nonzero: what a transfer returns instead of copying.
    pub transfer_error: c_int,
}

/// Downloads into YUV 4:2:0 or its luma alone, uploads from YUV 4:2:0.
pub static YUV: Behaviour = Behaviour {
    from: &[Fmt::AV_PIX_FMT_YUV420P, Fmt::AV_PIX_FMT_GRAY8],
    to: &[Fmt::AV_PIX_FMT_YUV420P],
    listing_error: 0,
    no_list: false,
    transfer_error: 0,
};

/// As [`YUV`], but downloads prefer the luma alone.
pub static GRAY_FIRST: Behaviour = Behaviour {
    from: &[Fmt::AV_PIX_FMT_GRAY8, Fmt::AV_PIX_FMT_YUV420P],
    to: &[Fmt::AV_PIX_FMT_YUV420P],
    listing_error: 0,
    no_list: false,
    transfer_error: 0,
};

/// Lists nothing but the terminator, which C promises never happens.
pub static EMPTY: Behaviour = Behaviour {
    from: &[],
    to: &[],
    listing_error: 0,
    no_list: false,
    transfer_error: 0,
};

/// Listing succeeds but stores no list.
pub static NO_LIST: Behaviour = Behaviour {
    from: &[],
    to: &[],
    listing_error: 0,
    no_list: true,
    transfer_error: 0,
};

/// Listing fails with `AVERROR(ENOMEM)`.
pub static LISTING_FAILS: Behaviour = Behaviour {
    from: &[],
    to: &[],
    listing_error: -(ffi::ENOMEM as c_int),
    no_list: false,
    transfer_error: 0,
};

/// Lists as [`YUV`], but every transfer fails with `AVERROR(EIO)`, or
/// declines with `AVERROR(ENOSYS)` for [`DECLINES`].
pub static TRANSFER_FAILS: Behaviour = Behaviour {
    from: &[Fmt::AV_PIX_FMT_YUV420P],
    to: &[Fmt::AV_PIX_FMT_YUV420P],
    listing_error: 0,
    no_list: false,
    transfer_error: -5,
};

/// See [`TRANSFER_FAILS`].
pub static DECLINES: Behaviour = Behaviour {
    from: &[Fmt::AV_PIX_FMT_YUV420P],
    to: &[Fmt::AV_PIX_FMT_YUV420P],
    listing_error: 0,
    no_list: false,
    transfer_error: -(ffi::ENOSYS as c_int),
};

/// The behaviour of the fake context `ctx`.
unsafe fn behaviour<'a>(ctx: *const ffi::AVHWFramesContext) -> &'a Behaviour {
    // SAFETY: every fake context points its `user_opaque` at a static.
    unsafe { &*(*ctx).user_opaque.cast::<Behaviour>() }
}

unsafe extern "C" fn get_formats(
    ctx: *mut ffi::AVHWFramesContext,
    dir: Dir,
    formats: *mut *mut Fmt,
) -> c_int {
    // SAFETY: C passes the live fake context and a writable slot.
    unsafe {
        let b = behaviour(ctx);
        if b.listing_error != 0 || b.no_list {
            return b.listing_error;
        }
        let list = if dir == Dir::AV_HWFRAME_TRANSFER_DIRECTION_FROM {
            b.from
        } else {
            b.to
        };
        let out = ffi::av_malloc_array(list.len() + 1, size_of::<Fmt>()).cast::<Fmt>();
        if out.is_null() {
            return -(ffi::ENOMEM as c_int);
        }
        ptr::copy_nonoverlapping(list.as_ptr(), out, list.len());
        out.add(list.len()).write(Fmt::AV_PIX_FMT_NONE);
        formats.write(out);
        0
    }
}

/// The first four plane pointers and line sizes of a software frame, or of
/// a hardware frame's surface.
unsafe fn planes(f: *const ffi::AVFrame) -> ([*mut u8; 4], [c_int; 4]) {
    // SAFETY: a live frame; a hardware frame of a fake context keeps a live
    // surface in `data[3]`.
    unsafe {
        let f = if (*f).hw_frames_ctx.is_null() {
            f
        } else {
            (*f).data[3].cast_const().cast::<ffi::AVFrame>()
        };
        let d = (*f).data;
        let l = (*f).linesize;
        ([d[0], d[1], d[2], d[3]], [l[0], l[1], l[2], l[3]])
    }
}

/// The rows of plane `i` of an image `height` rows high in `format`.
unsafe fn plane_rows(format: Fmt, i: usize, height: c_int) -> c_int {
    // SAFETY: C's descriptor table entry for a known format.
    let shift = unsafe { (*ffi::av_pix_fmt_desc_get(format)).log2_chroma_h };
    if i == 1 || i == 2 {
        -((-height) >> shift)
    } else {
        height
    }
}

unsafe extern "C" fn transfer(
    ctx: *mut ffi::AVHWFramesContext,
    dst: *mut ffi::AVFrame,
    src: *const ffi::AVFrame,
) -> c_int {
    // SAFETY: C passes the live fake context and two live frames, at least
    // one of them a hardware frame of a fake context; everything past here
    // trusts them, as described in the module documentation.
    unsafe {
        let b = behaviour(ctx);
        if b.transfer_error != 0 {
            return b.transfer_error;
        }
        let format = if (*dst).hw_frames_ctx.is_null() {
            Fmt((*dst).format)
        } else if (*src).hw_frames_ctx.is_null() {
            Fmt((*src).format)
        } else {
            (*ctx).sw_format
        };
        let ((dp, dl), (sp, sl)) = (planes(dst), planes(src));
        let count = ffi::av_pix_fmt_count_planes(format).clamp(0, 4) as usize;
        for i in 0..count {
            let len = sl[i].min(dl[i]) as usize;
            for row in 0..plane_rows(format, i, (*src).height) as isize {
                ptr::copy_nonoverlapping(
                    sp[i].offset(row * sl[i] as isize),
                    dp[i].offset(row * dl[i] as isize),
                    len,
                );
            }
        }
        0
    }
}

unsafe extern "C" fn free_context(_opaque: *mut c_void, data: *mut u8) {
    // SAFETY: C releases a fake context once, after its last reference.
    unsafe {
        let ctx = data.cast::<FfFramesContext>();
        ffi::av_buffer_unref(&raw mut (*ctx).source_frames);
        ffi::av_free(data.cast());
    }
}

/// A new reference to a fake frames context of `backend`, behaving as
/// `behaviour`, whose surfaces are `width` x `height` in `sw_format` and
/// whose hardware frames are in `format`; NULL when C cannot allocate.
pub fn new_context(
    backend: &'static HwContextType,
    behaviour: &'static Behaviour,
    format: Fmt,
    sw_format: Fmt,
    width: c_int,
    height: c_int,
) -> *mut ffi::AVBufferRef {
    let size = size_of::<FfFramesContext>();
    // SAFETY: a fresh zeroed block, filled before C sees it, then handed to
    // a new reference whose release frees it.
    unsafe {
        let ctx = ffi::av_mallocz(size).cast::<FfFramesContext>();
        if ctx.is_null() {
            return ptr::null_mut();
        }
        (*ctx).p.format = format;
        (*ctx).p.sw_format = sw_format;
        (*ctx).p.width = width;
        (*ctx).p.height = height;
        (*ctx).p.user_opaque = ptr::from_ref(behaviour).cast_mut().cast();
        (*ctx).hw_type = backend;
        let buf = ffi::av_buffer_create(ctx.cast(), size, Some(free_context), ptr::null_mut(), 0);
        if buf.is_null() {
            ffi::av_free(ctx.cast());
        }
        buf
    }
}

/// [`new_context`] of [`TRUSTING`] for `behaviour`, VAAPI surfaces of YUV
/// 4:2:0 at `width` x `height`.
pub fn yuv_context(
    behaviour: &'static Behaviour,
    width: c_int,
    height: c_int,
) -> *mut ffi::AVBufferRef {
    new_context(
        &TRUSTING,
        behaviour,
        Fmt::AV_PIX_FMT_VAAPI,
        Fmt::AV_PIX_FMT_YUV420P,
        width,
        height,
    )
}

/// Marks the fake context `ctx` derived from `source`, a new reference to
/// which it takes, as `av_hwframe_ctx_create_derived` does.
///
/// # Safety
///
/// `ctx` and `source` must be live references to fake contexts.
pub unsafe fn derive(ctx: *mut ffi::AVBufferRef, source: *mut ffi::AVBufferRef) {
    // SAFETY: the caller's references; the old source, if any, released.
    unsafe {
        let c = (*ctx).data.cast::<FfFramesContext>();
        ffi::av_buffer_unref(&raw mut (*c).source_frames);
        (*c).source_frames = ffi::av_buffer_ref(source);
    }
}

/// A new software frame of `format` at `width` x `height`, its bytes zeroed
/// and then each image byte `fill(plane, row, column)`; NULL on failure.
pub fn new_sw_frame(
    format: Fmt,
    width: c_int,
    height: c_int,
    fill: impl Fn(usize, usize, usize) -> u8,
) -> *mut ffi::AVFrame {
    // SAFETY: a fresh frame C gives buffers; every write lies in its image.
    unsafe {
        let f = ffi::av_frame_alloc();
        if f.is_null() {
            return f;
        }
        (*f).format = format.0;
        (*f).width = width;
        (*f).height = height;
        if ffi::av_frame_get_buffer(f, 0) < 0 {
            let mut f = f;
            ffi::av_frame_free(&mut f);
            return ptr::null_mut();
        }
        for i in 0..8 {
            let b = (*f).buf[i];
            if !b.is_null() {
                ptr::write_bytes((*b).data, 0, (*b).size);
            }
        }
        fill_image(f, format, width, height, fill);
        f
    }
}

/// Writes `fill(plane, row, column)` to every byte of `frame`'s
/// `width` x `height` image of `format`.
///
/// # Safety
///
/// The frame's planes must hold that image.
pub unsafe fn fill_image(
    frame: *mut ffi::AVFrame,
    format: Fmt,
    width: c_int,
    height: c_int,
    fill: impl Fn(usize, usize, usize) -> u8,
) {
    // SAFETY: the caller's contract.
    unsafe {
        let (d, l) = planes(frame);
        let count = ffi::av_pix_fmt_count_planes(format).clamp(0, 4) as usize;
        for i in 0..count {
            let len = ffi::av_image_get_linesize(format, width, i as c_int).max(0) as usize;
            for row in 0..plane_rows(format, i, height).max(0) as usize {
                let line = d[i].offset(row as isize * l[i] as isize);
                for col in 0..len {
                    line.add(col).write(fill(i, row, col));
                }
            }
        }
    }
}

/// The bytes of `frame`'s `width` x `height` image of `format`, plane by
/// plane and row by row, without padding; a hardware frame's are its
/// surface's.
///
/// # Safety
///
/// The frame's planes (its surface's) must hold that image, initialised.
pub unsafe fn image(
    frame: *const ffi::AVFrame,
    format: Fmt,
    width: c_int,
    height: c_int,
) -> Vec<u8> {
    let mut out = Vec::new();
    // SAFETY: the caller's contract.
    unsafe {
        let (d, l) = planes(frame);
        let count = ffi::av_pix_fmt_count_planes(format).clamp(0, 4) as usize;
        for i in 0..count {
            let len = ffi::av_image_get_linesize(format, width, i as c_int).max(0) as usize;
            for row in 0..plane_rows(format, i, height).max(0) as usize {
                let line = d[i].offset(row as isize * l[i] as isize);
                out.extend_from_slice(core::slice::from_raw_parts(line, len));
            }
        }
    }
    out
}

unsafe extern "C" fn free_surface(_opaque: *mut c_void, data: *mut u8) {
    let mut surface = data.cast::<ffi::AVFrame>();
    // SAFETY: C releases a surface once, after its last reference.
    unsafe { ffi::av_frame_free(&mut surface) };
}

/// A new hardware frame of the fake context `ctx`, holding a new surface
/// whose image bytes are `fill(plane, row, column)`, at the context's size
/// and in its hardware format, as `av_hwframe_get_buffer` makes one; NULL
/// on failure.
///
/// # Safety
///
/// `ctx` must be a live reference to a fake context.
pub unsafe fn new_hw_frame(
    ctx: *mut ffi::AVBufferRef,
    fill: impl Fn(usize, usize, usize) -> u8,
) -> *mut ffi::AVFrame {
    // SAFETY: the caller's context; fresh frames, the surface handed to the
    // buffer whose release frees it.
    unsafe {
        let p = (*ctx).data.cast::<ffi::AVHWFramesContext>();
        let (w, h) = ((*p).width, (*p).height);
        let mut surface = new_sw_frame((*p).sw_format, w, h, fill);
        let mut f = ffi::av_frame_alloc();
        if surface.is_null() || f.is_null() {
            ffi::av_frame_free(&mut surface);
            ffi::av_frame_free(&mut f);
            return ptr::null_mut();
        }
        (*f).format = (*p).format.0;
        (*f).width = w;
        (*f).height = h;
        (*f).buf[0] = ffi::av_buffer_create(
            surface.cast(),
            size_of::<ffi::AVFrame>(),
            Some(free_surface),
            ptr::null_mut(),
            0,
        );
        (*f).hw_frames_ctx = ffi::av_buffer_ref(ctx);
        if (*f).buf[0].is_null() || (*f).hw_frames_ctx.is_null() {
            if (*f).buf[0].is_null() {
                ffi::av_frame_free(&mut surface);
            }
            ffi::av_frame_free(&mut f);
            return ptr::null_mut();
        }
        (*f).data[3] = surface.cast();
        f
    }
}

/// The bytes of the surface of `frame`, a hardware frame of a fake context,
/// at the context's size.
///
/// # Safety
///
/// As [`new_hw_frame`]'s result.
pub unsafe fn surface_image(frame: *const ffi::AVFrame) -> Vec<u8> {
    // SAFETY: the caller's frame and its context.
    unsafe {
        let p = (*(*frame).hw_frames_ctx)
            .data
            .cast::<ffi::AVHWFramesContext>();
        image(frame, (*p).sw_format, (*p).width, (*p).height)
    }
}

/// The pattern most tests fill images with: distinct per plane, row and
/// column, offset by `seed`.
pub fn pattern(seed: u8) -> impl Fn(usize, usize, usize) -> u8 {
    move |plane, row, col| seed.wrapping_add((plane * 89 + row * 31 + col * 7) as u8)
}
