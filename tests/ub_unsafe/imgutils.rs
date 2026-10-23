//! UB tests for `libavutil/imgutils.h`'s `check_size_with_context`, with
//! parent contexts adopted through the unsafe `ObjectRef::from_ptr`, each
//! within its contract: no parent context, and a raw one led by a valid
//! class, with level offsets up to the overflow bound, on valid and invalid
//! sizes, on several threads. A class-less context is no `ObjectRef`.

use core::ffi::{c_int, c_void};
use core::ptr;
use std::thread;

use ffmpeg_bindings::libavutil::libavutil::ffi;
use ffmpeg_bindings::libavutil::libavutil::imgutils::check_size_with_context;
use ffmpeg_bindings::libavutil::libavutil::opt::ObjectRef;

/// `AVERROR(EINVAL)`.
const EINVAL: c_int = -22;

/// A logging context: its class first, as every C context has.
#[repr(C)]
struct Ctx {
    class: *const ffi::AVClass,
}

/// The current `LIBAVUTIL_VERSION_INT`.
const VERSION: c_int = (ffi::LIBAVUTIL_VERSION_MAJOR << 16
    | ffi::LIBAVUTIL_VERSION_MINOR << 8
    | ffi::LIBAVUTIL_VERSION_MICRO) as c_int;

/// A class C's default callback can name a parent by.
fn class() -> ffi::AVClass {
    ffi::AVClass {
        class_name: c"ub-imgutils-parent".as_ptr(),
        item_name: Some(ffi::av_default_item_name),
        version: VERSION,
        ..Default::default()
    }
}

/// The checks every context runs: valid and invalid sizes, at the offsets
/// around the overflow bound.
fn checks(ctx: Option<ObjectRef<'_>>) {
    let max = c_int::MAX - ffi::AV_LOG_ERROR as c_int;
    for offset in [c_int::MIN, -8, 0, 8, max] {
        assert_eq!(check_size_with_context(64, 48, offset, ctx), Ok(()));
        assert_eq!(check_size_with_context(0, 48, offset, ctx), Err(EINVAL));
        assert_eq!(
            check_size_with_context(u32::MAX, u32::MAX, offset, ctx),
            Err(EINVAL)
        );
    }
    for offset in [max + 1, c_int::MAX] {
        // Rejected before C runs.
        assert_eq!(check_size_with_context(64, 48, offset, ctx), Err(EINVAL));
        assert_eq!(check_size_with_context(0, 0, offset, ctx), Err(EINVAL));
    }
}

/// The context at `ctx` as a handle.
///
/// # Safety
///
/// `ctx` is a live `Ctx` led by a valid, live class, unwritten for `'a`.
unsafe fn object<'a>(ctx: *const Ctx) -> ObjectRef<'a> {
    // SAFETY: a context of a named class with no options, callbacks or
    // offsets, per the caller.
    unsafe { ObjectRef::from_ptr(ctx.cast_mut().cast::<c_void>()) }.unwrap()
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn null_context() {
    checks(None);
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn classless_context_is_no_object() {
    let mut ctx = Box::new(Ctx { class: ptr::null() });
    // SAFETY: a live struct, which `from_ptr` refuses for its NULL class.
    assert!(unsafe { ObjectRef::from_ptr((&raw mut *ctx).cast()) }.is_none());
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn context_with_a_class() {
    let class = Box::new(class());
    let ctx = Box::new(Ctx { class: &*class });
    // SAFETY: a live struct led by a pointer to a valid, live class.
    checks(Some(unsafe { object(&*ctx) }));
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn shared_context_on_many_threads() {
    let class = class();
    let ctx = Ctx { class: &class };
    let addr = &raw const ctx as usize;
    thread::scope(|s| {
        for _ in 0..4 {
            s.spawn(move || {
                // SAFETY: the context and its class outlive the scope and
                // are only read, which C does too.
                checks(Some(unsafe { object(addr as *const Ctx) }));
            });
        }
    });
}

/// UB tests for `check_size_max_pixels_with_context`, as above: no parent
/// context and a classed one, with
/// level offsets up to the overflow bound, on sizes valid and invalid by
/// dimension and by pixel count, for formats with and without linesizes,
/// including monochrome widths whose linesize overflows in C, on several
/// threads.
mod max_pixels {
    use core::ffi::c_int;
    use std::thread;

    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::imgutils::check_size_max_pixels_with_context;
    use ffmpeg_bindings::libavutil::libavutil::opt::ObjectRef;
    use ffmpeg_bindings::libavutil::libavutil::pixfmt::PixelFormat;

    use super::{Ctx, EINVAL, class, object};

    /// The checks every context runs.
    fn checks(ctx: Option<ObjectRef<'_>>) {
        let max = c_int::MAX - ffi::AV_LOG_ERROR as c_int;
        let mono = PixelFormat::MONOBLACK;
        let formats = [
            PixelFormat::YUV420P,
            mono,
            PixelFormat::BGR4,
            PixelFormat::NONE,
        ];
        for offset in [c_int::MIN, -8, 0, 8, max] {
            for f in formats {
                let check = |w, h, px| check_size_max_pixels_with_context(w, h, px, f, offset, ctx);
                assert_eq!(check(64, 48, i64::MAX), Ok(()));
                assert_eq!(check(64, 48, 64 * 48), Ok(()));
                assert_eq!(check(64, 48, 64 * 48 - 1), Err(EINVAL));
                assert_eq!(check(64, 48, i64::MIN), Err(EINVAL));
                assert_eq!(check(0, 48, i64::MAX), Err(EINVAL));
                assert_eq!(check(u32::MAX, u32::MAX, i64::MAX), Err(EINVAL));
                for w in [i32::MAX as u32 - 6, i32::MAX as u32, 536_870_911] {
                    assert_eq!(check(w, 1, i64::MAX), Err(EINVAL));
                }
            }
        }
        for offset in [max + 1, c_int::MAX] {
            // Rejected before C runs.
            let r = check_size_max_pixels_with_context(64, 48, i64::MAX, mono, offset, ctx);
            assert_eq!(r, Err(EINVAL));
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn null_context() {
        checks(None);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn context_with_a_class() {
        let class = Box::new(class());
        let ctx = Box::new(Ctx { class: &*class });
        // SAFETY: a live struct led by a pointer to a valid, live class.
        checks(Some(unsafe { object(&*ctx) }));
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn shared_context_on_many_threads() {
        let class = class();
        let ctx = Ctx { class: &class };
        let addr = &raw const ctx as usize;
        thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(move || {
                    // SAFETY: the context and its class outlive the scope
                    // and are only read, which C does too.
                    checks(Some(unsafe { object(addr as *const Ctx) }));
                });
            }
        });
    }
}
