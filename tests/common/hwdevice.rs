//! A fake hardware backend for `libavutil/hwcontext.h`'s device context and
//! frames constraints tests, shared by the crate's unit tests and the `equiv`
//! and `ub_unsafe` integration tests, which include it with `#[path]` after
//! bringing the `-sys` crate into scope as `ffi`.
//!
//! The campaign builds compile no hardware backend, so
//! `av_hwdevice_ctx_alloc` fails for every type. [`fake_device`] builds what
//! it would return for a backend of this file's own: the library's private
//! `FFHWDeviceContext` around a public context, pointing at a private
//! `HWContextType`, in a read-only buffer whose release mirrors
//! `hwdevice_ctx_free`. C's routines that only consult the backend table
//! (`av_hwdevice_ctx_init`, `av_hwdevice_get_hwframe_constraints`) run on it
//! unchanged.
#![expect(
    clippy::cast_possible_wrap,
    clippy::cast_ptr_alignment,
    clippy::cast_sign_loss,
    reason = "test inputs are chosen within range, and the casts mirror the C conversions under comparison"
)]

use core::ffi::{c_char, c_int, c_void};
use core::ptr::{addr_of, addr_of_mut, null, null_mut};
use core::sync::atomic::{AtomicI32, AtomicUsize, Ordering};

use ffibox::{CMutSeam, CRefSeam, define_ctype};

use super::ffi;

/// `hwcontext_internal.h`'s private `HWContextType`, field for field; the
/// slots no test fills are untyped.
#[repr(C)]
pub(crate) struct HwContextType {
    pub(crate) type_: ffi::AVHWDeviceType,
    pub(crate) name: *const c_char,
    pub(crate) pix_fmts: *const ffi::AVPixelFormat,
    pub(crate) device_hwctx_size: usize,
    pub(crate) device_hwconfig_size: usize,
    pub(crate) frames_hwctx_size: usize,
    pub(crate) device_create: Option<unsafe extern "C" fn()>,
    pub(crate) device_derive: Option<unsafe extern "C" fn()>,
    pub(crate) device_init: Option<unsafe extern "C" fn(*mut ffi::AVHWDeviceContext) -> c_int>,
    pub(crate) device_uninit: Option<unsafe extern "C" fn(*mut ffi::AVHWDeviceContext)>,
    pub(crate) frames_get_constraints: Option<
        unsafe extern "C" fn(
            *mut ffi::AVHWDeviceContext,
            *const c_void,
            *mut ffi::AVHWFramesConstraints,
        ) -> c_int,
    >,
    pub(crate) frames_init: Option<unsafe extern "C" fn()>,
    pub(crate) frames_uninit: Option<unsafe extern "C" fn()>,
    pub(crate) frames_get_buffer: Option<unsafe extern "C" fn()>,
    pub(crate) transfer_get_formats: Option<unsafe extern "C" fn()>,
    pub(crate) transfer_data_to: Option<unsafe extern "C" fn()>,
    pub(crate) transfer_data_from: Option<unsafe extern "C" fn()>,
    pub(crate) map_to: Option<unsafe extern "C" fn()>,
    pub(crate) map_from: Option<unsafe extern "C" fn()>,
    pub(crate) frames_derive_to: Option<unsafe extern "C" fn()>,
    pub(crate) frames_derive_from: Option<unsafe extern "C" fn()>,
}

/// `hwcontext.c`'s private `FFHWDeviceContext`: the public context first.
#[repr(C)]
pub(crate) struct FfDeviceContext {
    pub(crate) p: ffi::AVHWDeviceContext,
    pub(crate) hw_type: *const HwContextType,
    pub(crate) source_device: *mut ffi::AVBufferRef,
}

/// Counters the fake backend bumps, shared with a test through
/// [`FakeHwctx::events`] so that they outlive the context.
#[derive(Debug, Default)]
pub(crate) struct Events {
    pub(crate) init: AtomicUsize,
    pub(crate) uninit: AtomicUsize,
    pub(crate) constraints: AtomicUsize,
}

impl Events {
    pub(crate) fn get(&self) -> (usize, usize, usize) {
        let load = |a: &AtomicUsize| a.load(Ordering::SeqCst);
        (
            load(&self.init),
            load(&self.uninit),
            load(&self.constraints),
        )
    }
}

