//! A test hardware backend for the `hwcontext` suites: the crate's unit tests
//! and the `equiv` integration tests include it with `#[path]`, after bringing
//! the `-sys` crate into scope as `ffi`.
//!
//! The campaign builds compile no hwcontext backend, so `av_hwdevice_ctx_alloc`
//! fails for every device type and no device or frames context can be made
//! through the API. [`alloc_device`] stands in for it: it builds a device
//! context exactly as `av_hwdevice_ctx_alloc` does (hwcontext.c's private
//! `FFHWDeviceContext`: the public struct, the backend's `HWContextType`, the
//! source device; read-only buffer; a release mirroring `hwdevice_ctx_free`)
//! over one of two backends of its own, [`Kind::A`] and [`Kind::B`]. Every C
//! routine past allocation then runs for real: `av_hwdevice_ctx_init`,
//! `av_hwframe_ctx_alloc`/`init`, `av_hwframe_get_buffer`, `av_hwframe_map`,
//! the derivations. The mirrors follow `hwcontext.c` and `hwcontext_internal.h`
//! of the campaign's FFmpeg revision.
//!
//! The backends record what C asks of them in a thread-local [`Event`] log
//! ([`take_events`]); contexts are used on the thread that made them. Their
//! results are configured through the contexts' `hwctx` structs
//! ([`DeviceHwctx`], [`FramesHwctx`]), zeroed (all success) by default.
//!
//! Kind A (`VAAPI`, hardware format `VAAPI`) allocates surfaces from a pool,
//! maps them to software frames, or into another frames context
//! (`map_from`), and transfers data to and from them ([`transfer`]); kind B (`DRM`, hardware format `DRM_PRIME`) maps frames of
//! another context into its own (`map_to`) and derives frames contexts
//! (`frames_derive_to`), and transfers nothing. A surface is a
//! [`SURFACE_SIZE`]-byte pool buffer, zeroed, which `data[3]` points at, as
//! VA-API's surface IDs sit there.
#![allow(dead_code)]

use core::cell::RefCell;
use core::ffi::{c_char, c_int, c_void};
use core::mem::{size_of, zeroed};
use core::ptr::{self, addr_of, addr_of_mut, null_mut};

use super::ffi;

type DeviceCtx = ffi::AVHWDeviceContext;
type FramesCtx = ffi::AVHWFramesContext;

/// hwcontext_internal.h's `HWContextType`, the backend's routines and sizes.
#[repr(C)]
pub struct HwContextType {
    pub type_: ffi::AVHWDeviceType,
    pub name: *const c_char,
    pub pix_fmts: *const ffi::AVPixelFormat,
    pub device_hwctx_size: usize,
    pub device_hwconfig_size: usize,
    pub frames_hwctx_size: usize,
    pub device_create: Option<
        unsafe extern "C" fn(*mut DeviceCtx, *const c_char, *mut ffi::AVDictionary, c_int) -> c_int,
    >,
    pub device_derive: Option<
        unsafe extern "C" fn(
            *mut DeviceCtx,
            *mut DeviceCtx,
            *mut ffi::AVDictionary,
            c_int,
        ) -> c_int,
    >,
    pub device_init: Option<unsafe extern "C" fn(*mut DeviceCtx) -> c_int>,
    pub device_uninit: Option<unsafe extern "C" fn(*mut DeviceCtx)>,
    pub frames_get_constraints: Option<
        unsafe extern "C" fn(
            *mut DeviceCtx,
            *const c_void,
            *mut ffi::AVHWFramesConstraints,
        ) -> c_int,
    >,
    pub frames_init: Option<unsafe extern "C" fn(*mut FramesCtx) -> c_int>,
    pub frames_uninit: Option<unsafe extern "C" fn(*mut FramesCtx)>,
    pub frames_get_buffer: Option<unsafe extern "C" fn(*mut FramesCtx, *mut ffi::AVFrame) -> c_int>,
    pub transfer_get_formats: Option<
        unsafe extern "C" fn(
            *mut FramesCtx,
            ffi::AVHWFrameTransferDirection,
            *mut *mut ffi::AVPixelFormat,
        ) -> c_int,
    >,
    pub transfer_data_to: Option<
        unsafe extern "C" fn(*mut FramesCtx, *mut ffi::AVFrame, *const ffi::AVFrame) -> c_int,
    >,
    pub transfer_data_from: Option<
        unsafe extern "C" fn(*mut FramesCtx, *mut ffi::AVFrame, *const ffi::AVFrame) -> c_int,
    >,
    pub map_to: Option<
        unsafe extern "C" fn(
            *mut FramesCtx,
            *mut ffi::AVFrame,
            *const ffi::AVFrame,
            c_int,
        ) -> c_int,
    >,
    pub map_from: Option<
        unsafe extern "C" fn(
            *mut FramesCtx,
            *mut ffi::AVFrame,
            *const ffi::AVFrame,
            c_int,
        ) -> c_int,
    >,
    pub frames_derive_to:
        Option<unsafe extern "C" fn(*mut FramesCtx, *mut FramesCtx, c_int) -> c_int>,
    pub frames_derive_from:
        Option<unsafe extern "C" fn(*mut FramesCtx, *mut FramesCtx, c_int) -> c_int>,
}

