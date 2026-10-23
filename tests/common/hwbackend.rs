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
//! (`map_from`), and transfers data to and from them ([`transfer`]); kind B (`DRM`, hardware format
//! `DRM_PRIME`) maps frames of another context into its own (`map_to`) and derives frames contexts
//! (`frames_derive_to`), and transfers nothing. A surface is a
//! [`SURFACE_SIZE`]-byte pool buffer, zeroed, which `data[3]` points at, as
//! VA-API's surface IDs sit there.
#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_ptr_alignment,
    clippy::cast_sign_loss,
    reason = "test inputs are chosen within range, and the casts mirror the C conversions under comparison"
)]
#![expect(
    clippy::redundant_closure_for_method_calls,
    reason = "each test is one scenario, its helpers and steps kept where they are used"
)]

use core::cell::RefCell;
use core::ffi::{c_char, c_int, c_void};
use core::mem::{size_of, zeroed};
use core::ptr::{self, null_mut};

use ffibox::define_ctype;

use super::ffi;

type DeviceCtx = ffi::AVHWDeviceContext;
type FramesCtx = ffi::AVHWFramesContext;

/// `hwcontext_internal.h`'s `HWContextType`, the backend's routines and sizes.
#[repr(C)]
pub(crate) struct HwContextType {
    pub(crate) type_: ffi::AVHWDeviceType,
    pub(crate) name: *const c_char,
    pub(crate) pix_fmts: *const ffi::AVPixelFormat,
    pub(crate) device_hwctx_size: usize,
    pub(crate) device_hwconfig_size: usize,
    pub(crate) frames_hwctx_size: usize,
    pub(crate) device_create: Option<
        unsafe extern "C" fn(*mut DeviceCtx, *const c_char, *mut ffi::AVDictionary, c_int) -> c_int,
    >,
    pub(crate) device_derive: Option<
        unsafe extern "C" fn(
            *mut DeviceCtx,
            *mut DeviceCtx,
            *mut ffi::AVDictionary,
            c_int,
        ) -> c_int,
    >,
    pub(crate) device_init: Option<unsafe extern "C" fn(*mut DeviceCtx) -> c_int>,
    pub(crate) device_uninit: Option<unsafe extern "C" fn(*mut DeviceCtx)>,
    pub(crate) frames_get_constraints: Option<
        unsafe extern "C" fn(
            *mut DeviceCtx,
            *const c_void,
            *mut ffi::AVHWFramesConstraints,
        ) -> c_int,
    >,
    pub(crate) frames_init: Option<unsafe extern "C" fn(*mut FramesCtx) -> c_int>,
    pub(crate) frames_uninit: Option<unsafe extern "C" fn(*mut FramesCtx)>,
    pub(crate) frames_get_buffer:
        Option<unsafe extern "C" fn(*mut FramesCtx, *mut ffi::AVFrame) -> c_int>,
    pub(crate) transfer_get_formats: Option<
        unsafe extern "C" fn(
            *mut FramesCtx,
            ffi::AVHWFrameTransferDirection,
            *mut *mut ffi::AVPixelFormat,
        ) -> c_int,
    >,
    pub(crate) transfer_data_to: Option<
        unsafe extern "C" fn(*mut FramesCtx, *mut ffi::AVFrame, *const ffi::AVFrame) -> c_int,
    >,
    pub(crate) transfer_data_from: Option<
        unsafe extern "C" fn(*mut FramesCtx, *mut ffi::AVFrame, *const ffi::AVFrame) -> c_int,
    >,
    pub(crate) map_to: Option<
        unsafe extern "C" fn(
            *mut FramesCtx,
            *mut ffi::AVFrame,
            *const ffi::AVFrame,
            c_int,
        ) -> c_int,
    >,
    pub(crate) map_from: Option<
        unsafe extern "C" fn(
            *mut FramesCtx,
            *mut ffi::AVFrame,
            *const ffi::AVFrame,
            c_int,
        ) -> c_int,
    >,
    pub(crate) frames_derive_to:
        Option<unsafe extern "C" fn(*mut FramesCtx, *mut FramesCtx, c_int) -> c_int>,
    pub(crate) frames_derive_from:
        Option<unsafe extern "C" fn(*mut FramesCtx, *mut FramesCtx, c_int) -> c_int>,
}

/// hwcontext.c's private `FFHWDeviceContext`.
#[repr(C)]
pub(crate) struct DeviceInternal {
    pub(crate) p: DeviceCtx,
    pub(crate) hw_type: *const HwContextType,
    pub(crate) source_device: *mut ffi::AVBufferRef,
}