/// The fake backend's public hardware context, what `hwctx` addresses: the
/// knobs its callbacks obey, and a payload the caller fills.
#[repr(C)]
#[expect(
    unreachable_pub,
    reason = "`define_ctype!` publishes a newtype over it, which needs it as visible"
)]
pub struct FakeHwctx {
    /// What `device_init` returns.
    pub(crate) init_ret: c_int,
    /// What `frames_get_constraints` returns, after filling both lists.
    pub(crate) constraints_ret: c_int,
    /// How many of [`SW_FORMATS`] `frames_get_constraints` lists; a negative
    /// count leaves the software list NULL.
    pub(crate) nb_sw: c_int,
    /// Set to 1 by `device_init`.
    pub(crate) initialised: c_int,
    /// The caller's payload.
    pub(crate) value: i64,
    /// NULL, or counters the callbacks bump.
    pub(crate) events: *const Events,
    /// The sizes `frames_get_constraints` found in the constraints C handed
    /// it, before filling them: min width and height, max width and height.
    /// Atomic, as queries of one shared device may run concurrently.
    pub(crate) seen_sizes: [AtomicI32; 4],
}

define_ctype!(
    /// [`FakeHwctx`], the fake device type's hardware context.
    FakeHw,
    FakeHwRef,
    FakeHwMut,
    FakeHwctx
);

impl FakeHwRef<'_> {
    pub(crate) fn value(&self) -> i64 {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `value` field is in bounds; nothing is accessed here.
        let value_ptr = unsafe { addr_of!((*self.as_ptr()).value) };
        // SAFETY: a field of the live hardware context the handle borrows.
        unsafe { value_ptr.read() }
    }

    pub(crate) fn seen_sizes(&self) -> [c_int; 4] {
        // SAFETY: as in `value`; the counters are atomics, shared.
        let seen = unsafe { &(*self.as_ptr()).seen_sizes };
        seen.each_ref().map(|a| a.load(Ordering::SeqCst))
    }

    pub(crate) fn initialised(&self) -> bool {
        // SAFETY: the handle's pointer is to a live object, as the operation below relies on,
        // so the place of its `initialised` field is in bounds; nothing is accessed here.
        let initialised_ptr = unsafe { addr_of!((*self.as_ptr()).initialised) };
        // SAFETY: as in `value`.
        unsafe { initialised_ptr.read() != 0 }
    }
}

impl FakeHwMut<'_> {
    pub(crate) fn set_value(&mut self, value: i64) {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `value` field is in bounds; nothing is accessed here.
        let value_ptr = unsafe { addr_of_mut!((*self.as_mut_ptr()).value) };
        // SAFETY: a field of the live hardware context the handle borrows
        // exclusively.
        unsafe { value_ptr.write(value) }
    }

    pub(crate) fn set_init_ret(&mut self, ret: c_int) {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `init_ret` field is in bounds; nothing is accessed here.
        let init_ret_ptr = unsafe { addr_of_mut!((*self.as_mut_ptr()).init_ret) };
        // SAFETY: as in `set_value`.
        unsafe { init_ret_ptr.write(ret) }
    }

    pub(crate) fn set_constraints_ret(&mut self, ret: c_int) {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `constraints_ret` field is in bounds; nothing is accessed here.
        let constraints_ret_ptr = unsafe { addr_of_mut!((*self.as_mut_ptr()).constraints_ret) };
        // SAFETY: as in `set_value`.
        unsafe { constraints_ret_ptr.write(ret) }
    }

    pub(crate) fn set_nb_sw(&mut self, n: c_int) {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `nb_sw` field is in bounds; nothing is accessed here.
        let nb_sw_ptr = unsafe { addr_of_mut!((*self.as_mut_ptr()).nb_sw) };
        // SAFETY: as in `set_value`.
        unsafe { nb_sw_ptr.write(n) }
    }

    /// Points the callbacks' counters at `events`, which must outlive the
    /// context.
    pub(crate) fn set_events(&mut self, events: *const Events) {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `events` field is in bounds; nothing is accessed here.
        let events_ptr = unsafe { addr_of_mut!((*self.as_mut_ptr()).events) };
        // SAFETY: as in `set_value`.
        unsafe { events_ptr.write(events) }
    }
}

