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
//! through unchecked is overrun under `ASan`.
#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_ptr_alignment,
    clippy::cast_sign_loss,
    reason = "test inputs are chosen within range, and the casts mirror the C conversions under comparison"
)]

use core::ffi::{c_int, c_void};
use core::mem::size_of;
use core::ptr;

use ffi::{AVHWFrameTransferDirection as Dir, AVPixelFormat as Fmt};

use super::ffi;

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
pub(crate) struct HwContextType {
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
pub(crate) static TRUSTING: HwContextType = HwContextType::new(
    ffi::AV_HWDEVICE_TYPE_VAAPI,
    Some(get_formats),
    Some(transfer),
    Some(transfer),
);

/// A backend without transfers, as MediaCodec's and OpenHarmony's are: C
/// refuses its listing with `AVERROR(ENOSYS)` and calls its NULL transfers.
pub(crate) static INERT: HwContextType =
    HwContextType::new(ffi::AV_HWDEVICE_TYPE_MEDIACODEC, None, None, None);

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
pub(crate) struct Behaviour {
    /// The formats listed for downloads (`FROM`).
    pub(crate) from: &'static [Fmt],
    /// The formats listed for uploads (`TO`).
    pub(crate) to: &'static [Fmt],
    /// Nonzero: what listing returns instead.
    pub(crate) listing_error: c_int,
    /// Whether listing succeeds without storing a list, which C promises
    /// never happens.
    pub(crate) no_list: bool,
    /// Nonzero: what a transfer returns instead of copying.
    pub(crate) transfer_error: c_int,
}

/// Downloads into YUV 4:2:0 or its luma alone, uploads from YUV 4:2:0.
pub(crate) static YUV: Behaviour = Behaviour {
    from: &[ffi::AV_PIX_FMT_YUV420P, ffi::AV_PIX_FMT_GRAY8],
    to: &[ffi::AV_PIX_FMT_YUV420P],
    listing_error: 0,
    no_list: false,
    transfer_error: 0,
};

/// As [`YUV`], but downloads prefer the luma alone.
pub(crate) static GRAY_FIRST: Behaviour = Behaviour {
    from: &[ffi::AV_PIX_FMT_GRAY8, ffi::AV_PIX_FMT_YUV420P],
    to: &[ffi::AV_PIX_FMT_YUV420P],
    listing_error: 0,
    no_list: false,
    transfer_error: 0,
};

/// Lists nothing but the terminator, which C promises never happens.
pub(crate) static EMPTY: Behaviour = Behaviour {
    from: &[],
    to: &[],
    listing_error: 0,
    no_list: false,
    transfer_error: 0,
};

/// Listing succeeds but stores no list.
pub(crate) static NO_LIST: Behaviour = Behaviour {
    from: &[],
    to: &[],
    listing_error: 0,
    no_list: true,
    transfer_error: 0,
};

/// Listing fails with `AVERROR(ENOMEM)`.
pub(crate) static LISTING_FAILS: Behaviour = Behaviour {
    from: &[],
    to: &[],
    listing_error: -(ffi::ENOMEM as c_int),
    no_list: false,
    transfer_error: 0,
};

/// Lists as [`YUV`], but every transfer fails with `AVERROR(EIO)`, or
/// declines with `AVERROR(ENOSYS)` for [`DECLINES`].
pub(crate) static TRANSFER_FAILS: Behaviour = Behaviour {
    from: &[ffi::AV_PIX_FMT_YUV420P],
    to: &[ffi::AV_PIX_FMT_YUV420P],
    listing_error: 0,
    no_list: false,
    transfer_error: -5,
};

/// See [`TRANSFER_FAILS`].
pub(crate) static DECLINES: Behaviour = Behaviour {
    from: &[ffi::AV_PIX_FMT_YUV420P],
    to: &[ffi::AV_PIX_FMT_YUV420P],
    listing_error: 0,
    no_list: false,
    transfer_error: -(ffi::ENOSYS as c_int),
};

/// The behaviour of the fake context `ctx`.
unsafe fn behaviour<'a>(ctx: *const ffi::AVHWFramesContext) -> &'a Behaviour {
    // SAFETY: as the comment below states, for this part of the operation.
    let user_opaque_2 = unsafe { (*ctx).user_opaque };
    // SAFETY: every fake context points its `user_opaque` at a static.
    unsafe { &*user_opaque_2.cast::<Behaviour>() }
}

unsafe extern "C" fn get_formats(
    ctx: *mut ffi::AVHWFramesContext,
    dir: Dir,
    formats: *mut *mut Fmt,
) -> c_int {
    // SAFETY: C passes the live fake context.
    let b = unsafe { behaviour(ctx) };
    if b.listing_error != 0 || b.no_list {
        return b.listing_error;
    }
    let list = if dir == ffi::AV_HWFRAME_TRANSFER_DIRECTION_FROM {
        b.from
    } else {
        b.to
    };
    // SAFETY: no precondition; NULL when C cannot allocate.
    let out = unsafe { ffi::av_malloc_array(list.len() + 1, size_of::<Fmt>()) }.cast::<Fmt>();
    if out.is_null() {
        return -(ffi::ENOMEM as c_int);
    }
    // SAFETY: the fresh block holds `list.len() + 1` formats.
    unsafe { ptr::copy_nonoverlapping(list.as_ptr(), out, list.len()) };
    // SAFETY: as above: its last entry.
    let end = unsafe { out.add(list.len()) };
    // SAFETY: as above: the terminator.
    unsafe { end.write(ffi::AV_PIX_FMT_NONE) };
    // SAFETY: C passes a writable slot, which takes the list.
    unsafe { formats.write(out) };
    0
}

/// The first four plane pointers and line sizes of a software frame, or of
/// a hardware frame's surface.
unsafe fn planes(f: *const ffi::AVFrame) -> ([*mut u8; 4], [c_int; 4]) {
    // A live frame; a hardware frame of a fake context keeps a live surface in `data[3]`.
    // SAFETY: as above: a read of its frames reference.
    let f = if unsafe { (*f).hw_frames_ctx }.is_null() {
        f
    } else {
        // SAFETY: as above: a read of its surface.
        unsafe { (*f).data[3] }.cast_const().cast::<ffi::AVFrame>()
    };
    // SAFETY: as above: a copy of the planes.
    let d = unsafe { (*f).data };
    // SAFETY: as above: a copy of the line sizes.
    let l = unsafe { (*f).linesize };
    ([d[0], d[1], d[2], d[3]], [l[0], l[1], l[2], l[3]])
}

/// The rows of plane `i` of an image `height` rows high in `format`.
unsafe fn plane_rows(format: Fmt, i: usize, height: c_int) -> c_int {
    // SAFETY: as the comment below states, for this part of the operation.
    let pix_fmt_desc_get = unsafe { ffi::av_pix_fmt_desc_get(format) };
    // SAFETY: C's descriptor table entry for a known format.
    let shift = unsafe { (*pix_fmt_desc_get).log2_chroma_h };
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
    // C passes the live fake context and two live frames, at least one of them a hardware frame
    // of a fake context; everything past here trusts them, as described in the module
    // documentation.
    // SAFETY: as above.
    let b = unsafe { behaviour(ctx) };
    if b.transfer_error != 0 {
        return b.transfer_error;
    }
    // SAFETY: as above: reads of the frames' frames references.
    let (dst_hw, src_hw) = (
        !unsafe { (*dst).hw_frames_ctx }.is_null(),
        !unsafe { (*src).hw_frames_ctx }.is_null(),
    );
    let format = if !dst_hw {
        // SAFETY: as above: a read of the software frame's format.
        unsafe { (*dst).format }
    } else if !src_hw {
        // SAFETY: as above: a read of the software frame's format.
        unsafe { (*src).format }
    } else {
        // SAFETY: as above: a read of the context's software format.
        unsafe { (*ctx).sw_format }
    };
    // SAFETY: as above: two live frames.
    let ((dp, dl), (sp, sl)) = (unsafe { planes(dst) }, unsafe { planes(src) });
    // SAFETY: no precondition.
    let count = unsafe { ffi::av_pix_fmt_count_planes(format) }.clamp(0, 4) as usize;
    // SAFETY: as above: a read of the source's height.
    let height = unsafe { (*src).height };
    for i in 0..count {
        let len = sl[i].min(dl[i]) as usize;
        // SAFETY: a known format.
        for row in 0..unsafe { plane_rows(format, i, height) } as isize {
            // SAFETY: as above, trusted: row `row` of each plane.
            let (from, to) = (unsafe { sp[i].offset(row * sl[i] as isize) }, unsafe {
                dp[i].offset(row * dl[i] as isize)
            });
            // SAFETY: as above, trusted: `len` bytes, within both rows.
            unsafe { ptr::copy_nonoverlapping(from, to, len) };
        }
    }
    0
}

unsafe extern "C" fn free_context(_opaque: *mut c_void, data: *mut u8) {
    // For the unsafe operations below: C releases a fake context once, after its last reference.
    {
        let ctx = data.cast::<FfFramesContext>();
        // SAFETY: `ctx` is to a live object, as the block below relies on, so the
        // place of its `source_frames` field is in bounds; nothing is accessed here.
        let source_frames_ptr = unsafe { &raw mut (*ctx).source_frames };
        // SAFETY: as the comment above this block states.
        unsafe { ffi::av_buffer_unref(source_frames_ptr) };
        // SAFETY: as the comment above this block states.
        unsafe { ffi::av_free(data.cast()) };
    }
}

/// A new reference to a fake frames context of `backend`, behaving as
/// `behaviour`, whose surfaces are `width` x `height` in `sw_format` and
/// whose hardware frames are in `format`; NULL when C cannot allocate.
pub(crate) fn new_context(
    backend: &'static HwContextType,
    behaviour: &'static Behaviour,
    format: Fmt,
    sw_format: Fmt,
    width: c_int,
    height: c_int,
) -> *mut ffi::AVBufferRef {
    let size = size_of::<FfFramesContext>();
    // For the unsafe operations below: a fresh zeroed block, filled before C sees it, then handed
    // to a new reference whose release frees it.
    {
        // SAFETY: as the comment above this block states.
        let ctx = unsafe { ffi::av_mallocz(size).cast::<FfFramesContext>() };
        if ctx.is_null() {
            return ptr::null_mut();
        }
        // SAFETY: as the comment above this block states.
        unsafe { (*ctx).p.format = format };
        // SAFETY: as the comment above this block states.
        unsafe { (*ctx).p.sw_format = sw_format };
        // SAFETY: as the comment above this block states.
        unsafe { (*ctx).p.width = width };
        // SAFETY: as the comment above this block states.
        unsafe { (*ctx).p.height = height };
        // SAFETY: as the comment above this block states.
        unsafe { (*ctx).p.user_opaque = ptr::from_ref(behaviour).cast_mut().cast() };
        // SAFETY: as the comment above this block states.
        unsafe { (*ctx).hw_type = backend };
        // SAFETY: as the comment above this block states.
        let buf = unsafe {
            ffi::av_buffer_create(ctx.cast(), size, Some(free_context), ptr::null_mut(), 0)
        };
        // SAFETY: as the comment above this block states.
        unsafe {
            if buf.is_null() {
                ffi::av_free(ctx.cast());
            }
        }
        buf
    }
}

/// [`new_context`] of [`TRUSTING`] for `behaviour`, VAAPI surfaces of YUV
/// 4:2:0 at `width` x `height`.
pub(crate) fn yuv_context(
    behaviour: &'static Behaviour,
    width: c_int,
    height: c_int,
) -> *mut ffi::AVBufferRef {
    new_context(
        &TRUSTING,
        behaviour,
        ffi::AV_PIX_FMT_VAAPI,
        ffi::AV_PIX_FMT_YUV420P,
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
pub(crate) unsafe fn derive(ctx: *mut ffi::AVBufferRef, source: *mut ffi::AVBufferRef) {
    // SAFETY: by the caller's contract: a read of the live reference's data.
    let c = unsafe { (*ctx).data }.cast::<FfFramesContext>();
    // SAFETY: as above, a fake context: the place of its `source_frames`.
    let source_frames_ptr = unsafe { &raw mut (*c).source_frames };
    // SAFETY: NULL or the old source, released.
    unsafe { ffi::av_buffer_unref(source_frames_ptr) };
    // SAFETY: by the caller's contract: a live reference, to which C adds one.
    let source = unsafe { ffi::av_buffer_ref(source) };
    // SAFETY: as above: the context takes it.
    unsafe { (*c).source_frames = source };
}

/// A new software frame of `format` at `width` x `height`, its bytes zeroed
/// and then each image byte `fill(plane, row, column)`; NULL on failure.
pub(crate) fn new_sw_frame(
    format: Fmt,
    width: c_int,
    height: c_int,
    fill: impl Fn(usize, usize, usize) -> u8,
) -> *mut ffi::AVFrame {
    // SAFETY: no precondition; NULL when C cannot allocate.
    let mut f = unsafe { ffi::av_frame_alloc() };
    if f.is_null() {
        return f;
    }
    // SAFETY: a fresh frame: a write of its format.
    unsafe { (*f).format = format };
    // SAFETY: as above, its width.
    unsafe { (*f).width = width };
    // SAFETY: as above, its height.
    unsafe { (*f).height = height };
    // SAFETY: as above: C gives it buffers.
    if unsafe { ffi::av_frame_get_buffer(f, 0) } < 0 {
        // SAFETY: as above, released once.
        unsafe { ffi::av_frame_free(&raw mut f) };
        return ptr::null_mut();
    }
    // SAFETY: as above: a copy of its references.
    let bufs = unsafe { (*f).buf };
    for b in bufs.into_iter().filter(|b| !b.is_null()) {
        // SAFETY: a live reference C just allocated: reads of its window.
        let (data, size) = (unsafe { (*b).data }, unsafe { (*b).size });
        // SAFETY: as above: its writable bytes, zeroed.
        unsafe { ptr::write_bytes(data, 0, size) };
    }
    // SAFETY: the frame's planes hold its image.
    unsafe { fill_image(f, format, width, height, fill) };
    f
}

/// Writes `fill(plane, row, column)` to every byte of `frame`'s
/// `width` x `height` image of `format`.
///
/// # Safety
///
/// The frame's planes must hold that image.
pub(crate) unsafe fn fill_image(
    frame: *mut ffi::AVFrame,
    format: Fmt,
    width: c_int,
    height: c_int,
    fill: impl Fn(usize, usize, usize) -> u8,
) {
    // SAFETY: by the caller's contract.
    let (d, l) = unsafe { planes(frame) };
    // SAFETY: no precondition.
    let count = unsafe { ffi::av_pix_fmt_count_planes(format) }.clamp(0, 4) as usize;
    for i in 0..count {
        // SAFETY: no precondition.
        let len = unsafe { ffi::av_image_get_linesize(format, width, i as c_int) }.max(0) as usize;
        // SAFETY: a known format.
        for row in 0..unsafe { plane_rows(format, i, height) }.max(0) as usize {
            // SAFETY: by the caller's contract: row `row` of plane `i`.
            let line = unsafe { d[i].offset(row as isize * l[i] as isize) };
            for col in 0..len {
                // SAFETY: as above: byte `col < len` of the row.
                let at = unsafe { line.add(col) };
                // SAFETY: as above.
                unsafe { at.write(fill(i, row, col)) };
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
pub(crate) unsafe fn image(
    frame: *const ffi::AVFrame,
    format: Fmt,
    width: c_int,
    height: c_int,
) -> Vec<u8> {
    let mut out = Vec::new();
    // SAFETY: by the caller's contract.
    let (d, l) = unsafe { planes(frame) };
    // SAFETY: no precondition.
    let count = unsafe { ffi::av_pix_fmt_count_planes(format) }.clamp(0, 4) as usize;
    for i in 0..count {
        // SAFETY: no precondition.
        let len = unsafe { ffi::av_image_get_linesize(format, width, i as c_int) }.max(0) as usize;
        // SAFETY: a known format.
        for row in 0..unsafe { plane_rows(format, i, height) }.max(0) as usize {
            // SAFETY: by the caller's contract: row `row` of plane `i`.
            let line = unsafe { d[i].offset(row as isize * l[i] as isize) };
            // SAFETY: as above: `len` initialised bytes of the row.
            out.extend_from_slice(unsafe { core::slice::from_raw_parts(line, len) });
        }
    }
    out
}

unsafe extern "C" fn free_surface(_opaque: *mut c_void, data: *mut u8) {
    let mut surface = data.cast::<ffi::AVFrame>();
    // SAFETY: C releases a surface once, after its last reference.
    unsafe { ffi::av_frame_free(&raw mut surface) };
}

/// A new hardware frame of the fake context `ctx`, holding a new surface
/// whose image bytes are `fill(plane, row, column)`, at the context's size
/// and in its hardware format, as `av_hwframe_get_buffer` makes one; NULL
/// on failure.
///
/// # Safety
///
/// `ctx` must be a live reference to a fake context.
pub(crate) unsafe fn new_hw_frame(
    ctx: *mut ffi::AVBufferRef,
    fill: impl Fn(usize, usize, usize) -> u8,
) -> *mut ffi::AVFrame {
    // SAFETY: by the caller's contract: a read of the live reference's data.
    let p = unsafe { (*ctx).data }.cast::<ffi::AVHWFramesContext>();
    // SAFETY: as above, a fake context: reads of its geometry and formats.
    let (w, h, sw_format, format) = (
        unsafe { (*p).width },
        unsafe { (*p).height },
        unsafe { (*p).sw_format },
        unsafe { (*p).format },
    );
    let mut surface = new_sw_frame(sw_format, w, h, fill);
    // SAFETY: no precondition; NULL when C cannot allocate.
    let mut f = unsafe { ffi::av_frame_alloc() };
    if surface.is_null() || f.is_null() {
        // SAFETY: NULL or the fresh surface, released once.
        unsafe { ffi::av_frame_free(&raw mut surface) };
        // SAFETY: NULL or the fresh frame, released once.
        unsafe { ffi::av_frame_free(&raw mut f) };
        return ptr::null_mut();
    }
    // SAFETY: the fresh frame: a write of its format.
    unsafe { (*f).format = format };
    // SAFETY: as above, its width.
    unsafe { (*f).width = w };
    // SAFETY: as above, its height.
    unsafe { (*f).height = h };
    // SAFETY: the surface, which the buffer owns from here, released by `free_surface`.
    let buf = unsafe {
        ffi::av_buffer_create(
            surface.cast(),
            size_of::<ffi::AVFrame>(),
            Some(free_surface),
            ptr::null_mut(),
            0,
        )
    };
    // SAFETY: as above: the frame takes the buffer.
    unsafe { (*f).buf[0] = buf };
    // SAFETY: by the caller's contract: a live reference, to which C adds one.
    let frames = unsafe { ffi::av_buffer_ref(ctx) };
    // SAFETY: as above: the frame takes it.
    unsafe { (*f).hw_frames_ctx = frames };
    if buf.is_null() || frames.is_null() {
        if buf.is_null() {
            // SAFETY: the surface no buffer took, released once.
            unsafe { ffi::av_frame_free(&raw mut surface) };
        }
        // SAFETY: the fresh frame and what it took, released once.
        unsafe { ffi::av_frame_free(&raw mut f) };
        return ptr::null_mut();
    }
    // SAFETY: as above: the frame's surface.
    unsafe { (*f).data[3] = surface.cast() };
    f
}

/// The bytes of the surface of `frame`, a hardware frame of a fake context,
/// at the context's size.
///
/// # Safety
///
/// As [`new_hw_frame`]'s result.
pub(crate) unsafe fn surface_image(frame: *const ffi::AVFrame) -> Vec<u8> {
    // SAFETY: by the caller's contract: a read of the frame's frames reference.
    let frames = unsafe { (*frame).hw_frames_ctx };
    // SAFETY: as above: a read of the live reference's data.
    let p = unsafe { (*frames).data }.cast::<ffi::AVHWFramesContext>();
    // SAFETY: as above, a fake context: reads of its software format and size.
    let (format, width, height) = (unsafe { (*p).sw_format }, unsafe { (*p).width }, unsafe {
        (*p).height
    });
    // SAFETY: by the caller's contract, the surface holds that image.
    unsafe { image(frame, format, width, height) }
}

/// The pattern most tests fill images with: distinct per plane, row and
/// column, offset by `seed`.
pub(crate) fn pattern(seed: u8) -> impl Fn(usize, usize, usize) -> u8 {
    move |plane, row, col| seed.wrapping_add((plane * 89 + row * 31 + col * 7) as u8)
}
