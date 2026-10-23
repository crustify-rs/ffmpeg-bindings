//! On the affected commit this program does not compile: `OptionRanges` and
//! `OptionDef` carry `define_ctype!`'s `!Send + !Sync` marker and no opt-in,
//! so a queried range list cannot be moved to, dropped on, or shared with
//! another thread, and a table entry's handle cannot be read from one.
//! With the fix it compiles and exits 0.
use core::ffi::c_void;

use ffmpeg::libavutil::libavutil::ffi;
use ffmpeg::libavutil::libavutil::opt::{
    OptionDefRef, OptionDefault, OptionRanges, OptionRangesBox, OptionRangesRef,
};

fn send_sync<T: Send + Sync>() {}

#[repr(C)]
struct Ctx {
    class: *const ffi::AVClass,
    value: i32,
}

struct Table([ffi::AVOption; 2]);
// SAFETY: the table is immutable and its pointers address immutable statics.
unsafe impl Sync for Table {}

static OPTIONS: Table = Table([
    ffi::AVOption {
        name: c"value".as_ptr(),
        help: c"an int".as_ptr(),
        offset: core::mem::offset_of!(Ctx, value) as i32,
        type_: ffi::AVOptionType::AV_OPT_TYPE_INT,
        default_val: ffi::AVOption__bindgen_ty_1 { i64_: 7 },
        min: -10.0,
        max: 100.0,
        flags: 0,
        unit: core::ptr::null(),
    },
    ffi::AVOption {
        name: core::ptr::null(),
        help: core::ptr::null(),
        offset: 0,
        type_: ffi::AVOptionType(0),
        default_val: ffi::AVOption__bindgen_ty_1 { i64_: 0 },
        min: 0.0,
        max: 0.0,
        flags: 0,
        unit: core::ptr::null(),
    },
]);

struct Class(ffi::AVClass);
// SAFETY: the class and its table are immutable statics.
unsafe impl Sync for Class {}

static CLASS: Class = Class(ffi::AVClass {
    class_name: c"repro".as_ptr(),
    item_name: None,
    option: OPTIONS.0.as_ptr(),
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
    send_sync::<OptionRangesBox>();
    send_sync::<OptionRangesRef<'static>>();
    send_sync::<OptionDefRef<'static>>();
    send_sync::<OptionDefault<'static>>();

    let mut ctx = Ctx {
        class: &CLASS.0,
        value: 0,
    };
    // SAFETY: `ctx` is a live object of a well-formed class with no children
    // and no `query_ranges` callback.
    let list: OptionRangesBox =
        unsafe { OptionRanges::query((&raw mut ctx).cast::<c_void>(), c"value", 0) }.unwrap();
    let count = std::thread::scope(|s| {
        let shared = &list;
        s.spawn(move || shared.as_ref().range_count())
            .join()
            .unwrap()
    });
    assert_eq!(count, 1);
    // Dropped on another thread.
    std::thread::spawn(move || drop(list)).join().unwrap();
}
