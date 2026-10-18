//! `ObjectMut::set` of an element of a flags array option, whose `+`
//! operand names the evaluator constant `default`.
//!
//! C's `set_string_number` gives `default` the value
//! `DEFAULT_NUMVAL(o)`, which selects `default_val.i64` by comparing
//! `o->type` with the integer types; an array type (`FLAGS | ARRAY`) equals
//! none, so C reads `default_val.dbl`, the bits of the array definition's
//! address: a non-zero subnormal. The wrapper's preview of the operand,
//! which checks that C's unchecked `(int64_t)d` conversion (`opt.c:508`) is
//! defined, evaluated `default` as 0 for an array. So
//! `+1e30*not(not(default))` previews as 0 and passes, and C evaluates it to
//! 1e30 and converts it: UBSan float-cast-overflow.
//!
//! The object is adopted with `ObjectMut::from_ptr` within its contract
//! (a live object of a well-formed class, accessed through the handle
//! only); the defect is in the safe `set`.
//!
//! Build with `CRUSTIFY_FFMPEG_VARIANT=asan-ubsan`,
//! `RUSTFLAGS=-Zsanitizer=address` and run with the sanitized libraries on
//! `LD_LIBRARY_PATH` (see README.md). UBSan recovers by default, so `main`
//! re-runs itself with `UBSAN_OPTIONS=halt_on_error=1` and exits with the
//! child's status: non-zero on the affected commit, zero once the preview
//! gives `default` C's value.

use core::ffi::{c_int, c_uint, c_void};
use core::ptr::{null, null_mut};
use std::env;
use std::process::{Command, exit};

use ffmpeg::libavutil::ffi;
use ffmpeg::libavutil::opt::{ObjectMut, SearchFlags};

/// The object: its class pointer and one array of flags.
#[repr(C)]
struct Obj {
    class: *const ffi::AVClass,
    flags: *mut c_uint,
    nb_flags: c_uint,
}

struct ArrayDef(ffi::AVOptionArrayDef);
// SAFETY: immutable, and holds no pointer.
unsafe impl Sync for ArrayDef {}
struct Options([ffi::AVOption; 2]);
// SAFETY: immutable, and its pointers address literals and statics.
unsafe impl Sync for Options {}
struct Class(ffi::AVClass);
// SAFETY: as above.
unsafe impl Sync for Class {}

/// A definition with no default, no size bounds and the `,` separator.
static DEF: ArrayDef = ArrayDef(ffi::AVOptionArrayDef {
    def: null(),
    size_min: 0,
    size_max: 0,
    sep: 0,
});

static OPTIONS: Options = Options([
    ffi::AVOption {
        name: c"flags".as_ptr(),
        help: null(),
        offset: core::mem::offset_of!(Obj, flags) as c_int,
        type_: ffi::AVOptionType(
            ffi::AVOptionType::AV_OPT_TYPE_FLAGS.0 | ffi::AVOptionType::AV_OPT_TYPE_FLAG_ARRAY.0,
        ),
        default_val: ffi::AVOption__bindgen_ty_1 { arr: &DEF.0 },
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
    class_name: c"crustify-array-default".as_ptr(),
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
        flags: null_mut(),
        nb_flags: 0,
    }))
    .cast();
    {
        // SAFETY: a live object of a well-formed class (one flags array
        // option at its `{ unsigned *, unsigned }` field, empty, with a
        // definition but no default), reached through this handle only
        // while it lives.
        let mut m = unsafe { ObjectMut::from_ptr(obj) }.unwrap();
        let ret = m.set(c"flags", c"+1e30*not(not(default))", SearchFlags::NONE);
        println!("set: {ret:?}");
        m.free_options();
    }
    // SAFETY: `obj` came from `Box::into_raw`, no handle is live, and
    // `free_options` released its array.
    drop(unsafe { Box::from_raw(obj.cast::<Obj>()) });
}
