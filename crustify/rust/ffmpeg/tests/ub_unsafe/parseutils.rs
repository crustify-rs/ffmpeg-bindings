//! UB tests for `libavutil/parseutils.h`'s unsafe `parse_color_with_context`,
//! called within its `# Safety` contract: NULL, classless and classed log
//! contexts, whole strings and prefixes, valid and failing colors.

use core::ffi::{CStr, c_void};
use core::ptr::{null, null_mut};
use std::ffi::CString;

use ffmpeg::libavutil::ffi;
use ffmpeg::libavutil::parseutils::parse_color_with_context;

const EINVAL: i32 = -22;

/// A logging context: a struct whose first member is its class pointer.
#[repr(C)]
struct Ctx {
    class: *const ffi::AVClass,
    _payload: [u64; 2],
}

/// A class whose `item_name` reads the context, as `av_log` lets it.
struct Class(ffi::AVClass);
// SAFETY: the class is immutable and its pointers address immutable statics.
unsafe impl Sync for Class {}

unsafe extern "C" fn item_name(ctx: *mut c_void) -> *const core::ffi::c_char {
    // SAFETY: C passes the live context the class belongs to.
    let payload = unsafe { (*ctx.cast::<Ctx>())._payload };
    assert_eq!(payload, [1, 2]);
    c"crustify-color-ctx".as_ptr()
}

static CLASS: Class = Class(ffi::AVClass {
    class_name: c"crustify-color".as_ptr(),
    item_name: Some(item_name),
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

const COLORS: [&CStr; 14] = [
    c"red",
    c"AliceBlue@0.5",
    c"#ff000080",
    c"0x00ff00@0x40",
    c"random",
    c"bikeshed@1",
    c"nosuchcolor",
    c"#12345",
    c"red@2",
    c"red@nan",
    c"#",
    c"0x",
    c"",
    c"@",
];

/// Every color, whole and at every prefix length (plus a few past it), with
/// `log_ctx`.
fn sweep(log_ctx: *mut c_void) {
    for color in COLORS {
        let exact = CString::from(color).into_boxed_c_str();
        let lens = (0..=color.count_bytes() + 2)
            .map(Some)
            .chain([None, Some(usize::MAX)]);
        for len in lens {
            // SAFETY: the caller vouches for `log_ctx`.
            let ret = unsafe { parse_color_with_context(&exact, len, log_ctx) };
            if let Err(e) = ret {
                assert_eq!(e, EINVAL, "{color:?} {len:?}");
            }
        }
    }
}

#[test]
fn null_context() {
    sweep(null_mut());
}

#[test]
fn classless_context() {
    let mut ctx = Ctx {
        class: null(),
        _payload: [1, 2],
    };
    sweep((&raw mut ctx).cast());
}

#[test]
fn classed_context() {
    let mut ctx = Ctx {
        class: &CLASS.0,
        _payload: [1, 2],
    };
    sweep((&raw mut ctx).cast());
    // A boxed context, freed after the calls.
    let ctx = Box::into_raw(Box::new(Ctx {
        class: &CLASS.0,
        _payload: [1, 2],
    }));
    sweep(ctx.cast());
    // SAFETY: `ctx` came from `Box::into_raw`, and C kept no pointer to it.
    drop(unsafe { Box::from_raw(ctx) });
}

/// Long colors: C copies at most 127 bytes after the marker, so the alpha
/// specifier is cut off or missed, never read past.
#[test]
fn long_colors() {
    let mut ctx = Ctx {
        class: &CLASS.0,
        _payload: [1, 2],
    };
    for n in [120, 126, 127, 128, 129, 1000] {
        for (prefix, suffix) in [("", "@nan"), ("#", "@0.5"), ("0x", "@0x80"), ("red", "")] {
            let s = format!("{prefix}{}{suffix}", "f".repeat(n));
            let s = CString::new(s).unwrap().into_boxed_c_str();
            for len in [None, Some(n), Some(n + 3), Some(usize::MAX)] {
                // SAFETY: `ctx` is a live context of a valid class.
                let ret = unsafe { parse_color_with_context(&s, len, (&raw mut ctx).cast()) };
                assert_eq!(ret, Err(EINVAL), "{n} {prefix:?} {suffix:?} {len:?}");
            }
        }
    }
}
