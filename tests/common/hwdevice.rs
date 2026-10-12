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
#![allow(dead_code)]

use core::ffi::{c_char, c_int, c_void};
use core::ptr::{addr_of, addr_of_mut, null, null_mut};
use core::sync::atomic::{AtomicI32, AtomicUsize, Ordering};

use ffibox::define_ctype;

use super::ffi;

/// `hwcontext_internal.h`'s private `HWContextType`, field for field; the
/// slots no test fills are untyped.
#[repr(C)]
pub struct HwContextType {
    pub type_: ffi::AVHWDeviceType,
    pub name: *const c_char,
    pub pix_fmts: *const ffi::AVPixelFormat,
    pub device_hwctx_size: usize,
    pub device_hwconfig_size: usize,
    pub frames_hwctx_size: usize,
    pub device_create: Option<unsafe extern "C" fn()>,
    pub device_derive: Option<unsafe extern "C" fn()>,
    pub device_init: Option<unsafe extern "C" fn(*mut ffi::AVHWDeviceContext) -> c_int>,
    pub device_uninit: Option<unsafe extern "C" fn(*mut ffi::AVHWDeviceContext)>,
    pub frames_get_constraints: Option<
        unsafe extern "C" fn(
            *mut ffi::AVHWDeviceContext,
            *const c_void,
            *mut ffi::AVHWFramesConstraints,
        ) -> c_int,
    >,
    pub frames_init: Option<unsafe extern "C" fn()>,
    pub frames_uninit: Option<unsafe extern "C" fn()>,
    pub frames_get_buffer: Option<unsafe extern "C" fn()>,
    pub transfer_get_formats: Option<unsafe extern "C" fn()>,
    pub transfer_data_to: Option<unsafe extern "C" fn()>,
    pub transfer_data_from: Option<unsafe extern "C" fn()>,
    pub map_to: Option<unsafe extern "C" fn()>,
    pub map_from: Option<unsafe extern "C" fn()>,
    pub frames_derive_to: Option<unsafe extern "C" fn()>,
    pub frames_derive_from: Option<unsafe extern "C" fn()>,
}

/// `hwcontext.c`'s private `FFHWDeviceContext`: the public context first.
#[repr(C)]
pub struct FfDeviceContext {
    pub p: ffi::AVHWDeviceContext,
    pub hw_type: *const HwContextType,
    pub source_device: *mut ffi::AVBufferRef,
}

/// Counters the fake backend bumps, shared with a test through
/// [`FakeHwctx::events`] so that they outlive the context.
#[derive(Debug, Default)]
pub struct Events {
    pub init: AtomicUsize,
    pub uninit: AtomicUsize,
    pub constraints: AtomicUsize,
}

impl Events {
    pub fn get(&self) -> (usize, usize, usize) {
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
pub struct FakeHwctx {
    /// What `device_init` returns.
    pub init_ret: c_int,
    /// What `frames_get_constraints` returns, after filling both lists.
    pub constraints_ret: c_int,
    /// How many of [`SW_FORMATS`] `frames_get_constraints` lists; a negative
    /// count leaves the software list NULL.
    pub nb_sw: c_int,
    /// Set to 1 by `device_init`.
    pub initialised: c_int,
    /// The caller's payload.
    pub value: i64,
    /// NULL, or counters the callbacks bump.
    pub events: *const Events,
    /// The sizes `frames_get_constraints` found in the constraints C handed
    /// it, before filling them: min width and height, max width and height.
    /// Atomic, as queries of one shared device may run concurrently.
    pub seen_sizes: [AtomicI32; 4],
}

define_ctype!(
    /// [`FakeHwctx`], the fake device type's hardware context.
    FakeHw,
    FakeHwRef,
    FakeHwMut,
    FakeHwctx
);

impl FakeHwRef<'_> {
    pub fn value(&self) -> i64 {
        // SAFETY: a field of the live hardware context the handle borrows.
        unsafe { addr_of!((*self.as_ptr()).value).read() }
    }

    pub fn seen_sizes(&self) -> [c_int; 4] {
        // SAFETY: as in `value`; the counters are atomics, shared.
        let seen = unsafe { &(*self.as_ptr()).seen_sizes };
        seen.each_ref().map(|a| a.load(Ordering::SeqCst))
    }

    pub fn initialised(&self) -> bool {
        // SAFETY: as in `value`.
        unsafe { addr_of!((*self.as_ptr()).initialised).read() != 0 }
    }
}

impl FakeHwMut<'_> {
    pub fn set_value(&mut self, value: i64) {
        // SAFETY: a field of the live hardware context the handle borrows
        // exclusively.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).value).write(value) }
    }

    pub fn set_init_ret(&mut self, ret: c_int) {
        // SAFETY: as in `set_value`.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).init_ret).write(ret) }
    }

    pub fn set_constraints_ret(&mut self, ret: c_int) {
        // SAFETY: as in `set_value`.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).constraints_ret).write(ret) }
    }