/// hwcontext.c's private `FFHWDeviceContext`.
#[repr(C)]
pub struct DeviceInternal {
    pub p: DeviceCtx,
    pub hw_type: *const HwContextType,
    pub source_device: *mut ffi::AVBufferRef,
}

/// hwcontext_internal.h's `FFHWFramesContext`.
#[repr(C)]
pub struct FramesInternal {
    pub p: FramesCtx,
    pub hw_type: *const HwContextType,
    pub pool_internal: *mut ffi::AVBufferPool,
    pub source_frames: *mut ffi::AVBufferRef,
    pub source_allocation_map_flags: c_int,
}

/// hwcontext_internal.h's `HWMapDescriptor`, the data of a mapped frame's
/// first buffer.
#[repr(C)]
pub struct MapDescriptor {
    pub source: *mut ffi::AVFrame,
    pub hw_frames_ctx: *mut ffi::AVBufferRef,
    pub unmap: Option<unsafe extern "C" fn(*mut FramesCtx, *mut MapDescriptor)>,
    pub priv_: *mut c_void,
}

/// A device context's `hwctx`: what its backend returns.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DeviceHwctx {
    /// `device_init`'s result.
    pub init_ret: c_int,
}

/// A frames context's `hwctx`: what its backend returns.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct FramesHwctx {
    /// `frames_init`'s result.
    pub init_ret: c_int,
    /// `frames_get_buffer`'s result, when negative.
    pub get_buffer_ret: c_int,
    /// When negative, kind A's `map_from` result for frames of this context,
    /// and kind B's `map_to` result for mappings into it.
    pub map_ret: c_int,
    /// Kind A's `frames_derive_from` result when this context is the
    /// source, and kind B's `frames_derive_to` when it is the target.
    pub derive_ret: c_int,
    /// Nonzero for a `frames_init` that leaves the context without a pool.
    pub no_pool: c_int,
    /// When negative, kind A's transfer result for frames of this context.
    pub transfer_ret: c_int,
}

/// The bytes of a surface.
pub const SURFACE_SIZE: usize = 64;
/// The bytes of a backend's hardware configuration, which only kind A has.
pub const HWCONFIG_SIZE: usize = 24;

/// One of the two test backends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    A,
    B,
}

impl Kind {
    /// The backend's `HWContextType`.
    pub fn backend(self) -> *const HwContextType {
        match self {
            Kind::A => &raw const BACKEND_A.0,
            Kind::B => &raw const BACKEND_B.0,
        }
    }

    /// The device type the backend implements.
    pub fn device_type(self) -> ffi::AVHWDeviceType {
        match self {
            Kind::A => ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_VAAPI,
            Kind::B => ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_DRM,
        }
    }

    /// The backend's hardware pixel format.
    pub fn format(self) -> ffi::AVPixelFormat {
        match self {
            Kind::A => ffi::AVPixelFormat::AV_PIX_FMT_VAAPI,
            Kind::B => ffi::AVPixelFormat::AV_PIX_FMT_DRM_PRIME,
        }
    }