/// `hwcontext_internal.h`'s `FFHWFramesContext`.
#[repr(C)]
pub(crate) struct FramesInternal {
    pub(crate) p: FramesCtx,
    pub(crate) hw_type: *const HwContextType,
    pub(crate) pool_internal: *mut ffi::AVBufferPool,
    pub(crate) source_frames: *mut ffi::AVBufferRef,
    pub(crate) source_allocation_map_flags: c_int,
}

/// `hwcontext_internal.h`'s `HWMapDescriptor`, the data of a mapped frame's
/// first buffer.
#[repr(C)]
pub(crate) struct MapDescriptor {
    pub(crate) source: *mut ffi::AVFrame,
    pub(crate) hw_frames_ctx: *mut ffi::AVBufferRef,
    pub(crate) unmap: Option<unsafe extern "C" fn(*mut FramesCtx, *mut MapDescriptor)>,
    pub(crate) priv_: *mut c_void,
}

/// A device context's `hwctx`: what its backend returns.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct DeviceHwctx {
    /// `device_init`'s result.
    pub(crate) init_ret: c_int,
}

/// A frames context's `hwctx`: what its backend returns.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
#[expect(
    unreachable_pub,
    reason = "`define_ctype!` publishes a newtype over it, which needs it as visible"
)]
pub struct FramesHwctx {
    /// `frames_init`'s result.
    pub(crate) init_ret: c_int,
    /// `frames_get_buffer`'s result, when negative.
    pub(crate) get_buffer_ret: c_int,
    /// When negative, kind A's `map_from` result for frames of this context,
    /// and kind B's `map_to` result for mappings into it.
    pub(crate) map_ret: c_int,
    /// Kind A's `frames_derive_from` result when this context is the
    /// source, and kind B's `frames_derive_to` when it is the target.
    pub(crate) derive_ret: c_int,
    /// Nonzero for a `frames_init` that leaves the context without a pool.
    pub(crate) no_pool: c_int,
    /// When negative, kind A's transfer result for frames of this context.
    pub(crate) transfer_ret: c_int,
}

define_ctype!(
    /// [`FramesHwctx`], the test backends' frames context, as the frames
    /// context wrappers type `hwctx`.
    FramesHw,
    FramesHwRef,
    FramesHwMut,
    FramesHwctx
);

impl FramesHwRef<'_> {
    /// The backend results the frames context holds.
    pub(crate) fn get(&self) -> FramesHwctx {
        // SAFETY: the live frames struct the handle borrows.
        unsafe { self.as_ptr().read() }
    }
}

impl FramesHwMut<'_> {
    /// Sets the backend results of the frames context.
    pub(crate) fn set(&mut self, hwctx: FramesHwctx) {
        // SAFETY: the live frames struct the handle borrows exclusively.
        unsafe { self.as_mut_ptr().write(hwctx) }
    }
}

/// The bytes of a surface.
pub(crate) const SURFACE_SIZE: usize = 64;
/// The bytes of a backend's hardware configuration, which only kind A has.
pub(crate) const HWCONFIG_SIZE: usize = 24;

/// One of the two test backends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    A,
    B,
}

impl Kind {
    /// The backend's `HWContextType`.
    pub(crate) fn backend(self) -> *const HwContextType {
        match self {
            Kind::A => &raw const BACKEND_A.0,
            Kind::B => &raw const BACKEND_B.0,
        }
    }

    /// The device type the backend implements.
    pub(crate) fn device_type(self) -> ffi::AVHWDeviceType {
        match self {
            Kind::A => ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_VAAPI,
            Kind::B => ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_DRM,
        }
    }