    pub fn set_nb_sw(&mut self, n: c_int) {
        // SAFETY: as in `set_value`.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).nb_sw).write(n) }
    }

    /// Points the callbacks' counters at `events`, which must outlive the
    /// context.
    pub fn set_events(&mut self, events: *const Events) {
        // SAFETY: as in `set_value`.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).events).write(events) }
    }
}

/// The fake backend's hardware configuration, what
/// `av_hwdevice_hwconfig_alloc` would allocate for it.
#[repr(C)]
pub struct FakeConfig {
    /// The one hardware format the constraints list under this configuration.
    pub hw_format: ffi::AVPixelFormat,
}

/// The hardware format the fake backend lists without a configuration.
pub const HW_FORMAT: ffi::AVPixelFormat = ffi::AVPixelFormat::AV_PIX_FMT_VAAPI;

/// The software formats the fake backend can list.
pub const SW_FORMATS: [ffi::AVPixelFormat; 4] = [
    ffi::AVPixelFormat::AV_PIX_FMT_NV12,
    ffi::AVPixelFormat::AV_PIX_FMT_YUV420P,
    ffi::AVPixelFormat::AV_PIX_FMT_P010LE,
    ffi::AVPixelFormat::AV_PIX_FMT_RGB0,
];

/// The sizes the fake backend's constraints give: min width and height, max
/// width and height.
pub const SIZES: [c_int; 4] = [2, 4, 8192, 4320];

/// The events of the context `ctx`'s hardware context points at, if any.
///
/// # Safety
///
/// `ctx` must be a live fake context.
unsafe fn events<'a>(ctx: *mut ffi::AVHWDeviceContext) -> Option<&'a Events> {
    // SAFETY: the caller passes a live fake context, whose `hwctx` is a
    // `FakeHwctx` and whose events, when set, outlive it.
    unsafe { (*(*ctx).hwctx.cast::<FakeHwctx>()).events.as_ref() }
}

unsafe extern "C" fn fake_device_init(ctx: *mut ffi::AVHWDeviceContext) -> c_int {
    // SAFETY: C passes the live fake context it initialises.
    unsafe {
        if let Some(e) = events(ctx) {
            e.init.fetch_add(1, Ordering::SeqCst);
        }
        let hw = (*ctx).hwctx.cast::<FakeHwctx>();
        (*hw).initialised = 1;
        (*hw).init_ret
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
            // SAFETY: `i < n`, inside the block.
            unsafe { list.add(i).write(f) };
        }
    }
    list
}

unsafe extern "C" fn fake_frames_get_constraints(
    ctx: *mut ffi::AVHWDeviceContext,
    hwconfig: *const c_void,
    constraints: *mut ffi::AVHWFramesConstraints,
) -> c_int {
    // SAFETY: C passes the live fake context, NULL or the caller's
    // `FakeConfig`, and the zero-filled constraints it owns.
    unsafe {
        if let Some(e) = events(ctx) {
            e.constraints.fetch_add(1, Ordering::SeqCst);
        }
        let hw = (*ctx).hwctx.cast::<FakeHwctx>();
        let c = constraints;
        let sizes = [
            (*c).min_width,
            (*c).min_height,
            (*c).max_width,
            (*c).max_height,
        ];
        for (seen, size) in (*hw).seen_sizes.iter().zip(sizes) {
            seen.store(size, Ordering::SeqCst);
        }
        let hw_format = match hwconfig.cast::<FakeConfig>().as_ref() {
            Some(config) => config.hw_format,
            None => HW_FORMAT,
        };
        (*constraints).valid_hw_formats = c_list(&[hw_format]);
        let nb_sw = (*hw).nb_sw;
        if nb_sw >= 0 {
            (*constraints).valid_sw_formats =
                c_list(&SW_FORMATS[..(nb_sw as usize).min(SW_FORMATS.len())]);
        }
        (*constraints).min_width = SIZES[0];
        (*constraints).min_height = SIZES[1];
        (*constraints).max_width = SIZES[2];
        (*constraints).max_height = SIZES[3];
        (*hw).constraints_ret
    }
}

/// The fake backend's name.
pub const NAME: &core::ffi::CStr = c"crustify-fake";

/// The formats a fake frames context may use, as a backend's table.
pub struct Formats(pub [ffi::AVPixelFormat; 2]);
// SAFETY: plain values, never written.
unsafe impl Sync for Formats {}

pub static PIX_FMTS: Formats = Formats([HW_FORMAT, ffi::AVPixelFormat::AV_PIX_FMT_NONE]);

/// A backend table, shareable as C's `const` ones are.
pub struct Backend(pub HwContextType);
// SAFETY: never written; the pointers address statics.
unsafe impl Sync for Backend {}