    /// The backend of `hw_type`.
    fn of(hw_type: *const HwContextType) -> Kind {
        if ptr::eq(hw_type, Kind::A.backend()) {
            Kind::A
        } else {
            assert!(ptr::eq(hw_type, Kind::B.backend()), "not a test backend");
            Kind::B
        }
    }
}

/// What C asked of a backend, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    DeviceInit(Kind),
    DeviceUninit(Kind),
    DeviceFreed(Kind),
    FramesInit(Kind),
    FramesUninit(Kind),
    GetBuffer(Kind),
    MapFrom(Kind),
    MapTo(Kind),
    Unmap(Kind),
    DeriveFrom(Kind),
    DeriveTo(Kind),
    /// A transfer by the backend of the frames context it ran in.
    Transfer(Kind),
}

thread_local! {
    static EVENTS: RefCell<Vec<Event>> = const { RefCell::new(Vec::new()) };
}

fn record(event: Event) {
    EVENTS.with(|e| e.borrow_mut().push(event));
}

/// The events recorded on this thread since the last call.
pub fn take_events() -> Vec<Event> {
    EVENTS.with(|e| e.take())
}

/// `AVERROR(e)`.
pub const fn averror(e: u32) -> c_int {
    -(e as c_int)
}

/// A static the backends point at, which only C reads.
#[repr(transparent)]
pub struct Shared<T>(pub T);
// SAFETY: every `Shared` static is immutable data and C routines that
// read it.
unsafe impl<T> Sync for Shared<T> {}

static FORMATS_A: [ffi::AVPixelFormat; 2] = [
    ffi::AVPixelFormat::AV_PIX_FMT_VAAPI,
    ffi::AVPixelFormat::AV_PIX_FMT_NONE,
];
static FORMATS_B: [ffi::AVPixelFormat; 2] = [
    ffi::AVPixelFormat::AV_PIX_FMT_DRM_PRIME,
    ffi::AVPixelFormat::AV_PIX_FMT_NONE,
];

/// A class for the test device contexts, as `hwdevice_ctx_class` is for
/// C's.
pub static DEVICE_CLASS: Shared<ffi::AVClass> = Shared(ffi::AVClass {
    class_name: c"AVHWDeviceContext".as_ptr(),
    item_name: Some(ffi::av_default_item_name),
    // SAFETY: the rest of an `AVClass` (options, version, callbacks) may be
    // zero, as for a class with nothing but a name.
    ..unsafe { zeroed() }
});

pub static BACKEND_A: Shared<HwContextType> = Shared(HwContextType {
    type_: ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_VAAPI,
    name: c"test-a".as_ptr(),
    pix_fmts: (&raw const FORMATS_A).cast(),
    device_hwctx_size: size_of::<DeviceHwctx>(),
    device_hwconfig_size: HWCONFIG_SIZE,
    frames_hwctx_size: size_of::<FramesHwctx>(),
    device_create: None,
    device_derive: None,
    device_init: Some(device_init),
    device_uninit: Some(device_uninit),
    frames_get_constraints: None,
    frames_init: Some(frames_init),
    frames_uninit: Some(frames_uninit),
    frames_get_buffer: Some(frames_get_buffer),
    transfer_get_formats: Some(transfer_get_formats),
    transfer_data_to: Some(transfer),
    transfer_data_from: Some(transfer),
    map_to: None,
    map_from: Some(map_from),
    frames_derive_to: None,
    frames_derive_from: Some(frames_derive_from),
});

pub static BACKEND_B: Shared<HwContextType> = Shared(HwContextType {
    type_: ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_DRM,
    name: c"test-b".as_ptr(),
    pix_fmts: (&raw const FORMATS_B).cast(),
    device_hwctx_size: size_of::<DeviceHwctx>(),
    device_hwconfig_size: 0,
    frames_hwctx_size: size_of::<FramesHwctx>(),
    device_create: None,
    device_derive: None,
    device_init: Some(device_init),
    device_uninit: Some(device_uninit),
    frames_get_constraints: None,
    frames_init: Some(frames_init),
    frames_uninit: Some(frames_uninit),
    frames_get_buffer: Some(frames_get_buffer),
    transfer_get_formats: None,
    transfer_data_to: None,
    transfer_data_from: None,
    map_to: Some(map_to),
    map_from: None,
    frames_derive_to: Some(frames_derive_to),
    frames_derive_from: None,
});

