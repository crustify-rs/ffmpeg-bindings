//! `BareObject::new` of a class whose `item_name` reads its objects: a
//! bare object is only a class pointer (8 bytes), but logging calls the
//! class's `item_name` (and `get_category`) on the object it logs with, so
//! `ObjectRef::show` with the bare object as log context reads past it.
//! ASan reports a heap-buffer-overflow in the callback.
//!
//! FFmpeg has such classes: `hwdevice_ctx_class` has no options, children
//! or offsets, and its `item_name` reads `ctx->hw_type->name`. The class
//! here is a Rust one of the same shape, whose `item_name` reads its
//! device's name field. Its `ClassRef` comes from a live device adopted with
//! `ObjectRef::from_ptr`, within that function's contract; the defect is in
//! the safe `BareObject::new`, which accepted the class.
//!
//! Build with `CRUSTIFY_FFMPEG_VARIANT=asan-ubsan`,
//! `RUSTFLAGS=-Zsanitizer=address`, and run with the sanitized libraries on
//! `LD_LIBRARY_PATH` (see README.md): ASan aborts the affected commit with a
//! non-zero status; the fixed `BareObject::new` refuses the class, and the
//! program exits 0.

use core::ffi::{c_char, c_void};
use core::ptr::null;

use ffmpeg::libavutil::libavutil::ffi;
use ffmpeg::libavutil::libavutil::opt::{BareObject, ObjectRef};

/// A device: its class pointer, then the name its class's `item_name`
/// returns.
#[repr(C)]
struct Device {
    class: *const ffi::AVClass,
    name: *const c_char,
}

/// The class's `item_name`, which reads the device's name.
///
/// # Safety
///
/// `obj` must address a live [`Device`].
unsafe extern "C" fn device_name(obj: *mut c_void) -> *const c_char {
    // SAFETY: C passes the object it logs with, a device of this class.
    unsafe { (*obj.cast::<Device>()).name }
}

struct Class(ffi::AVClass);
// SAFETY: immutable, and its pointers address literals and functions.
unsafe impl Sync for Class {}

static CLASS: Class = Class(ffi::AVClass {
    class_name: c"crustify-device".as_ptr(),
    item_name: Some(device_name),
    option: null(),
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
    let mut device = Device {
        class: &CLASS.0,
        name: c"device0".as_ptr(),
    };
    // SAFETY: a live device of a well-formed class without options, whose
    // `item_name` accepts it; nothing writes it while the handle lives.
    let r = unsafe { ObjectRef::from_ptr((&raw mut device).cast()) }.unwrap();
    let Some(bare) = BareObject::new(r.class()) else {
        println!("BareObject::new refused the class");
        return;
    };
    // On the heap, so that ASan's redzone follows its 8 bytes.
    let bare = Box::new(bare);
    println!("BareObject::new accepted the class; logging through it");
    r.show(Some(bare.as_object()), -1, 0);
}