/// The fake backend's hardware configuration, what
/// `av_hwdevice_hwconfig_alloc` would allocate for it.
#[repr(C)]
pub(crate) struct FakeConfig {
    /// The one hardware format the constraints list under this configuration.
    pub(crate) hw_format: ffi::AVPixelFormat,
}

/// The hardware format the fake backend lists without a configuration.
pub(crate) const HW_FORMAT: ffi::AVPixelFormat = ffi::AVPixelFormat::AV_PIX_FMT_VAAPI;

/// The software formats the fake backend can list.
pub(crate) const SW_FORMATS: [ffi::AVPixelFormat; 4] = [
    ffi::AVPixelFormat::AV_PIX_FMT_NV12,
    ffi::AVPixelFormat::AV_PIX_FMT_YUV420P,
    ffi::AVPixelFormat::AV_PIX_FMT_P010LE,
    ffi::AVPixelFormat::AV_PIX_FMT_RGB0,
];

/// The sizes the fake backend's constraints give: min width and height, max
/// width and height.
pub(crate) const SIZES: [c_int; 4] = [2, 4, 8192, 4320];

/// The events of the context `ctx`'s hardware context points at, if any.
///
/// # Safety
///
/// `ctx` must be a live fake context.
unsafe fn events<'a>(ctx: *mut ffi::AVHWDeviceContext) -> Option<&'a Events> {
    // SAFETY: as the comment below states, for this part of the operation.
    let hwctx_2 = unsafe { (*ctx).hwctx };
    // SAFETY: the caller passes a live fake context, whose `hwctx` is a
    // `FakeHwctx`: a read of its events pointer.
    let events = unsafe { (*hwctx_2.cast::<FakeHwctx>()).events };
    // SAFETY: as above; the events, when set, outlive the context.
    unsafe { events.as_ref() }
}

unsafe extern "C" fn fake_device_init(ctx: *mut ffi::AVHWDeviceContext) -> c_int {
    // For the unsafe operations below: C passes the live fake context it initialises.
    {
        // SAFETY: as the comment above this block states.
        if let Some(e) = unsafe { events(ctx) } {
            e.init.fetch_add(1, Ordering::SeqCst);
        }
        // SAFETY: as the comment above this block states.
        let hw = unsafe { (*ctx).hwctx.cast::<FakeHwctx>() };
        // SAFETY: as the comment above this block states.
        unsafe { (*hw).initialised = 1 };
        // SAFETY: as the comment above this block states.
        unsafe { (*hw).init_ret }
    }
}

unsafe extern "C" fn fake_device_uninit(ctx: *mut ffi::AVHWDeviceContext) {
    // SAFETY: C passes the live fake context it releases.
    if let Some(e) = unsafe { events(ctx) } {
        e.uninit.fetch_add(1, Ordering::SeqCst);
    }
}

/// An `av_malloc_array` block holding `formats` and a terminating `NONE`.
fn c_list(formats: &[ffi::AVPixelFormat]) -> *mut ffi::AVPixelFormat {
    let n = formats.len() + 1;
    // SAFETY: a fresh block of `n` formats or NULL.
    let list = unsafe { ffi::av_malloc_array(n, size_of::<ffi::AVPixelFormat>()) }
        .cast::<ffi::AVPixelFormat>();
    if !list.is_null() {
        for (i, &f) in formats
            .iter()
            .chain(&[ffi::AVPixelFormat::AV_PIX_FMT_NONE])
            .enumerate()
        {
            // SAFETY: `i` is within the block `list` points to, as the operation below
            // relies on, so the offset stays in bounds; nothing is accessed here.
            let list_at = unsafe { list.add(i) };
            // SAFETY: `i < n`, inside the block.
            unsafe { list_at.write(f) };
        }
    }
    list
}