/// The backend of the device context `ctx`.
///
/// # Safety
///
/// `ctx` must be a live test device context.
unsafe fn device_kind(ctx: *const DeviceCtx) -> Kind {
    // SAFETY: the public struct starts the private one, by the caller's
    // contract.
    Kind::of(unsafe { (*ctx.cast::<DeviceInternal>()).hw_type })
}

/// The backend of the frames context `ctx`.
///
/// # Safety
///
/// `ctx` must be a live frames context on a test device.
unsafe fn frames_kind(ctx: *const FramesCtx) -> Kind {
    // SAFETY: the public struct starts the private one, by the caller's
    // contract.
    Kind::of(unsafe { (*ctx.cast::<FramesInternal>()).hw_type })
}

/// The `hwctx` of the frames context `ctx`.
///
/// # Safety
///
/// `ctx` must be a live frames context on a test device.
unsafe fn frames_hwctx(ctx: *const FramesCtx) -> FramesHwctx {
    // SAFETY: C allocated the test backends' `frames_hwctx_size` for it.
    unsafe { (*ctx).hwctx.cast::<FramesHwctx>().read() }
}

unsafe extern "C" fn device_init(ctx: *mut DeviceCtx) -> c_int {
    // SAFETY: C passes the live test device context it initialises.
    unsafe {
        record(Event::DeviceInit(device_kind(ctx)));
        (*ctx).hwctx.cast::<DeviceHwctx>().read().init_ret
    }
}

unsafe extern "C" fn device_uninit(ctx: *mut DeviceCtx) {
    // SAFETY: C passes the live test device context it releases.
    record(Event::DeviceUninit(unsafe { device_kind(ctx) }));
}

unsafe extern "C" fn frames_init(ctx: *mut FramesCtx) -> c_int {
    // SAFETY: C passes the live frames context it initialises, whose
    // private part C allocated; the pool is the backend's to set, and C
    // uninits it with the context.
    unsafe {
        record(Event::FramesInit(frames_kind(ctx)));
        let ctxi = ctx.cast::<FramesInternal>();
        if (*ctxi).pool_internal.is_null() && frames_hwctx(ctx).no_pool == 0 {
            (*ctxi).pool_internal =
                ffi::av_buffer_pool_init(SURFACE_SIZE, Some(ffi::av_buffer_allocz));
        }
        frames_hwctx(ctx).init_ret
    }
}

unsafe extern "C" fn frames_uninit(ctx: *mut FramesCtx) {
    // SAFETY: C passes the live frames context it releases.
    record(Event::FramesUninit(unsafe { frames_kind(ctx) }));
}

unsafe extern "C" fn frames_get_buffer(ctx: *mut FramesCtx, frame: *mut ffi::AVFrame) -> c_int {
    // SAFETY: C passes a live, initialised frames context with a pool and
    // the frame it fills, which holds no buffer yet.
    unsafe {
        record(Event::GetBuffer(frames_kind(ctx)));
        let ret = frames_hwctx(ctx).get_buffer_ret;
        if ret < 0 {
            return ret;
        }
        let buf = ffi::av_buffer_pool_get((*ctx).pool);
        if buf.is_null() {
            return averror(ffi::ENOMEM);
        }
        (*frame).buf[0] = buf;
        (*frame).data[3] = (*buf).data;
        (*frame).format = (*ctx).format.0;
        (*frame).width = (*ctx).width;
        (*frame).height = (*ctx).height;
        0
    }
}

unsafe extern "C" fn unmap(ctx: *mut FramesCtx, _hwmap: *mut MapDescriptor) {
    // SAFETY: C passes the live frames context the mapping was made in.
    record(Event::Unmap(unsafe { frames_kind(ctx) }));
}

