//! `ObjectMut::remove_array` of one element of a three-element string array
//! while `av_max_alloc` is below the shrunk array's size.
//!
//! C's removal path (`av_opt_set_array` with a NULL `val`, `opt.c:2392`)
//! releases the removed elements and shifts the later ones down, then
//! shrinks the block with `av_realloc_array`; when that fails it returns
//! `AVERROR(ENOMEM)` without storing the new count. The array keeps its old
//! count, so its last slot still holds the string that was shifted into the
//! slot before it, and `av_opt_free` (here `free_options`) frees that string
//! twice: ASan `attempting double-free`. `mem::set_max_alloc` is safe, so the
//! limit is reachable from safe code.
//!
//! The object is adopted with `ObjectMut::from_ptr` within its contract (a
//! live object of a well-formed class, accessed through the handle only);
//! the defect is in the safe `remove_array`.
//!
//! Build with `CRUSTIFY_FFMPEG_VARIANT=asan-ubsan`,
//! `RUSTFLAGS=-Zsanitizer=address` and run with the sanitized libraries on
//! `LD_LIBRARY_PATH` (see README.md): exits non-zero on the affected commit
//! (ASan aborts), zero once the wrapper repairs the count.

use core::ffi::{c_char, c_int, c_uint, c_void};
use core::ptr::{null, null_mut};

use ffmpeg::libavutil::libavutil::ffi;
use ffmpeg::libavutil::libavutil::mem::set_max_alloc;
use ffmpeg::libavutil::libavutil::opt::{ObjectMut, SearchFlags};

/// The object: its class pointer and one array of strings.
#[repr(C)]
struct Obj {
    class: *const ffi::AVClass,
    strs: *mut *mut c_char,
    nb_strs: c_uint,
}

struct Options([ffi::AVOption; 2]);
// SAFETY: immutable, and its pointers address literals and statics.
unsafe impl Sync for Options {}
struct Class(ffi::AVClass);
// SAFETY: as above.
unsafe impl Sync for Class {}

static OPTIONS: Options = Options([
    ffi::AVOption {
        name: c"strs".as_ptr(),
        help: null(),
        offset: core::mem::offset_of!(Obj, strs) as c_int,
        type_: ffi::AVOptionType(
            ffi::AVOptionType::AV_OPT_TYPE_STRING.0 | ffi::AVOptionType::AV_OPT_TYPE_FLAG_ARRAY.0,
        ),
        // No definition: no default, no size bounds, the `,` separator.
        default_val: ffi::AVOption__bindgen_ty_1 { arr: null() },
        min: 0.0,
        max: 0.0,
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
    class_name: c"crustify-remove-array".as_ptr(),
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
    let obj: *mut c_void = Box::into_raw(Box::new(Obj {
        class: &CLASS.0,
        strs: null_mut(),
        nb_strs: 0,
    }))
    .cast();
    {
        // SAFETY: a live object of a well-formed class (one string array
        // option at its `{ char **, unsigned }` field, empty, with no
        // definition), reached through this handle only while it lives.
        let mut m = unsafe { ObjectMut::from_ptr(obj) }.unwrap();
        m.insert_array(c"strs", 0, &[c"a", c"b", c"c"], SearchFlags::NONE)
            .unwrap();
        // Two pointers no longer fit: the shrinking realloc fails.
        set_max_alloc(8);
        let ret = m.remove_array(c"strs", 0, 1, SearchFlags::NONE);
        set_max_alloc(i32::MAX as usize);
        println!("remove_array: {ret:?}");
        let left = m
            .as_ref()
            .get_array::<Option<ffmpeg::libavutil::libavutil::mem::HeapString>>(
                c"strs",
                SearchFlags::NONE,
                0,
                m.as_ref()
                    .get_array_size(c"strs", SearchFlags::NONE)
                    .unwrap() as u32,
            )
            .unwrap();
        let left: Vec<_> = left
            .iter()
            .map(|s| {
                s.as_ref()
                    .map(|s| s.as_c_str().to_string_lossy().into_owned())
            })
            .collect();
        println!("left: {left:?}");
        // Releases every element the count covers.
        m.free_options();
    }
    // SAFETY: `obj` came from `Box::into_raw`, no handle is live, and
    // `free_options` released its array.
    drop(unsafe { Box::from_raw(obj.cast::<Obj>()) });
}