/// The fake backend.
pub static FAKE_TYPE: Backend = Backend(HwContextType {
    type_: ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_VAAPI,
    name: NAME.as_ptr(),
    pix_fmts: &PIX_FMTS.0[0],
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
pub static BARE_TYPE: Backend = Backend(HwContextType {
    device_hwctx_size: 0,
    device_init: None,
    device_uninit: None,
    frames_get_constraints: None,
    ..FAKE_TYPE.0
});

/// `hwdevice_ctx_class`'s `item_name`: the backend's name.
unsafe extern "C" fn item_name(ctx: *mut c_void) -> *const c_char {
    // SAFETY: C passes a live fake context.
    unsafe { (*(*ctx.cast::<FfDeviceContext>()).hw_type).name }
}

/// An `AVClass`, shareable as C's `const` ones are.
pub struct Class(pub ffi::AVClass);
// SAFETY: never written; the pointers address statics.
unsafe impl Sync for Class {}

/// A copy of `hwdevice_ctx_class`.
pub static CLASS: Class = Class(ffi::AVClass {
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
    // SAFETY: the buffer's last reference is gone, so the context is this
    // call's; the steps are hwdevice_ctx_free's.
    unsafe {
        let ctx = addr_of_mut!((*ctxi).p);
        if let Some(uninit) = (*(*ctxi).hw_type).device_uninit {
            uninit(ctx);
        }
        if let Some(free) = (*ctx).free {
            free(ctx);
        }
        ffi::av_buffer_unref(addr_of_mut!((*ctxi).source_device));
        ffi::av_freep(addr_of_mut!((*ctx).hwctx).cast());
        ffi::av_free(ctxi.cast());
    }
}

/// What `av_hwdevice_ctx_alloc(type_)` returns for a backend `backend`: the
/// only reference to a fresh, uninitialised context; NULL when C cannot
/// allocate it.
pub fn fake_device_of(
    backend: &'static Backend,
    type_: ffi::AVHWDeviceType,
) -> *mut ffi::AVBufferRef {
    let hw_type = &backend.0;
    // SAFETY: av_hwdevice_ctx_alloc's steps, on blocks this function owns
    // until the buffer takes them.
    unsafe {
        let ctxi = ffi::av_mallocz(size_of::<FfDeviceContext>()).cast::<FfDeviceContext>();
        if ctxi.is_null() {
            return null_mut();
        }
        let ctx = addr_of_mut!((*ctxi).p);
        if hw_type.device_hwctx_size != 0 {
            (*ctx).hwctx = ffi::av_mallocz(hw_type.device_hwctx_size);
            if (*ctx).hwctx.is_null() {
                ffi::av_free(ctxi.cast());
                return null_mut();
            }
        }
        let buf = ffi::av_buffer_create(
            ctx.cast(),
            size_of::<ffi::AVHWDeviceContext>(),
            Some(fake_ctx_free),
            null_mut(),
            ffi::AV_BUFFER_FLAG_READONLY as c_int,
        );
        if buf.is_null() {
            ffi::av_freep(addr_of_mut!((*ctx).hwctx).cast());
            ffi::av_free(ctxi.cast());
            return null_mut();
        }
        (*ctx).type_ = type_;
        (*ctx).av_class = &CLASS.0;
        (*ctxi).hw_type = hw_type;
        buf
    }
}

/// [`fake_device_of`] the fake backend, as a VAAPI device.
pub fn fake_device() -> *mut ffi::AVBufferRef {
    fake_device_of(&FAKE_TYPE, ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_VAAPI)
}

/// The private data [`set_foreign_free`] stores, as a backend's
/// `device_create` does: counted when its free callback releases it.
pub struct ForeignPriv {
    pub freed: *const AtomicUsize,
}

unsafe extern "C" fn foreign_free(ctx: *mut ffi::AVHWDeviceContext) {
    // SAFETY: `user_opaque` is the box `set_foreign_free` leaked, and its
    // counter outlives the context.
    unsafe {
        let p = Box::from_raw((*ctx).user_opaque.cast::<ForeignPriv>());
        if let Some(freed) = p.freed.as_ref() {
            freed.fetch_add(1, Ordering::SeqCst);
        }
        (*ctx).user_opaque = null_mut();
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
pub unsafe fn set_foreign_free(buf: *mut ffi::AVBufferRef, freed: *const AtomicUsize) {
    // SAFETY: the caller's context, which nothing else reaches.
    unsafe {
        let ctx = (*buf).data.cast::<ffi::AVHWDeviceContext>();
        (*ctx).user_opaque = Box::into_raw(Box::new(ForeignPriv { freed })).cast();
        (*ctx).free = Some(foreign_free);
    }
}

/// The public context `buf` refers to.
///
/// # Safety
///
/// `buf` must be a live reference to a fake context.
pub unsafe fn context(buf: *mut ffi::AVBufferRef) -> *mut ffi::AVHWDeviceContext {
    // SAFETY: the caller's live reference.
    unsafe { (*buf).data.cast() }
}

/// The fake hardware context of the context `buf` refers to; NULL for a
/// bare one.
///
/// # Safety
///
/// As [`context`].
pub unsafe fn hwctx(buf: *mut ffi::AVBufferRef) -> *mut FakeHwctx {
    // SAFETY: the caller's live reference.
    unsafe { (*context(buf)).hwctx.cast() }
}