/// `ff_hwframe_unmap`, the release of a mapping's buffer.
unsafe extern "C" fn release_mapping(opaque: *mut c_void, data: *mut u8) {
    // SAFETY: `data` is the descriptor `map_frame` allocated, and `opaque`
    // the frames context it holds a reference to.
    unsafe {
        let hwmap = data.cast::<MapDescriptor>();
        if let Some(f) = (*hwmap).unmap {
            f(opaque.cast(), hwmap);
        }
        ffi::av_frame_free(addr_of_mut!((*hwmap).source));
        ffi::av_buffer_unref(addr_of_mut!((*hwmap).hw_frames_ctx));
        ffi::av_free(hwmap.cast());
    }
}

/// `ff_hwframe_map_create` (which libavutil does not export): makes `dst` a
/// mapping of `src` in the frames context `hwframe_ref`, holding a reference
/// to `src` and to the context, and points it at `src`'s surface.
///
/// # Safety
///
/// As for `ff_hwframe_map_create`.
unsafe fn map_frame(
    hwframe_ref: *mut ffi::AVBufferRef,
    dst: *mut ffi::AVFrame,
    src: *const ffi::AVFrame,
) -> c_int {
    // SAFETY: as `ff_hwframe_map_create`, step by step, by the caller's
    // contract.
    unsafe {
        let ctx = (*hwframe_ref).data.cast::<FramesCtx>();
        let hwmap = ffi::av_mallocz(size_of::<MapDescriptor>()).cast::<MapDescriptor>();
        if hwmap.is_null() {
            return averror(ffi::ENOMEM);
        }
        let mut ret = averror(ffi::ENOMEM);
        (*hwmap).source = ffi::av_frame_alloc();
        if !(*hwmap).source.is_null() {
            ret = ffi::av_frame_ref((*hwmap).source, src);
            if ret >= 0 {
                (*hwmap).hw_frames_ctx = ffi::av_buffer_ref(hwframe_ref);
                ret = averror(ffi::ENOMEM);
                if !(*hwmap).hw_frames_ctx.is_null() {
                    (*hwmap).unmap = Some(unmap);
                    (*dst).buf[0] = ffi::av_buffer_create(
                        hwmap.cast(),
                        size_of::<MapDescriptor>(),
                        Some(release_mapping),
                        ctx.cast(),
                        0,
                    );
                    if !(*dst).buf[0].is_null() {
                        (*dst).data[3] = (*src).data[3];
                        (*dst).width = (*src).width;
                        (*dst).height = (*src).height;
                        return 0;
                    }
                }
            }
        }
        ffi::av_buffer_unref(addr_of_mut!((*hwmap).hw_frames_ctx));
        ffi::av_frame_free(addr_of_mut!((*hwmap).source));
        ffi::av_free(hwmap.cast());
        ret
    }
}

/// Kind A's `map_from`: maps a surface to a software frame of the context's
/// software format (any other format is `ENOSYS`), or into the frames context
/// `dst` holds.
unsafe extern "C" fn map_from(
    ctx: *mut FramesCtx,
    dst: *mut ffi::AVFrame,
    src: *const ffi::AVFrame,
    _flags: c_int,
) -> c_int {
    // SAFETY: C passes the source's live frames context, a blank `dst` and a
    // hardware frame of that context.
    unsafe {
        record(Event::MapFrom(frames_kind(ctx)));
        let ret = frames_hwctx(ctx).map_ret;
        if ret < 0 {
            return ret;
        }
        if (*dst).hw_frames_ctx.is_null() {
            let sw = (*ctx).sw_format.0;
            if (*dst).format == ffi::AVPixelFormat::AV_PIX_FMT_NONE.0 {
                (*dst).format = sw;
            } else if (*dst).format != sw {
                return averror(ffi::ENOSYS);
            }
            let ret = map_frame((*src).hw_frames_ctx, dst, src);
            if ret >= 0 {
                (*dst).data[0] = (*dst).data[3];
                (*dst).data[3] = null_mut();
                (*dst).linesize[0] = SURFACE_SIZE as c_int;
            }
            ret
        } else {
            map_frame((*dst).hw_frames_ctx, dst, src)
        }
    }
}