    /// The backend's hardware pixel format.
    pub(crate) fn format(self) -> ffi::AVPixelFormat {
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
pub(crate) enum Event {
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
pub(crate) fn take_events() -> Vec<Event> {
    EVENTS.with(|e| e.take())
}

/// `AVERROR(e)`.
pub(crate) const fn averror(e: u32) -> c_int {
    -(e as c_int)
}

/// A static the backends point at, which only C reads.
#[repr(transparent)]
pub(crate) struct Shared<T>(pub T);
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
pub(crate) static DEVICE_CLASS: Shared<ffi::AVClass> = Shared(ffi::AVClass {
    class_name: c"AVHWDeviceContext".as_ptr(),
    item_name: Some(ffi::av_default_item_name),
    // SAFETY: the rest of an `AVClass` (options, version, callbacks) may be
    // zero, as for a class with nothing but a name.
    ..unsafe { zeroed() }
});

pub(crate) static BACKEND_A: Shared<HwContextType> = Shared(HwContextType {
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

pub(crate) static BACKEND_B: Shared<HwContextType> = Shared(HwContextType {
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
    // SAFETY: as the comment below states, for this part of the operation.
    let hwctx_2 = unsafe { (*ctx).hwctx };
    // SAFETY: C allocated the test backends' `frames_hwctx_size` for it.
    unsafe { hwctx_2.cast::<FramesHwctx>().read() }
}

unsafe extern "C" fn device_init(ctx: *mut DeviceCtx) -> c_int {
    // For the unsafe operations below: C passes the live test device context it initialises.
    {
        // SAFETY: as the comment above this block states.
        unsafe { record(Event::DeviceInit(device_kind(ctx))) };
        // SAFETY: as the comment below states, for this part of the operation.
        let hwctx_2 = unsafe { (*ctx).hwctx };
        // SAFETY: as the comment above this block states.
        unsafe { hwctx_2.cast::<DeviceHwctx>().read().init_ret }
    }
}

unsafe extern "C" fn device_uninit(ctx: *mut DeviceCtx) {
    // SAFETY: C passes the live test device context it releases.
    record(Event::DeviceUninit(unsafe { device_kind(ctx) }));
}

unsafe extern "C" fn frames_init(ctx: *mut FramesCtx) -> c_int {
    // C passes the live frames context it initialises, whose private part C allocated; the pool
    // is the backend's to set, and C uninits it with the context.
    // SAFETY: as above.
    record(Event::FramesInit(unsafe { frames_kind(ctx) }));
    let ctxi = ctx.cast::<FramesInternal>();
    // SAFETY: as above: the context's backend configuration.
    let hwctx = unsafe { frames_hwctx(ctx) };
    // SAFETY: as above: a read of the private pool.
    if unsafe { (*ctxi).pool_internal }.is_null() && hwctx.no_pool == 0 {
        // SAFETY: no precondition; NULL when C cannot allocate.
        let pool = unsafe { ffi::av_buffer_pool_init(SURFACE_SIZE, Some(ffi::av_buffer_allocz)) };
        // SAFETY: as above: the backend sets the private pool.
        unsafe { (*ctxi).pool_internal = pool };
    }
    hwctx.init_ret
}

unsafe extern "C" fn frames_uninit(ctx: *mut FramesCtx) {
    // SAFETY: C passes the live frames context it releases.
    record(Event::FramesUninit(unsafe { frames_kind(ctx) }));
}

unsafe extern "C" fn frames_get_buffer(ctx: *mut FramesCtx, frame: *mut ffi::AVFrame) -> c_int {
    // C passes a live, initialised frames context with a pool and the frame it fills, which holds
    // no buffer yet.
    // SAFETY: as above.
    record(Event::GetBuffer(unsafe { frames_kind(ctx) }));
    // SAFETY: as above: the context's backend configuration.
    let ret = unsafe { frames_hwctx(ctx) }.get_buffer_ret;
    if ret < 0 {
        return ret;
    }
    // SAFETY: as above: reads of the context's pool and geometry.
    let (pool, format, width, height) = (
        unsafe { (*ctx).pool },
        unsafe { (*ctx).format.0 },
        unsafe { (*ctx).width },
        unsafe { (*ctx).height },
    );
    // SAFETY: as above: the context's live pool.
    let buf = unsafe { ffi::av_buffer_pool_get(pool) };
    if buf.is_null() {
        return averror(ffi::ENOMEM);
    }
    // SAFETY: `buf` is the live reference C just handed out: a read of its data.
    let data = unsafe { (*buf).data };
    // SAFETY: as above: the frame takes the reference.
    unsafe { (*frame).buf[0] = buf };
    // SAFETY: as above: its surface.
    unsafe { (*frame).data[3] = data };
    // SAFETY: as above: its format.
    unsafe { (*frame).format = format };
    // SAFETY: as above: its width.
    unsafe { (*frame).width = width };
    // SAFETY: as above: its height.
    unsafe { (*frame).height = height };
    0
}

unsafe extern "C" fn unmap(ctx: *mut FramesCtx, _hwmap: *mut MapDescriptor) {
    // SAFETY: C passes the live frames context the mapping was made in.
    record(Event::Unmap(unsafe { frames_kind(ctx) }));
}

/// `ff_hwframe_unmap`, the release of a mapping's buffer.
unsafe extern "C" fn release_mapping(opaque: *mut c_void, data: *mut u8) {
    // For the unsafe operations below: `data` is the descriptor `map_frame` allocated, and `opaque`
    // the frames context it holds a reference to.
    {
        let hwmap = data.cast::<MapDescriptor>();
        // SAFETY: as the comment above this block states.
        if let Some(f) = unsafe { (*hwmap).unmap } {
            // SAFETY: as the comment above this statement states.
            unsafe { f(opaque.cast(), hwmap) };
        }
        // SAFETY: `hwmap` is to a live object, as the block below relies on, so the
        // place of its `source` field is in bounds; nothing is accessed here.
        let source_ptr = unsafe { &raw mut (*hwmap).source };
        // SAFETY: as the comment above this block states.
        unsafe { ffi::av_frame_free(source_ptr) };
        // SAFETY: `hwmap` is to a live object, as the block below relies on, so the
        // place of its `hw_frames_ctx` field is in bounds; nothing is accessed here.
        let hw_frames_ctx_ptr = unsafe { &raw mut (*hwmap).hw_frames_ctx };
        // SAFETY: as the comment above this block states.
        unsafe { ffi::av_buffer_unref(hw_frames_ctx_ptr) };
        // SAFETY: as the comment above this block states.
        unsafe { ffi::av_free(hwmap.cast()) };
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
    // As `ff_hwframe_map_create`, step by step, by the caller's contract.
    // SAFETY: as above: a read of the live frames reference's data.
    let ctx = unsafe { (*hwframe_ref).data }.cast::<FramesCtx>();
    // SAFETY: no precondition; NULL when C cannot allocate.
    let hwmap = unsafe { ffi::av_mallocz(size_of::<MapDescriptor>()) }.cast::<MapDescriptor>();
    if hwmap.is_null() {
        return averror(ffi::ENOMEM);
    }
    let mut ret = averror(ffi::ENOMEM);
    // SAFETY: no precondition; NULL when C cannot allocate.
    let source = unsafe { ffi::av_frame_alloc() };
    // SAFETY: `hwmap` is the fresh descriptor: it takes the frame.
    unsafe { (*hwmap).source = source };
    if !source.is_null() {
        // SAFETY: a fresh frame and the caller's live source.
        ret = unsafe { ffi::av_frame_ref(source, src) };
        if ret >= 0 {
            // SAFETY: the caller's live frames reference, to which C adds one.
            let frames = unsafe { ffi::av_buffer_ref(hwframe_ref) };
            // SAFETY: as above: the descriptor takes it.
            unsafe { (*hwmap).hw_frames_ctx = frames };
            ret = averror(ffi::ENOMEM);
            if !frames.is_null() {
                // SAFETY: as above: the descriptor's unmap hook.
                unsafe { (*hwmap).unmap = Some(unmap) };
                // SAFETY: the descriptor, which the buffer owns from here, released by
                // `release_mapping` with the context as its opaque.
                let buf = unsafe {
                    ffi::av_buffer_create(
                        hwmap.cast(),
                        size_of::<MapDescriptor>(),
                        Some(release_mapping),
                        ctx.cast(),
                        0,
                    )
                };
                // SAFETY: the caller's blank `dst` takes it.
                unsafe { (*dst).buf[0] = buf };
                if !buf.is_null() {
                    // SAFETY: the caller's live source: reads of its surface and size.
                    let (surface, width, height) =
                        (unsafe { (*src).data[3] }, unsafe { (*src).width }, unsafe {
                            (*src).height
                        });
                    // SAFETY: as above: `dst` points at the source's surface.
                    unsafe { (*dst).data[3] = surface };
                    // SAFETY: as above.
                    unsafe { (*dst).width = width };
                    // SAFETY: as above.
                    unsafe { (*dst).height = height };
                    return 0;
                }
            }
        }
    }
    // SAFETY: the descriptor, owned here: the place of its `hw_frames_ctx`.
    let hw_frames_ctx_ptr = unsafe { &raw mut (*hwmap).hw_frames_ctx };
    // SAFETY: NULL or the reference the descriptor took.
    unsafe { ffi::av_buffer_unref(hw_frames_ctx_ptr) };
    // SAFETY: the descriptor, owned here: the place of its `source`.
    let source_ptr = unsafe { &raw mut (*hwmap).source };
    // SAFETY: NULL or the frame the descriptor took.
    unsafe { ffi::av_frame_free(source_ptr) };
    // SAFETY: the descriptor's `av_mallocz` block, released once.
    unsafe { ffi::av_free(hwmap.cast()) };
    ret
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
    // C passes the source's live frames context, a blank `dst` and a hardware frame of that
    // context.
    // SAFETY: as above.
    record(Event::MapFrom(unsafe { frames_kind(ctx) }));
    // SAFETY: as above: the context's backend configuration.
    let ret = unsafe { frames_hwctx(ctx) }.map_ret;
    if ret < 0 {
        return ret;
    }
    // SAFETY: as above: a read of `dst`'s frames reference.
    let dst_frames = unsafe { (*dst).hw_frames_ctx };
    if !dst_frames.is_null() {
        // SAFETY: as above, mapping into the context `dst` holds.
        return unsafe { map_frame(dst_frames, dst, src) };
    }
    // SAFETY: as above: reads of the software format and `dst`'s format.
    let (sw, format) = (unsafe { (*ctx).sw_format.0 }, unsafe { (*dst).format });
    if format == ffi::AVPixelFormat::AV_PIX_FMT_NONE.0 {
        // SAFETY: as above: `dst` takes the software format.
        unsafe { (*dst).format = sw };
    } else if format != sw {
        return averror(ffi::ENOSYS);
    }
    // SAFETY: as above: a read of the source's frames reference.
    let src_frames = unsafe { (*src).hw_frames_ctx };
    // SAFETY: as above.
    let ret = unsafe { map_frame(src_frames, dst, src) };
    if ret >= 0 {
        // SAFETY: the mapped `dst`: a read of the surface pointer.
        let surface = unsafe { (*dst).data[3] };
        // SAFETY: as above: the surface becomes plane 0 of a software frame.
        unsafe { (*dst).data[0] = surface };
        // SAFETY: as above.
        unsafe { (*dst).data[3] = null_mut() };
        // SAFETY: as above.
        unsafe { (*dst).linesize[0] = SURFACE_SIZE as c_int };
    }
    ret
}

/// Kind B's `map_to`: maps a frame of another context into the frames
/// context `dst` holds.
unsafe extern "C" fn map_to(
    ctx: *mut FramesCtx,
    dst: *mut ffi::AVFrame,
    src: *const ffi::AVFrame,
    _flags: c_int,
) -> c_int {
    // C passes `dst`'s live frames context, `dst` holding it and a hardware frame of another
    // context.
    // SAFETY: as above.
    record(Event::MapTo(unsafe { frames_kind(ctx) }));
    // SAFETY: as above: the context's backend configuration.
    let ret = unsafe { frames_hwctx(ctx) }.map_ret;
    if ret < 0 {
        return ret;
    }
    // SAFETY: as above: a read of the source's frames reference.
    if unsafe { (*src).hw_frames_ctx }.is_null() {
        return averror(ffi::ENOSYS);
    }
    // SAFETY: as above: a read of `dst`'s frames reference.
    let dst_frames = unsafe { (*dst).hw_frames_ctx };
    // SAFETY: as above.
    unsafe { map_frame(dst_frames, dst, src) }
}

/// Kind A's `transfer_get_formats`: the context's software format, for
/// either direction.
unsafe extern "C" fn transfer_get_formats(
    ctx: *mut FramesCtx,
    _dir: ffi::AVHWFrameTransferDirection,
    formats: *mut *mut ffi::AVPixelFormat,
) -> c_int {
    // For the unsafe operations below: C passes a live, initialised frames context and a writable
    // slot, to which a NONE-terminated `av_malloc` list is handed over.
    {
        // SAFETY: as the comment above this block states.
        let list = unsafe {
            ffi::av_malloc_array(2, size_of::<ffi::AVPixelFormat>()).cast::<ffi::AVPixelFormat>()
        };
        if list.is_null() {
            return averror(ffi::ENOMEM);
        }
        // SAFETY: as the comment below states, for this part of the operation.
        let sw_format_2 = unsafe { (*ctx).sw_format };
        // SAFETY: as the comment above this block states.
        unsafe { list.write(sw_format_2) };
        // SAFETY: as the comment below states, for this part of the operation.
        let list_add = unsafe { list.add(1) };
        // SAFETY: as the comment above this block states.
        unsafe { list_add.write(ffi::AVPixelFormat::AV_PIX_FMT_NONE) };
        // SAFETY: as the comment above this block states.
        unsafe { formats.write(list) };
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
    // C passes the live frames context of the hardware side, whose frames' `data[3]` are
    // surfaces, and the frames; the software side is trusted to hold the image, as the wrappers
    // check.
    // SAFETY: as above.
    record(Event::Transfer(unsafe { frames_kind(ctx) }));
    // SAFETY: as above: the context's backend configuration.
    let ret = unsafe { frames_hwctx(ctx) }.transfer_ret;
    if ret < 0 {
        return ret;
    }
    // SAFETY: as above: reads of the two frames' frames references.
    let (dst_hw, src_hw) = (
        !unsafe { (*dst).hw_frames_ctx }.is_null(),
        !unsafe { (*src).hw_frames_ctx }.is_null(),
    );
    if dst_hw && src_hw {
        // SAFETY: as above: reads of the two surfaces.
        let (from, to) = (unsafe { (*src).data[3] }, unsafe { (*dst).data[3] });
        // SAFETY: as above: two surfaces of `SURFACE_SIZE` bytes.
        unsafe { ptr::copy(from, to, SURFACE_SIZE) };
        return 0;
    }
    let (sw, surface) = if src_hw {
        // SAFETY: as above: a read of the source's surface.
        (dst.cast_const(), unsafe { (*src).data[3] })
    } else {
        // SAFETY: as above: a read of the destination's surface.
        (src, unsafe { (*dst).data[3] })
    };
    // SAFETY: as above: reads of the context's software format and size.
    let (format, width, height) = (
        unsafe { (*ctx).sw_format },
        unsafe { (*ctx).width },
        unsafe { (*ctx).height },
    );
    // SAFETY: no precondition: NULL or a static descriptor.
    let desc = unsafe { ffi::av_pix_fmt_desc_get(format) };
    let mut k = 0;
    // SAFETY: no precondition.
    for i in 0..unsafe { ffi::av_pix_fmt_count_planes(format) } {
        // SAFETY: no precondition.
        let len = unsafe { ffi::av_image_get_linesize(format, width, i) } as usize;
        let shift = if i == 1 || i == 2 {
            // SAFETY: the software format's live, static descriptor.
            unsafe { (*desc).log2_chroma_h }
        } else {
            0
        };
        let rows = -((-height) >> shift);
        // SAFETY: as above: reads of the software frame's plane and line size.
        let (plane, line) =
            (
                unsafe { (*sw).data[i as usize] },
                unsafe { (*sw).linesize[i as usize] } as isize,
            );
        for row in 0..rows as isize {
            // SAFETY: as above, trusted: row `row` of the plane.
            let p = unsafe { plane.offset(row * line) };
            for x in 0..len {
                // SAFETY: as above: byte `k % SURFACE_SIZE` of the surface.
                let s = unsafe { surface.add(k % SURFACE_SIZE) };
                // SAFETY: as above, trusted: byte `x < len` of the row.
                let at = unsafe { p.add(x) };
                if src_hw {
                    // SAFETY: as above: a read of the surface byte.
                    let byte = unsafe { s.read() };
                    // SAFETY: as above: a write of the row byte.
                    unsafe { at.write(byte) };
                } else {
                    // SAFETY: as above: reads of the two bytes.
                    let (old, add) = (unsafe { s.read() }, unsafe { at.read() });
                    // SAFETY: as above: a write of the surface byte.
                    unsafe { s.write(old.wrapping_add(add)) };
                }
                k += 1;
            }
        }
    }
    0
}

unsafe extern "C" fn frames_derive_from(
    _dst: *mut FramesCtx,
    src: *mut FramesCtx,
    _flags: c_int,
) -> c_int {
    // SAFETY: C passes the live derived context and its live source.
    record(Event::DeriveFrom(unsafe { frames_kind(src) }));
    // SAFETY: as above: the source's backend configuration.
    unsafe { frames_hwctx(src) }.derive_ret
}

unsafe extern "C" fn frames_derive_to(
    dst: *mut FramesCtx,
    _src: *mut FramesCtx,
    _flags: c_int,
) -> c_int {
    // SAFETY: C passes the live derived context.
    record(Event::DeriveTo(unsafe { frames_kind(dst) }));
    // SAFETY: as above: its backend configuration.
    unsafe { frames_hwctx(dst) }.derive_ret
}

/// `hwdevice_ctx_free`, the release of a test device context.
unsafe extern "C" fn free_device(_opaque: *mut c_void, data: *mut u8) {
    // `data` is the private context `alloc_device` allocated, at its last reference.
    let ctxi = data.cast::<DeviceInternal>();
    // SAFETY: as above: the place of its public part.
    let ctx = unsafe { &raw mut (*ctxi).p };
    // SAFETY: as above: a read of its backend.
    let backend = unsafe { (*ctxi).hw_type };
    let kind = Kind::of(backend);
    // SAFETY: as above: a read of its `device_uninit` hook.
    if let Some(uninit) = unsafe { (*backend).device_uninit } {
        // SAFETY: the hook takes the live context, as C calls it.
        unsafe { uninit(ctx) };
    }
    // SAFETY: as above: a read of the user's `free` hook.
    if let Some(free) = unsafe { (*ctx).free } {
        // SAFETY: the hook takes the live context, as C calls it.
        unsafe { free(ctx) };
    }
    // SAFETY: as above: the place of its `source_device`.
    let source_device_ptr = unsafe { &raw mut (*ctxi).source_device };
    // SAFETY: NULL or the reference the context holds.
    unsafe { ffi::av_buffer_unref(source_device_ptr) };
    // SAFETY: as above: a read of its `hwctx`.
    let hwctx = unsafe { (*ctx).hwctx };
    // SAFETY: the `av_mallocz` block `alloc_device` gave it.
    unsafe { ffi::av_free(hwctx) };
    // SAFETY: the private context's own `av_mallocz` block, released once.
    unsafe { ffi::av_free(ctxi.cast()) };
    record(Event::DeviceFreed(kind));
}

/// A new, uninitialised device context of backend `kind`, as
/// `av_hwdevice_ctx_alloc` builds one: the only reference to it, its `hwctx`
/// a zeroed [`DeviceHwctx`]. Panics when out of memory.
pub(crate) fn alloc_device(kind: Kind) -> *mut ffi::AVBufferRef {
    // `av_hwdevice_ctx_alloc`'s steps over zeroed allocations.
    // SAFETY: no precondition; NULL when C cannot allocate.
    let ctxi = unsafe { ffi::av_mallocz(size_of::<DeviceInternal>()) }.cast::<DeviceInternal>();
    assert!(!ctxi.is_null());
    // SAFETY: the fresh private context: the place of its public part.
    let ctx = unsafe { &raw mut (*ctxi).p };
    // SAFETY: no precondition; NULL when C cannot allocate.
    let hwctx = unsafe { ffi::av_mallocz(size_of::<DeviceHwctx>()) };
    assert!(!hwctx.is_null());
    // SAFETY: as above: the context takes its `hwctx`.
    unsafe { (*ctx).hwctx = hwctx };
    // SAFETY: the fresh private context, which the buffer owns from here, released by
    // `free_device`.
    let buf = unsafe {
        ffi::av_buffer_create(
            ctxi.cast(),
            size_of::<DeviceCtx>(),
            Some(free_device),
            null_mut(),
            ffi::AV_BUFFER_FLAG_READONLY as c_int,
        )
    };
    assert!(!buf.is_null());
    // SAFETY: as above: its type.
    unsafe { (*ctx).type_ = kind.device_type() };
    // SAFETY: as above: its class.
    unsafe { (*ctx).av_class = &raw const DEVICE_CLASS.0 };
    // SAFETY: as above: its backend.
    unsafe { (*ctxi).hw_type = kind.backend() };
    buf
}

/// [`alloc_device`] initialised through C (`av_hwdevice_ctx_init`), whose
/// event it takes from the log.
pub(crate) fn device(kind: Kind) -> *mut ffi::AVBufferRef {
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
pub(crate) unsafe fn device_hwctx(buf: *mut ffi::AVBufferRef) -> *mut DeviceHwctx {
    // SAFETY: as the comment below states, for this part of the operation.
    let data_2 = unsafe { (*buf).data };
    // SAFETY: by the caller's contract.
    unsafe { (*data_2.cast::<DeviceCtx>()).hwctx.cast() }
}

/// Makes the test device context `buf` refers to derived from `source`, as
/// `av_hwdevice_ctx_create_derived_opts` records it: a reference to `source`
/// in its private `source_device`.
///
/// # Safety
///
/// `buf` and `source` must be live references to test device contexts, `buf`
/// not derived yet.
pub(crate) unsafe fn set_source_device(buf: *mut ffi::AVBufferRef, source: *mut ffi::AVBufferRef) {
    // SAFETY: by the caller's contract: a read of the live reference's data.
    let ctxi = unsafe { (*buf).data }.cast::<DeviceInternal>();
    // SAFETY: as above: a read of its `source_device`.
    assert!(unsafe { (*ctxi).source_device }.is_null());
    // SAFETY: by the caller's contract: a live reference, to which C adds one.
    let source = unsafe { ffi::av_buffer_ref(source) };
    assert!(!source.is_null());
    // SAFETY: as above: the context takes it.
    unsafe { (*ctxi).source_device = source };
}

/// The frames context a frames reference `buf` refers to.
///
/// # Safety
///
/// `buf` must be a live frames context reference.
pub(crate) unsafe fn frames_ctx(buf: *const ffi::AVBufferRef) -> *mut FramesCtx {
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
pub(crate) unsafe fn fill_frames(
    ctx: *mut FramesCtx,
    kind: Kind,
    width: c_int,
    height: c_int,
    pool_size: c_int,
) {
    // For the unsafe operations below: by the caller's contract.
    {
        // SAFETY: as the comment above this block states.
        unsafe { (*ctx).format = kind.format() };
        // SAFETY: as the comment above this block states.
        unsafe { (*ctx).sw_format = ffi::AVPixelFormat::AV_PIX_FMT_NV12 };
        // SAFETY: as the comment above this block states.
        unsafe { (*ctx).width = width };
        // SAFETY: as the comment above this block states.
        unsafe { (*ctx).height = height };
        // SAFETY: as the comment above this block states.
        unsafe { (*ctx).initial_pool_size = pool_size };
    }
}

/// Sets the backend results of the frames context `ctx`.
///
/// # Safety
///
/// `ctx` must be a live frames context on a test device, which no one reads
/// meanwhile.
pub(crate) unsafe fn set_frames_hwctx(ctx: *mut FramesCtx, hwctx: FramesHwctx) {
    // SAFETY: as the comment below states, for this part of the operation.
    let hwctx_2 = unsafe { (*ctx).hwctx };
    // SAFETY: by the caller's contract; the backends' `frames_hwctx_size` is
    // that of `FramesHwctx`.
    unsafe { hwctx_2.cast::<FramesHwctx>().write(hwctx) }
}

/// A frames context of `width` x `height` on `device` (a test device of
/// backend `kind`), initialised through C, its events taken from the log.
///
/// # Safety
///
/// `device` must be a live reference to an initialised test device context
/// of backend `kind`.
pub(crate) unsafe fn frames(
    device: *mut ffi::AVBufferRef,
    kind: Kind,
    width: c_int,
    height: c_int,
) -> *mut ffi::AVBufferRef {
    // For the unsafe operations below: by the caller's contract; then a fresh frames context.
    {
        // SAFETY: as the comment above this block states.
        let buf = unsafe { ffi::av_hwframe_ctx_alloc(device) };
        assert!(!buf.is_null());
        // SAFETY: as the comment below states, for this part of the operation.
        let frames_ctx_2 = unsafe { frames_ctx(buf) };
        // SAFETY: as the comment above this block states.
        unsafe { fill_frames(frames_ctx_2, kind, width, height, 0) };
        // SAFETY: as the comment above this block states.
        unsafe { assert_eq!(ffi::av_hwframe_ctx_init(buf), 0) };
        assert_eq!(take_events(), [Event::FramesInit(kind)]);
        buf
    }
}

/// The count of references to `buf`'s buffer.
///
/// # Safety
///
/// `buf` must be a live reference.
pub(crate) unsafe fn ref_count(buf: *const ffi::AVBufferRef) -> c_int {
    // SAFETY: by the caller's contract.
    unsafe { ffi::av_buffer_get_ref_count(buf) }
}

/// Releases the reference `buf`, if any.
///
/// # Safety
///
/// `buf` must be NULL or a live reference the caller owns.
pub(crate) unsafe fn unref(mut buf: *mut ffi::AVBufferRef) {
    // SAFETY: by the caller's contract.
    unsafe { ffi::av_buffer_unref(&raw mut buf) }
}

/// What a frame shows of itself: its format, size, which of `buf`/`data`
/// are set, whether `data[3]` and `data[0]` point at its first buffer's
/// surface, its frames context, and whether `extended_data` is `data`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FrameShape {
    pub(crate) format: c_int,
    pub(crate) width: c_int,
    pub(crate) height: c_int,
    pub(crate) bufs: [bool; 8],
    pub(crate) data: [bool; 8],
    pub(crate) linesize0: c_int,
    pub(crate) has_frames: bool,
    pub(crate) own_extended: bool,
}

/// The shape of `frame`.
///
/// # Safety
///
/// `frame` must be a live frame.
pub(crate) unsafe fn shape(frame: *const ffi::AVFrame) -> FrameShape {
    // SAFETY: `frame` is to a live object, as the block below relies on, so the
    // place of its `data` field is in bounds; nothing is accessed here.
    let data_ptr = unsafe { &raw const (*frame).data };
    // SAFETY: by the caller's contract, a live frame: a copy of its fields.
    let f = unsafe { frame.read() };
    FrameShape {
        format: f.format,
        width: f.width,
        height: f.height,
        bufs: f.buf.map(|b| !b.is_null()),
        data: f.data.map(|d| !d.is_null()),
        linesize0: f.linesize[0],
        has_frames: !f.hw_frames_ctx.is_null(),
        own_extended: ptr::eq(f.extended_data.cast_const(), data_ptr.cast()),
    }
}