unsafe extern "C" fn fake_frames_get_constraints(
    ctx: *mut ffi::AVHWDeviceContext,
    hwconfig: *const c_void,
    constraints: *mut ffi::AVHWFramesConstraints,
) -> c_int {
    // For the unsafe operations below: C passes the live fake context, NULL or the caller's
    // `FakeConfig`, and the zero-filled constraints it owns.
    {
        // SAFETY: as the comment above this block states.
        unsafe {
            if let Some(e) = events(ctx) {
                e.constraints.fetch_add(1, Ordering::SeqCst);
            }
        }
        // SAFETY: as the comment above this block states.
        let hw = unsafe { (*ctx).hwctx.cast::<FakeHwctx>() };
        let c = constraints;
        // SAFETY: as the comment above this block states.
        let sizes = [
            unsafe { (*c).min_width },
            unsafe { (*c).min_height },
            unsafe { (*c).max_width },
            unsafe { (*c).max_height },
        ];
        // SAFETY: as the comment above this block states.
        unsafe {
            for (seen, size) in (*hw).seen_sizes.iter().zip(sizes) {
                seen.store(size, Ordering::SeqCst);
            }
        }
        // SAFETY: as the comment above this block states.
        let hw_format = unsafe {
            match hwconfig.cast::<FakeConfig>().as_ref() {
                Some(config) => config.hw_format,
                None => HW_FORMAT,
            }
        };
        // SAFETY: as the comment above this block states.
        unsafe { (*constraints).valid_hw_formats = c_list(&[hw_format]) };
        // SAFETY: as the comment above this block states.
        let nb_sw = unsafe { (*hw).nb_sw };
        // SAFETY: as the comment above this block states.
        unsafe {
            if nb_sw >= 0 {
                (*constraints).valid_sw_formats =
                    c_list(&SW_FORMATS[..(nb_sw as usize).min(SW_FORMATS.len())]);
            }
        }
        // SAFETY: as the comment above this block states.
        unsafe { (*constraints).min_width = SIZES[0] };
        // SAFETY: as the comment above this block states.
        unsafe { (*constraints).min_height = SIZES[1] };
        // SAFETY: as the comment above this block states.
        unsafe { (*constraints).max_width = SIZES[2] };
        // SAFETY: as the comment above this block states.
        unsafe { (*constraints).max_height = SIZES[3] };
        // SAFETY: as the comment above this block states.
        unsafe { (*hw).constraints_ret }
    }
}

/// The fake backend's name.
pub(crate) const NAME: &core::ffi::CStr = c"crustify-fake";

/// The formats a fake frames context may use, as a backend's table.
pub(crate) struct Formats(pub [ffi::AVPixelFormat; 2]);
// SAFETY: plain values, never written.
unsafe impl Sync for Formats {}

pub(crate) static PIX_FMTS: Formats = Formats([HW_FORMAT, ffi::AVPixelFormat::AV_PIX_FMT_NONE]);

/// A backend table, shareable as C's `const` ones are.
pub(crate) struct Backend(pub HwContextType);
// SAFETY: never written; the pointers address statics.
unsafe impl Sync for Backend {}

/// The fake backend.
pub(crate) static FAKE_TYPE: Backend = Backend(HwContextType {
    type_: ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_VAAPI,
    name: NAME.as_ptr(),
    pix_fmts: &raw const PIX_FMTS.0[0],
    device_hwctx_size: size_of::<FakeHwctx>(),
    device_hwconfig_size: size_of::<FakeConfig>(),
    frames_hwctx_size: 0,
    device_create: None,
    device_derive: None,
    device_init: Some(fake_device_init),
    device_uninit: Some(fake_device_uninit),
    frames_get_constraints: Some(fake_frames_get_constraints),
    frames_init: None,
    frames_uninit: None,
    frames_get_buffer: None,
    transfer_get_formats: None,
    transfer_data_to: None,
    transfer_data_from: None,
    map_to: None,
    map_from: None,
    frames_derive_to: None,
    frames_derive_from: None,
});

/// The same backend without a hardware context or a constraints query.
pub(crate) static BARE_TYPE: Backend = Backend(HwContextType {
    device_hwctx_size: 0,
    device_init: None,
    device_uninit: None,
    frames_get_constraints: None,
    ..FAKE_TYPE.0
});

/// `hwdevice_ctx_class`'s `item_name`: the backend's name.
unsafe extern "C" fn item_name(ctx: *mut c_void) -> *const c_char {
    // SAFETY: C passes a live fake context: a read of its backend.
    let backend = unsafe { (*ctx.cast::<FfDeviceContext>()).hw_type };
    // SAFETY: the backend is one of the static fake backends.
    unsafe { (*backend).name }
}