/// Kind B's `map_to`: maps a frame of another context into the frames
/// context `dst` holds.
unsafe extern "C" fn map_to(
    ctx: *mut FramesCtx,
    dst: *mut ffi::AVFrame,
    src: *const ffi::AVFrame,
    _flags: c_int,
) -> c_int {
    // SAFETY: C passes `dst`'s live frames context, `dst` holding it and a
    // hardware frame of another context.
    unsafe {
        record(Event::MapTo(frames_kind(ctx)));
        let ret = frames_hwctx(ctx).map_ret;
        if ret < 0 {
            return ret;
        }
        if (*src).hw_frames_ctx.is_null() {
            return averror(ffi::ENOSYS);
        }
        map_frame((*dst).hw_frames_ctx, dst, src)
    }
}

/// Kind A's `transfer_get_formats`: the context's software format, for
/// either direction.
unsafe extern "C" fn transfer_get_formats(
    ctx: *mut FramesCtx,
    _dir: ffi::AVHWFrameTransferDirection,
    formats: *mut *mut ffi::AVPixelFormat,
) -> c_int {
    // SAFETY: C passes a live, initialised frames context and a writable
    // slot, to which a NONE-terminated `av_malloc` list is handed over.
    unsafe {
        let list =
            ffi::av_malloc_array(2, size_of::<ffi::AVPixelFormat>()).cast::<ffi::AVPixelFormat>();
        if list.is_null() {
            return averror(ffi::ENOMEM);
        }
        list.write((*ctx).sw_format);
        list.add(1).write(ffi::AVPixelFormat::AV_PIX_FMT_NONE);
        formats.write(list);
        0
    }
}

/// Kind A's `transfer_data_to` and `transfer_data_from`. Between two
/// hardware frames it copies the source's surface over the destination's.
/// Otherwise, for each plane of the context's software format, it runs over
/// the rows of an image of the context's allocated size in the software
/// frame, at its line size, unchecked, as CUDA's transfers trust the frame:
/// a download writes them with the surface's bytes, read cyclically; an
/// upload adds them into the surface's bytes, cyclically.
unsafe extern "C" fn transfer(
    ctx: *mut FramesCtx,
    dst: *mut ffi::AVFrame,
    src: *const ffi::AVFrame,
) -> c_int {
    // SAFETY: C passes the live frames context of the hardware side, whose
    // frames' `data[3]` are surfaces, and the frames; the software side is
    // trusted to hold the image, as the wrappers check.
    unsafe {
        record(Event::Transfer(frames_kind(ctx)));
        let ret = frames_hwctx(ctx).transfer_ret;
        if ret < 0 {
            return ret;
        }
        let (dst_hw, src_hw) = (
            !(*dst).hw_frames_ctx.is_null(),
            !(*src).hw_frames_ctx.is_null(),
        );
        if dst_hw && src_hw {
            ptr::copy((*src).data[3], (*dst).data[3], SURFACE_SIZE);
            return 0;
        }
        let (sw, surface) = if src_hw {
            (dst.cast_const(), (*src).data[3])
        } else {
            (src, (*dst).data[3])
        };
        let format = (*ctx).sw_format;
        let desc = ffi::av_pix_fmt_desc_get(format);
        let mut k = 0;
        for i in 0..ffi::av_pix_fmt_count_planes(format) {
            let len = ffi::av_image_get_linesize(format, (*ctx).width, i) as usize;
            let shift = if i == 1 || i == 2 {
                (*desc).log2_chroma_h
            } else {
                0
            };
            let rows = -((-(*ctx).height) >> shift);
            let line = (*sw).linesize[i as usize] as isize;
            for row in 0..rows as isize {
                let p = (*sw).data[i as usize].offset(row * line);
                for x in 0..len {
                    let s = surface.add(k % SURFACE_SIZE);
                    if src_hw {
                        p.add(x).write(s.read());
                    } else {
                        s.write(s.read().wrapping_add(p.add(x).read()));
                    }
                    k += 1;
                }
            }
        }
        0
    }
}

unsafe extern "C" fn frames_derive_from(
    _dst: *mut FramesCtx,
    src: *mut FramesCtx,
    _flags: c_int,
) -> c_int {
    // SAFETY: C passes the live derived context and its live source.
    unsafe {
        record(Event::DeriveFrom(frames_kind(src)));
        frames_hwctx(src).derive_ret
    }
}

