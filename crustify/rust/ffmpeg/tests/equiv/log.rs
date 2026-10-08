//! Equivalence of `libavutil/log.h`'s category and state-flag values with the
//! raw C enums, as C reads them back from independently owned `AVClass`
//! contexts.

use core::ffi::{c_int, c_uint, c_void};
use core::mem::offset_of;
use core::ptr::addr_of_mut;

use ffmpeg::libavutil::ffi;
use ffmpeg::libavutil::log::{ClassCategory, ClassStateFlags};

/// A logging context: an `AVClass` pointer first, as every C context has, then
/// the state flags `state_flags_offset` locates and one non-runtime option.
#[repr(C)]
struct Ctx {
    class: *const ffi::AVClass,
    state_flags: c_uint,
    value: c_int,
}

/// The option table of [`class`]: one `int` option, `value`, settable only
/// before initialization (no `AV_OPT_FLAG_RUNTIME_PARAM`), then the NULL
/// terminator.
fn options() -> [ffi::AVOption; 2] {
    let value = ffi::AVOption {
        name: c"value".as_ptr(),
        offset: offset_of!(Ctx, value) as c_int,
        type_: ffi::AVOptionType::AV_OPT_TYPE_INT,
        min: f64::from(c_int::MIN),
        max: f64::from(c_int::MAX),
        ..Default::default()
    };
    [value, ffi::AVOption::default()]
}

/// A class reporting `category`, with `options` and the context's state flags.
fn class(category: ffi::AVClassCategory, options: &[ffi::AVOption; 2]) -> ffi::AVClass {
    ffi::AVClass {
        class_name: c"crustify-log-equiv".as_ptr(),
        option: options.as_ptr(),
        category,
        state_flags_offset: offset_of!(Ctx, state_flags) as c_int,
        ..Default::default()
    }
}

/// `av_default_get_category` over a context of `class`.
fn c_category(class: &ffi::AVClass) -> ffi::AVClassCategory {
    let mut ctx = Ctx {
        class,
        state_flags: 0,
        value: 0,
    };
    // SAFETY: `ctx` starts with a pointer to `class`, which outlives the call;
    // `av_default_get_category` only reads `class->category` through it.
    unsafe { ffi::av_default_get_category(addr_of_mut!(ctx).cast::<c_void>()) }
}

/// `av_opt_set_int(ctx, "value", value, 0)`, and the value it leaves stored.
fn c_set_value(ctx: &mut Ctx, value: i64) -> (c_int, c_int) {
    // SAFETY: `ctx` is a live context whose class, options and state flags
    // outlive the call; C writes only `ctx.value`, at the option's offset.
    let ret = unsafe { ffi::av_opt_set_int((ctx as *mut Ctx).cast(), c"value".as_ptr(), value, 0) };
    (ret, ctx.value)
}

/// Every category with a constant, against its raw C enumerator.
const CATEGORIES: [(ClassCategory, ffi::AVClassCategory); 18] = [
    (
        ClassCategory::NA,
        ffi::AVClassCategory::AV_CLASS_CATEGORY_NA,
    ),
    (
        ClassCategory::INPUT,
        ffi::AVClassCategory::AV_CLASS_CATEGORY_INPUT,
    ),
    (
        ClassCategory::OUTPUT,
        ffi::AVClassCategory::AV_CLASS_CATEGORY_OUTPUT,
    ),
    (
        ClassCategory::MUXER,
        ffi::AVClassCategory::AV_CLASS_CATEGORY_MUXER,
    ),
    (
        ClassCategory::DEMUXER,
        ffi::AVClassCategory::AV_CLASS_CATEGORY_DEMUXER,
    ),
    (
        ClassCategory::ENCODER,
        ffi::AVClassCategory::AV_CLASS_CATEGORY_ENCODER,
    ),
    (
        ClassCategory::DECODER,
        ffi::AVClassCategory::AV_CLASS_CATEGORY_DECODER,
    ),
    (
        ClassCategory::FILTER,
        ffi::AVClassCategory::AV_CLASS_CATEGORY_FILTER,
    ),
    (
        ClassCategory::BITSTREAM_FILTER,
        ffi::AVClassCategory::AV_CLASS_CATEGORY_BITSTREAM_FILTER,
    ),
    (
        ClassCategory::SWSCALER,
        ffi::AVClassCategory::AV_CLASS_CATEGORY_SWSCALER,
    ),
    (
        ClassCategory::SWRESAMPLER,
        ffi::AVClassCategory::AV_CLASS_CATEGORY_SWRESAMPLER,
    ),
    (
        ClassCategory::HWDEVICE,
        ffi::AVClassCategory::AV_CLASS_CATEGORY_HWDEVICE,
    ),
    (
        ClassCategory::DEVICE_VIDEO_OUTPUT,
        ffi::AVClassCategory::AV_CLASS_CATEGORY_DEVICE_VIDEO_OUTPUT,
    ),
    (
        ClassCategory::DEVICE_VIDEO_INPUT,
        ffi::AVClassCategory::AV_CLASS_CATEGORY_DEVICE_VIDEO_INPUT,
    ),
    (
        ClassCategory::DEVICE_AUDIO_OUTPUT,
        ffi::AVClassCategory::AV_CLASS_CATEGORY_DEVICE_AUDIO_OUTPUT,
    ),
    (
        ClassCategory::DEVICE_AUDIO_INPUT,
        ffi::AVClassCategory::AV_CLASS_CATEGORY_DEVICE_AUDIO_INPUT,
    ),
    (
        ClassCategory::DEVICE_OUTPUT,
        ffi::AVClassCategory::AV_CLASS_CATEGORY_DEVICE_OUTPUT,
    ),
    (
        ClassCategory::DEVICE_INPUT,
        ffi::AVClassCategory::AV_CLASS_CATEGORY_DEVICE_INPUT,
    ),
];