/// An `AVClass`, shareable as C's `const` ones are.
pub(crate) struct Class(pub ffi::AVClass);
// SAFETY: never written; the pointers address statics.
unsafe impl Sync for Class {}

/// A copy of `hwdevice_ctx_class`.
pub(crate) static CLASS: Class = Class(ffi::AVClass {
    class_name: c"AVHWDeviceContext".as_ptr(),
    item_name: Some(item_name),
    option: null(),
    version: 0,
    log_level_offset_offset: 0,
    parent_log_context_offset: 0,
    category: ffi::AVClassCategory::AV_CLASS_CATEGORY_HWDEVICE,
    get_category: None,
    query_ranges: None,
    child_next: None,
    child_class_iterate: None,
    state_flags_offset: 0,
});

/// `hwdevice_ctx_free`, for a fake context.
unsafe extern "C" fn fake_ctx_free(_opaque: *mut c_void, data: *mut u8) {
    let ctxi = data.cast::<FfDeviceContext>();
    // For the unsafe operations below: the buffer's last reference is gone, so the context is this
    // call's; the steps are hwdevice_ctx_free's.
    {
        // SAFETY: as the comment above this block states.
        let ctx = unsafe { addr_of_mut!((*ctxi).p) };
        // SAFETY: as the comment above this block states: a read of the backend.
        let backend = unsafe { (*ctxi).hw_type };
        // SAFETY: the backend is one of the static fake backends.
        if let Some(uninit) = unsafe { (*backend).device_uninit } {
            // SAFETY: the hook takes the live context, as C calls it.
            unsafe { uninit(ctx) };
        }
        // SAFETY: as the comment above this block states.
        if let Some(free) = unsafe { (*ctx).free } {
            // SAFETY: as the comment above this statement states.
            unsafe { free(ctx) };
        }
        // SAFETY: `ctxi` is to a live object, as the block below relies on, so the
        // place of its `source_device` field is in bounds; nothing is accessed here.
        let source_device_ptr = unsafe { &raw mut (*ctxi).source_device };
        // SAFETY: as the comment above this block states.
        unsafe { ffi::av_buffer_unref(source_device_ptr) };
        // SAFETY: `ctx` is to a live object, as the block below relies on, so the
        // place of its `hwctx` field is in bounds; nothing is accessed here.
        let hwctx_ptr = unsafe { &raw mut (*ctx).hwctx };
        // SAFETY: as the comment above this block states.
        unsafe { ffi::av_freep(hwctx_ptr.cast()) };
        // SAFETY: as the comment above this block states.
        unsafe { ffi::av_free(ctxi.cast()) };
    }
}

/// What `av_hwdevice_ctx_alloc(type_)` returns for a backend `backend`: the
/// only reference to a fresh, uninitialised context; NULL when C cannot
/// allocate it.
pub(crate) fn fake_device_of(
    backend: &'static Backend,
    type_: ffi::AVHWDeviceType,
) -> *mut ffi::AVBufferRef {
    let hw_type = &backend.0;
    // For the unsafe operations below: av_hwdevice_ctx_alloc's steps, on blocks this function owns
    // until the buffer takes them.
    {
        // SAFETY: as the comment above this block states.
        let ctxi =
            unsafe { ffi::av_mallocz(size_of::<FfDeviceContext>()).cast::<FfDeviceContext>() };
        if ctxi.is_null() {
            return null_mut();
        }
        // SAFETY: as the comment above this block states.
        let ctx = unsafe { addr_of_mut!((*ctxi).p) };
        if hw_type.device_hwctx_size != 0 {
            // SAFETY: no precondition; NULL when C cannot allocate.
            let hwctx = unsafe { ffi::av_mallocz(hw_type.device_hwctx_size) };
            // SAFETY: as the comment above this block states: the context takes it.
            unsafe { (*ctx).hwctx = hwctx };
            if hwctx.is_null() {
                // SAFETY: as the comment above this block states, released once.
                unsafe { ffi::av_free(ctxi.cast()) };
                return null_mut();
            }
        }
        // SAFETY: as the comment above this block states.
        let buf = unsafe {
            ffi::av_buffer_create(
                ctx.cast(),
                size_of::<ffi::AVHWDeviceContext>(),
                Some(fake_ctx_free),
                null_mut(),
                ffi::AV_BUFFER_FLAG_READONLY as c_int,
            )
        };
        // SAFETY: `ctx` is to a live object, as the block below relies on, so the
        // place of its `hwctx` field is in bounds; nothing is accessed here.
        let hwctx_ptr = unsafe { &raw mut (*ctx).hwctx };
        if buf.is_null() {
            // SAFETY: as the comment above this block states, released once.
            unsafe { ffi::av_freep(hwctx_ptr.cast()) };
            // SAFETY: as the comment above this block states, released once.
            unsafe { ffi::av_free(ctxi.cast()) };
            return null_mut();
        }
        // SAFETY: as the comment above this block states.
        unsafe { (*ctx).type_ = type_ };
        // SAFETY: as the comment above this block states.
        unsafe { (*ctx).av_class = &raw const CLASS.0 };
        // SAFETY: as the comment above this block states.
        unsafe { (*ctxi).hw_type = hw_type };
        buf
    }
}

