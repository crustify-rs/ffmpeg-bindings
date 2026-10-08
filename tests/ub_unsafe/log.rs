//! UB tests for `libavutil/log.h`'s [`log_with_context`], called within its
//! `# Safety` contract, through the default log callback.

use core::ffi::{CStr, c_char, c_int, c_void};
use core::mem::offset_of;
use core::ptr;
use std::thread;

use ffmpeg::libavutil::ffi;
use ffmpeg::libavutil::log::log_with_context;

const INFO: c_int = ffi::AV_LOG_INFO as c_int;
const DEBUG: c_int = ffi::AV_LOG_DEBUG as c_int;
const FATAL: c_int = ffi::AV_LOG_FATAL as c_int;

/// The first class version `av_vlog` applies a level offset for.
const OFFSET_VERSION: c_int = (50 << 16) | (15 << 8) | 2;
/// A version the default callback asks for a category.
const CATEGORY_VERSION: c_int = (61 << 16) | (6 << 8) | 100;

/// A context with a level offset and a parent slot, heap-allocated so a read
/// past it reaches the sanitizers.
#[repr(C)]
struct Ctx {
    class: *const ffi::AVClass,
    level_offset: c_int,
    parent: *mut *const ffi::AVClass,
}

impl Ctx {
    fn boxed(class: *const ffi::AVClass, level_offset: c_int) -> Box<Self> {
        Box::new(Self {
            class,
            level_offset,
            parent: ptr::null_mut(),
        })
    }

    fn as_obj(&mut self) -> *mut c_void {
        (self as *mut Self).cast()
    }
}

/// The context's class name, as `av_default_item_name` gives it.
unsafe extern "C" fn item_name(ctx: *mut c_void) -> *const c_char {
    // SAFETY: C passes a live context, whose first member is its class.
    unsafe { (*ctx.cast::<*const ffi::AVClass>().read()).class_name }
}

/// A category read from the context.
unsafe extern "C" fn get_category(_ctx: *mut c_void) -> ffi::AVClassCategory {
    ffi::AVClassCategory::AV_CLASS_CATEGORY_FILTER
}

fn class(version: c_int, offset: bool, parent: bool) -> Box<ffi::AVClass> {
    Box::new(ffi::AVClass {
        class_name: c"crustify-vlog-unsafe".as_ptr(),
        item_name: Some(item_name),
        version,
        log_level_offset_offset: if offset {
            offset_of!(Ctx, level_offset) as c_int
        } else {
            0
        },
        parent_log_context_offset: if parent {
            offset_of!(Ctx, parent) as c_int
        } else {
            0
        },
        get_category: Some(get_category),
        ..Default::default()
    })
}

/// Logs `message` at each of `levels` in `obj`'s context.
///
/// # Safety
///
/// As [`log_with_context`], for every level.
unsafe fn log_all(obj: *mut c_void, levels: &[c_int], message: &CStr) {
    for &level in levels {
        // SAFETY: the caller's contract.
        unsafe { log_with_context(obj, level, message) };
    }
}

#[test]
fn null_context_and_null_class() {
    // SAFETY: NULL is the general log.
    unsafe {
        log_all(
            ptr::null_mut(),
            &[DEBUG, ffi::AV_LOG_TRACE as c_int],
            c"%s%n",
        )
    };
    let mut ctx = Ctx::boxed(ptr::null(), 0);
    // SAFETY: a context whose class pointer is NULL.
    unsafe {
        log_all(
            ctx.as_obj(),
            &[DEBUG, INFO],
            c"crustify ub_unsafe vlog: classless\n",
        )
    };
}

#[test]
fn level_offsets_up_to_the_overflow_bound() {
    let cls = class(OFFSET_VERSION, true, false);
    let trace = ffi::AV_LOG_TRACE as c_int;
    for offset in [c_int::MIN, -1000, -FATAL, 0, 1, 1000, c_int::MAX - trace] {
        let mut ctx = Ctx::boxed(&*cls, offset);
        // SAFETY: `level + offset` stays within `c_int` for these levels, from
        // `FATAL + c_int::MIN` up to exactly `c_int::MAX`.
        unsafe {
            log_all(
                ctx.as_obj(),
                &[FATAL, DEBUG, trace],
                c"crustify ub_unsafe vlog: %%%s\n",
            )
        };
    }
    let mut ctx = Ctx::boxed(&*cls, c_int::MAX - 0xff);
    // SAFETY: levels below `AV_LOG_FATAL` take no offset, and `0xff` sums to
    // exactly `c_int::MAX`.
    unsafe {
        log_all(
            ctx.as_obj(),
            &[ffi::AV_LOG_PANIC as c_int, ffi::AV_LOG_QUIET - 1000, 0xff],
            c"crustify ub_unsafe vlog: below FATAL\n",
        )
    };
}

#[test]
fn offsets_of_old_classes_are_ignored() {
    let old = class(OFFSET_VERSION - 1, true, false);
    let mut ctx = Ctx::boxed(&*old, c_int::MAX);
    // SAFETY: the class predates level offsets, so the `c_int::MAX` offset is
    // never added; the levels the default callback drops.
    unsafe { log_all(ctx.as_obj(), &[DEBUG, c_int::MAX], c"old class") };
}

#[test]
fn printed_lines_name_the_context_and_its_parent() {
    let parent_class = class(CATEGORY_VERSION, false, false);
    let mut parent: *const ffi::AVClass = &*parent_class;
    let child_class = class(CATEGORY_VERSION, true, true);
    let mut ctx = Ctx::boxed(&*child_class, -8);
    for slot in [ptr::null_mut(), &mut parent as *mut _] {
        ctx.parent = slot;
        // SAFETY: a well-formed context whose parent slot is NULL or points
        // at a classed parent; the offset keeps every level in range.
        unsafe {
            log_all(
                ctx.as_obj(),
                &[INFO + 8, DEBUG],
                c"crustify ub_unsafe vlog: child\n",
            )
        };
    }
    let mut null_parent: *const ffi::AVClass = ptr::null();
    ctx.parent = &mut null_parent;
    // SAFETY: the parent slot points to a NULL class pointer.
    unsafe {
        log_all(
            ctx.as_obj(),
            &[INFO + 8],
            c"crustify ub_unsafe vlog: orphan\n",
        )
    };
}

#[test]
fn one_context_logged_from_many_threads() {
    let cls = class(CATEGORY_VERSION, true, false);
    let ctx = Ctx::boxed(&*cls, 16);
    let addr = Box::into_raw(ctx) as usize;
    let workers: Vec<_> = (0..8)
        .map(|i| {
            thread::spawn(move || {
                for j in 0..200 {
                    let level = if i == 0 && j == 0 { FATAL } else { DEBUG };
                    // SAFETY: the context and class outlive the threads,
                    // which only read them; the offset keeps levels in range.
                    unsafe {
                        log_with_context(
                            addr as *mut c_void,
                            level,
                            c"crustify ub_unsafe vlog: threads\n",
                        )
                    };
                }
            })
        })
        .collect();
    for w in workers {
        w.join().unwrap();
    }
    // SAFETY: `addr` came from `Box::into_raw`, and the threads are done.
    drop(unsafe { Box::from_raw(addr as *mut Ctx) });
    drop(cls);
}
