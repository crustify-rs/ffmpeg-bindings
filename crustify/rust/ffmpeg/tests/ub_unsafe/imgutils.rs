//! UB tests for `libavutil/imgutils.h`'s unsafe `check_size_with_context`,
//! each call within its `# Safety` contract: a NULL parent context, a
//! class-less one, and one led by a valid class, with level offsets up to
//! the overflow bound, on valid and invalid sizes, on several threads.

use core::ffi::{c_int, c_void};
use core::ptr;
use std::thread;

use ffmpeg::libavutil::ffi;
use ffmpeg::libavutil::imgutils::check_size_with_context;

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
///
/// # Safety
///
/// `ctx` must be valid for `check_size_with_context` throughout.
unsafe fn checks(ctx: *mut c_void) {
    let max = c_int::MAX - ffi::AV_LOG_ERROR as c_int;
    for offset in [c_int::MIN, -8, 0, 8, max] {
        // SAFETY: the caller vouches for `ctx`.
        unsafe {
            assert_eq!(check_size_with_context(64, 48, offset, ctx), Ok(()));
            assert_eq!(check_size_with_context(0, 48, offset, ctx), Err(EINVAL));
            assert_eq!(
                check_size_with_context(u32::MAX, u32::MAX, offset, ctx),
                Err(EINVAL)
            );
        }
    }
    for offset in [max + 1, c_int::MAX] {
        // SAFETY: as above; rejected before C runs.
        unsafe {
            assert_eq!(check_size_with_context(64, 48, offset, ctx), Err(EINVAL));
            assert_eq!(check_size_with_context(0, 0, offset, ctx), Err(EINVAL));
        }
    }
}

#[test]
fn null_context() {
    // SAFETY: NULL is "no parent context".
    unsafe { checks(ptr::null_mut()) };
}

#[test]
fn classless_context() {
    let mut ctx = Box::new(Ctx { class: ptr::null() });
    // SAFETY: a live struct led by a NULL class pointer, unwritten while
    // the checks run.
    unsafe { checks((&raw mut *ctx).cast()) };
}

#[test]
fn context_with_a_class() {
    let class = Box::new(class());
    let mut ctx = Box::new(Ctx { class: &*class });
    // SAFETY: a live struct led by a pointer to a valid, live class.
    unsafe { checks((&raw mut *ctx).cast()) };
}

#[test]
fn shared_context_on_many_threads() {
    let class = class();
    let ctx = Ctx { class: &class };
    let addr = &raw const ctx as usize;
    thread::scope(|s| {
        for _ in 0..4 {
            s.spawn(move || {
                // SAFETY: the context and its class outlive the scope and
                // are only read, which C does too.
                unsafe { checks(addr as *mut c_void) };
            });
        }
    });
}

/// UB tests for `check_size_max_pixels_with_context`, each call within its
/// `# Safety` contract: NULL, class-less and classed parent contexts, with
/// level offsets up to the overflow bound, on sizes valid and invalid by
/// dimension and by pixel count, for formats with and without linesizes,
/// including monochrome widths whose linesize overflows in C, on several
/// threads.
mod max_pixels {
    use core::ffi::{c_int, c_void};
    use core::ptr;
    use std::thread;

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::imgutils::check_size_max_pixels_with_context;
    use ffmpeg::libavutil::pixfmt::PixelFormat;

    use super::{Ctx, EINVAL, class};

    /// The checks every context runs.
    ///
    /// # Safety
    ///
    /// `ctx` must be valid for `check_size_max_pixels_with_context`
    /// throughout.
    unsafe fn checks(ctx: *mut c_void) {
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
                // SAFETY: the caller vouches for `ctx`.
                unsafe {
                    let check =
                        |w, h, px| check_size_max_pixels_with_context(w, h, px, f, offset, ctx);
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
        }
        for offset in [max + 1, c_int::MAX] {
            // SAFETY: as above; rejected before C runs.
            unsafe {
                let r = check_size_max_pixels_with_context(64, 48, i64::MAX, mono, offset, ctx);
                assert_eq!(r, Err(EINVAL));
            }
        }
    }

    #[test]
    fn null_context() {
        // SAFETY: NULL is "no parent context".
        unsafe { checks(ptr::null_mut()) };
    }

    #[test]
    fn classless_context() {
        let mut ctx = Box::new(Ctx { class: ptr::null() });
        // SAFETY: a live struct led by a NULL class pointer, unwritten while
        // the checks run.
        unsafe { checks((&raw mut *ctx).cast()) };
    }

    #[test]
    fn context_with_a_class() {
        let class = Box::new(class());
        let mut ctx = Box::new(Ctx { class: &*class });
        // SAFETY: a live struct led by a pointer to a valid, live class.
        unsafe { checks((&raw mut *ctx).cast()) };
    }

    #[test]
    fn shared_context_on_many_threads() {
        let class = class();
        let ctx = Ctx { class: &class };
        let addr = &raw const ctx as usize;
        thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(move || {
                    // SAFETY: the context and its class outlive the scope
                    // and are only read, which C does too.
                    unsafe { checks(addr as *mut c_void) };
                });
            }
        });
    }
}