/// [`fake_device_of`] the fake backend, as a VAAPI device.
pub(crate) fn fake_device() -> *mut ffi::AVBufferRef {
    fake_device_of(&FAKE_TYPE, ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_VAAPI)
}

/// The private data [`set_foreign_free`] stores, as a backend's
/// `device_create` does: counted when its free callback releases it.
pub(crate) struct ForeignPriv {
    pub(crate) freed: *const AtomicUsize,
}

unsafe extern "C" fn foreign_free(ctx: *mut ffi::AVHWDeviceContext) {
    // For the unsafe operations below: `user_opaque` is the box `set_foreign_free` leaked, and its
    // counter outlives the context.
    {
        // SAFETY: as the comment below states, for this part of the operation.
        let user_opaque_2 = unsafe { (*ctx).user_opaque };
        // SAFETY: as the comment above this block states.
        let p = unsafe { Box::from_raw(user_opaque_2.cast::<ForeignPriv>()) };
        // SAFETY: as the comment above this block states.
        unsafe {
            if let Some(freed) = p.freed.as_ref() {
                freed.fetch_add(1, Ordering::SeqCst);
            }
        }
        // SAFETY: as the comment above this block states.
        unsafe { (*ctx).user_opaque = null_mut() };
    }
}

/// Sets the context's `free` and `user_opaque` as a backend's
/// `device_create` does (`vaapi_device_create`): a private struct, released
/// by the backend's own free callback, which bumps `freed`.
///
/// # Safety
///
/// `buf` must be the only reference to a live fake context whose `free` and
/// `user_opaque` are unset, and `freed` NULL or live as long as it.
pub(crate) unsafe fn set_foreign_free(buf: *mut ffi::AVBufferRef, freed: *const AtomicUsize) {
    // For the unsafe operations below: the caller's context, which nothing else reaches.
    {
        // SAFETY: as the comment above this block states.
        let ctx = unsafe { (*buf).data.cast::<ffi::AVHWDeviceContext>() };
        // SAFETY: as the comment above this block states.
        unsafe { (*ctx).user_opaque = Box::into_raw(Box::new(ForeignPriv { freed })).cast() };
        // SAFETY: as the comment above this block states.
        unsafe { (*ctx).free = Some(foreign_free) };
    }
}

/// The public context `buf` refers to.
///
/// # Safety
///
/// `buf` must be a live reference to a fake context.
pub(crate) unsafe fn context(buf: *mut ffi::AVBufferRef) -> *mut ffi::AVHWDeviceContext {
    // SAFETY: the caller's live reference.
    unsafe { (*buf).data.cast() }
}

/// The fake hardware context of the context `buf` refers to; NULL for a
/// bare one.
///
/// # Safety
///
/// As [`context`].
pub(crate) unsafe fn hwctx(buf: *mut ffi::AVBufferRef) -> *mut FakeHwctx {
    // SAFETY: as the comment below states, for this part of the operation.
    let context_2 = unsafe { context(buf) };
    // SAFETY: the caller's live reference.
    unsafe { (*context_2).hwctx.cast() }
}