#[test]
fn named_categories_read_back_through_c() {
    let options = options();
    for (category, raw) in CATEGORIES {
        let c = c_category(&class(raw, &options));
        let rust = c_category(&class(category.into(), &options));
        assert_eq!(rust, c, "{category:?}");
        assert_eq!(ClassCategory::from(rust), category);
        assert_eq!(ClassCategory::from(c).value(), raw.0);
    }
}

#[test]
fn unnamed_categories_read_back_through_c() {
    let options = options();
    // The gap below the device categories, `AV_CLASS_CATEGORY_NB` and beyond:
    // C stores and returns them unchecked.
    for v in [12, 39, 46, 1000, c_uint::MAX] {
        let c = c_category(&class(ffi::AVClassCategory(v), &options));
        let rust = ClassCategory::from(c_category(&class(
            ClassCategory::from_value(v).into(),
            &options,
        )));
        assert_eq!(rust.value(), c.0);
        assert_eq!(rust.name(), None);
    }
}

/// One write to the Rust flag set.
type Step = fn(&mut ClassStateFlags);

#[test]
fn state_flags_gate_non_runtime_options_as_in_c() {
    let options = options();
    let class = class(ffi::AVClassCategory::AV_CLASS_CATEGORY_NA, &options);
    let initialized = ffi::AVClassStateFlags::AV_CLASS_STATE_INITIALIZED.0;
    let mut c = Ctx {
        class: &class,
        state_flags: 0,
        value: 0,
    };
    let mut rust = Ctx {
        class: &class,
        state_flags: 0,
        value: 0,
    };
    let mut flags = ClassStateFlags::empty();

    // Each step: the raw C bits next to the Rust set's, then the same write.
    let steps: [(Step, c_uint); 7] = [
        (|_| {}, 0),
        (|f| f.insert(ClassStateFlags::INITIALIZED), initialized),
        (
            |f| *f |= ClassStateFlags::from_bits_retain(0x80),
            initialized | 0x80,
        ),
        (|f| f.remove(ClassStateFlags::INITIALIZED), 0x80),
        (
            |f| f.set(ClassStateFlags::INITIALIZED, true),
            initialized | 0x80,
        ),
        (|f| *f &= ClassStateFlags::from_bits_retain(0x80), 0x80),
        (
            |f| *f = ClassStateFlags::from_bits_truncate(c_uint::MAX),
            initialized,
        ),
    ];
    for (i, (step, c_bits)) in steps.into_iter().enumerate() {
        step(&mut flags);
        c.state_flags = c_bits;
        rust.state_flags = flags.bits();
        assert_eq!(rust.state_flags, c.state_flags, "step {i}");

        let value = 100 + i as i64;
        let (c_ret, c_value) = c_set_value(&mut c, value);
        let (rust_ret, rust_value) = c_set_value(&mut rust, value);
        assert_eq!(rust_ret, c_ret, "step {i}");
        assert_eq!(rust_value, c_value, "step {i}");
        // C refuses exactly when the initialized bit is set.
        assert_eq!(
            c_ret < 0,
            flags.contains(ClassStateFlags::INITIALIZED),
            "step {i}"
        );
    }
}
