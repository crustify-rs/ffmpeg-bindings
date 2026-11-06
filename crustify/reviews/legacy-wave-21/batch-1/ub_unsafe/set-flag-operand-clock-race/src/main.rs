//! `ObjectMut::set` of a `+` flag operand that calls `time()`: the wrapper
//! evaluates the operand once to check that C's unchecked `(int64_t)d`
//! conversion of it (`opt.c:508`) is defined, then C evaluates it again. An
//! operand that reads the clock, `if(gte(time(0),T),1e30,1)`, can be 1 at
//! the check and 1e30 when C converts it, which UBSan reports as a
//! float-cast-overflow. Each attempt sets `T` up to half a millisecond ahead of
//! the clock, so that some attempt straddles it.
//!
//! The object is adopted with `ObjectMut::from_ptr` within its contract
//! (a live object of a well-formed class, accessed through the handle
//! only); the defect is in the safe `set`.
//!
//! Build with `CRUSTIFY_FFMPEG_VARIANT=asan-ubsan`,
//! `RUSTFLAGS=-Zsanitizer=address` and run with the sanitized libraries on
//! `LD_LIBRARY_PATH` (see README.md). UBSan recovers by default, so `main`
//! re-runs itself with `UBSAN_OPTIONS=halt_on_error=1` and exits with the
//! child's status: non-zero on the affected commit, zero once the wrapper
//! refuses operands that read the clock.

use core::ffi::{c_int, c_void};
use core::ptr::null;
use std::env;
use std::ffi::CString;
use std::process::{Command, exit};
use std::time::{SystemTime, UNIX_EPOCH};

use ffmpeg::libavutil::libavutil::ffi;
use ffmpeg::libavutil::libavutil::opt::{ObjectMut, SearchFlags};

/// The object: its class pointer and one flags field.
#[repr(C)]
struct Obj {
    class: *const ffi::AVClass,
    flags: c_int,
}

struct Options([ffi::AVOption; 2]);
// SAFETY: immutable, and its pointers address string literals.
unsafe impl Sync for Options {}
struct Class(ffi::AVClass);
// SAFETY: as above.
unsafe impl Sync for Class {}

static OPTIONS: Options = Options([
    ffi::AVOption {
        name: c"flags".as_ptr(),
        help: null(),
        offset: core::mem::offset_of!(Obj, flags) as c_int,
        type_: ffi::AVOptionType::AV_OPT_TYPE_FLAGS,
        default_val: ffi::AVOption__bindgen_ty_1 { i64_: 0 },
        min: 0.0,
        max: u32::MAX as f64,
        flags: 0,
        unit: null(),
    },
    ffi::AVOption {
        name: null(),
        help: null(),
        offset: 0,
        type_: ffi::AVOptionType(0),
        default_val: ffi::AVOption__bindgen_ty_1 { i64_: 0 },
        min: 0.0,
        max: 0.0,
        flags: 0,
        unit: null(),
    },
]);

static CLASS: Class = Class(ffi::AVClass {
    class_name: c"crustify-clock-race".as_ptr(),
    item_name: Some(ffi::av_default_item_name),
    option: &OPTIONS.0[0],
    version: 0,
    log_level_offset_offset: 0,
    parent_log_context_offset: 0,
    category: ffi::AVClassCategory::AV_CLASS_CATEGORY_NA,
    get_category: None,
    query_ranges: None,
    child_next: None,
    child_class_iterate: None,
    state_flags_offset: 0,
});

fn main() {
    if env::var_os("UBSAN_OPTIONS").is_none() {
        let status = Command::new(env::current_exe().unwrap())
            .env("UBSAN_OPTIONS", "halt_on_error=1:print_stacktrace=1")
            .status()
            .unwrap();
        exit(status.code().unwrap_or(1));
    }
    let obj: *mut c_void = Box::into_raw(Box::new(Obj {
        class: &CLASS.0,
        flags: 0,
    }))
    .cast();
    let (mut refused, mut set) = (0u32, 0u32);
    for attempt in 0..200_000u32 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();
        // The check and C's own evaluation run tens to hundreds of
        // microseconds after `now` in a sanitized debug build.
        let t = now + f64::from(attempt % 500) * 1e-6;
        let val = CString::new(format!("+if(gte(time(0),{t:.6}),1e30,1)")).unwrap();
        // SAFETY: a live object of a well-formed class (one flags option at
        // its field's offset, of `int` storage, bounds in range, no default
        // C parses), reached through this handle only while it lives.
        let mut m = unsafe { ObjectMut::from_ptr(obj) }.unwrap();
        match m.set(c"flags", &val, SearchFlags::NONE) {
            Ok(()) => set += 1,
            Err(_) => refused += 1,
        }
    }
    println!("no straddle: {set} operands set, {refused} refused");
    // SAFETY: `obj` came from `Box::into_raw`, and no handle is live; its
    // only option owns nothing.
    drop(unsafe { Box::from_raw(obj.cast::<Obj>()) });
}