unsafe extern "C" fn frames_derive_to(
    dst: *mut FramesCtx,
    _src: *mut FramesCtx,
    _flags: c_int,
) -> c_int {
    // SAFETY: C passes the live derived context.
    unsafe {
        record(Event::DeriveTo(frames_kind(dst)));
        frames_hwctx(dst).derive_ret
    }
}

/// `hwdevice_ctx_free`, the release of a test device context.
unsafe extern "C" fn free_device(_opaque: *mut c_void, data: *mut u8) {
    // SAFETY: `data` is the private context `alloc_device` allocated, at its
    // last reference.
    unsafe {
        let ctxi = data.cast::<DeviceInternal>();
        let ctx = addr_of_mut!((*ctxi).p);
        let kind = Kind::of((*ctxi).hw_type);
        if let Some(uninit) = (*(*ctxi).hw_type).device_uninit {
            uninit(ctx);
        }
        if let Some(free) = (*ctx).free {
            free(ctx);
        }
        ffi::av_buffer_unref(addr_of_mut!((*ctxi).source_device));
        ffi::av_free((*ctx).hwctx);
        ffi::av_free(ctxi.cast());
        record(Event::DeviceFreed(kind));
    }
}

/// A new, uninitialised device context of backend `kind`, as
/// `av_hwdevice_ctx_alloc` builds one: the only reference to it, its `hwctx`
/// a zeroed [`DeviceHwctx`]. Panics when out of memory.
pub fn alloc_device(kind: Kind) -> *mut ffi::AVBufferRef {
    // SAFETY: `av_hwdevice_ctx_alloc`'s steps over zeroed allocations.
    unsafe {
        let ctxi = ffi::av_mallocz(size_of::<DeviceInternal>()).cast::<DeviceInternal>();
        assert!(!ctxi.is_null());
        let ctx = addr_of_mut!((*ctxi).p);
        (*ctx).hwctx = ffi::av_mallocz(size_of::<DeviceHwctx>());
        assert!(!(*ctx).hwctx.is_null());
        let buf = ffi::av_buffer_create(
            ctxi.cast(),
            size_of::<DeviceCtx>(),
            Some(free_device),
            null_mut(),
            ffi::AV_BUFFER_FLAG_READONLY as c_int,
        );
        assert!(!buf.is_null());
        (*ctx).type_ = kind.device_type();
        (*ctx).av_class = &raw const DEVICE_CLASS.0;
        (*ctxi).hw_type = kind.backend();
        buf
    }
}

/// [`alloc_device`] initialised through C (`av_hwdevice_ctx_init`), whose
/// event it takes from the log.
pub fn device(kind: Kind) -> *mut ffi::AVBufferRef {
    let buf = alloc_device(kind);
    // SAFETY: a fresh test device context.
    assert_eq!(unsafe { ffi::av_hwdevice_ctx_init(buf) }, 0);
    assert_eq!(take_events(), [Event::DeviceInit(kind)]);
    buf
}

/// The `hwctx` of the test device context `buf` refers to, for configuring
/// its backend before `av_hwdevice_ctx_init`.
///
/// # Safety
///
/// `buf` must be a live reference to a test device context.
pub unsafe fn device_hwctx(buf: *mut ffi::AVBufferRef) -> *mut DeviceHwctx {
    // SAFETY: by the caller's contract.
    unsafe { (*(*buf).data.cast::<DeviceCtx>()).hwctx.cast() }
}

/// Makes the test device context `buf` refers to derived from `source`, as
/// `av_hwdevice_ctx_create_derived_opts` records it: a reference to `source`
/// in its private `source_device`.
///
/// # Safety
///
/// `buf` and `source` must be live references to test device contexts, `buf`
/// not derived yet.
pub unsafe fn set_source_device(buf: *mut ffi::AVBufferRef, source: *mut ffi::AVBufferRef) {
    // SAFETY: by the caller's contract.
    unsafe {
        let ctxi = (*buf).data.cast::<DeviceInternal>();
        assert!((*ctxi).source_device.is_null());
        (*ctxi).source_device = ffi::av_buffer_ref(source);
        assert!(!(*ctxi).source_device.is_null());
    }
}

/// The frames context a frames reference `buf` refers to.
///
/// # Safety
///
/// `buf` must be a live frames context reference.
pub unsafe fn frames_ctx(buf: *const ffi::AVBufferRef) -> *mut FramesCtx {
    // SAFETY: by the caller's contract.
    unsafe { (*buf).data.cast() }
}

/// Fills the uninitialised frames context `ctx` on a device of backend
/// `kind` for `av_hwframe_ctx_init`: the backend's format, NV12 as the
/// software format, `width` x `height`, `pool_size` frames preallocated.
///
/// # Safety
///
/// `ctx` must be a live, uninitialised frames context no one else reaches.
pub unsafe fn fill_frames(
    ctx: *mut FramesCtx,
    kind: Kind,
    width: c_int,
    height: c_int,
    pool_size: c_int,
) {
    // SAFETY: by the caller's contract.
    unsafe {
        (*ctx).format = kind.format();
        (*ctx).sw_format = ffi::AVPixelFormat::AV_PIX_FMT_NV12;
        (*ctx).width = width;
        (*ctx).height = height;
        (*ctx).initial_pool_size = pool_size;
    }
}

/// Sets the backend results of the frames context `ctx`.
///
/// # Safety
///
/// `ctx` must be a live frames context on a test device, which no one reads
/// meanwhile.
pub unsafe fn set_frames_hwctx(ctx: *mut FramesCtx, hwctx: FramesHwctx) {
    // SAFETY: by the caller's contract; the backends' `frames_hwctx_size` is
    // that of `FramesHwctx`.
    unsafe { (*ctx).hwctx.cast::<FramesHwctx>().write(hwctx) }
}

/// A frames context of `width` x `height` on `device` (a test device of
/// backend `kind`), initialised through C, its events taken from the log.
///
/// # Safety
///
/// `device` must be a live reference to an initialised test device context
/// of backend `kind`.
pub unsafe fn frames(
    device: *mut ffi::AVBufferRef,
    kind: Kind,
    width: c_int,
    height: c_int,
) -> *mut ffi::AVBufferRef {
    // SAFETY: by the caller's contract; then a fresh frames context.
    unsafe {
        let buf = ffi::av_hwframe_ctx_alloc(device);
        assert!(!buf.is_null());
        fill_frames(frames_ctx(buf), kind, width, height, 0);
        assert_eq!(ffi::av_hwframe_ctx_init(buf), 0);
        assert_eq!(take_events(), [Event::FramesInit(kind)]);
        buf
    }
}

/// The count of references to `buf`'s buffer.
///
/// # Safety
///
/// `buf` must be a live reference.
pub unsafe fn ref_count(buf: *const ffi::AVBufferRef) -> c_int {
    // SAFETY: by the caller's contract.
    unsafe { ffi::av_buffer_get_ref_count(buf) }
}

/// Releases the reference `buf`, if any.
///
/// # Safety
///
/// `buf` must be NULL or a live reference the caller owns.
pub unsafe fn unref(mut buf: *mut ffi::AVBufferRef) {
    // SAFETY: by the caller's contract.
    unsafe { ffi::av_buffer_unref(&mut buf) }
}

/// What a frame shows of itself: its format, size, which of `buf`/`data`
/// are set, whether `data[3]` and `data[0]` point at its first buffer's
/// surface, its frames context, and whether `extended_data` is `data`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameShape {
    pub format: c_int,
    pub width: c_int,
    pub height: c_int,
    pub bufs: [bool; 8],
    pub data: [bool; 8],
    pub linesize0: c_int,
    pub has_frames: bool,
    pub own_extended: bool,
}

/// The shape of `frame`.
///
/// # Safety
///
/// `frame` must be a live frame.
pub unsafe fn shape(frame: *const ffi::AVFrame) -> FrameShape {
    // SAFETY: reads of a live frame's fields.
    unsafe {
        FrameShape {
            format: (*frame).format,
            width: (*frame).width,
            height: (*frame).height,
            bufs: (*frame).buf.map(|b| !b.is_null()),
            data: (*frame).data.map(|d| !d.is_null()),
            linesize0: (*frame).linesize[0],
            has_frames: !(*frame).hw_frames_ctx.is_null(),
            own_extended: ptr::eq(
                (*frame).extended_data.cast_const(),
                addr_of!((*frame).data).cast(),
            ),
        }
    }
}
