//! Equivalence of `libavutil/log.h`'s category and state-flag values with the
//! raw C enums, as C reads them back from independently owned `AVClass`
//! contexts, and of the logging functions and callback records with the C
//! functions they wrap.
#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    trivial_numeric_casts,
    reason = "test inputs are chosen within range, and the casts mirror the C conversions under comparison"
)]
#![expect(
    clippy::redundant_closure_for_method_calls,
    reason = "each test is one scenario, its helpers and steps kept where they are used"
)]
#![expect(
    clippy::assert_is_empty,
    reason = "assertions keep the form of the C behaviour they check"
)]
#![expect(
    clippy::decimal_bitwise_operands,
    reason = "constants as the C sources and specifications spell them"
)]

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::mem::offset_of;
use core::ptr::{self, addr_of_mut};
use std::cell::RefCell;
use std::sync::{Mutex, MutexGuard, PoisonError};

use ffmpeg_bindings::libavutil::libavutil::ffi;
use ffmpeg_bindings::libavutil::libavutil::log::{
    self, Callback, ClassCategory, ClassStateFlags, Flags, Level, Record,
};

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
    let ret = unsafe {
        ffi::av_opt_set_int(
            ptr::from_mut::<Ctx>(ctx).cast(),
            c"value".as_ptr(),
            value,
            0,
        )
    };
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
#[cfg_attr(miri, ignore = "calls C")]
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
#[cfg_attr(miri, ignore = "calls C")]
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
#[cfg_attr(miri, ignore = "calls C")]
fn state_flags_gate_non_runtime_options_as_in_c() {
    let options = options();
    let class = class(ffi::AVClassCategory::AV_CLASS_CATEGORY_NA, &options);
    let initialized = ffi::AVClassStateFlags::AV_CLASS_STATE_INITIALIZED.0;
    let mut c = Ctx {
        class: &raw const class,
        state_flags: 0,
        value: 0,
    };
    let mut rust = Ctx {
        class: &raw const class,
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

/// A child context: an `AVClass` pointer, then the option the parent's
/// search reaches through `child_next`; its class has no state flags.
#[repr(C)]
struct Child {
    class: *const ffi::AVClass,
    value: c_int,
}

/// A parent context: its class finds [`Child`] through `child_next`, and
/// carries the state flags `state_flags_offset` locates; it has no option
/// of its own.
#[repr(C)]
struct Parent {
    class: *const ffi::AVClass,
    state_flags: c_uint,
    child: *mut Child,
}

/// `child_next` of the parent class: the one child, then no more.
unsafe extern "C" fn parent_child_next(obj: *mut c_void, prev: *mut c_void) -> *mut c_void {
    if !prev.is_null() {
        return ptr::null_mut();
    }
    // SAFETY: C passes the parent context `av_opt_set_int` received, a live
    // `Parent`; only its `child` pointer is read.
    unsafe { (*obj.cast::<Parent>()).child.cast() }
}

/// The option table of the child's class: `value`, settable only before
/// initialization, then the NULL terminator.
fn child_options() -> [ffi::AVOption; 2] {
    let value = ffi::AVOption {
        name: c"value".as_ptr(),
        offset: offset_of!(Child, value) as c_int,
        type_: ffi::AVOptionType::AV_OPT_TYPE_INT,
        min: f64::from(c_int::MIN),
        max: f64::from(c_int::MAX),
        ..Default::default()
    };
    [value, ffi::AVOption::default()]
}

/// The child's class, with `options` and, when `with_flags`, state flags
/// located at the child's `value` member reinterpreted as the flag word.
fn child_class(options: &[ffi::AVOption; 2], with_flags: bool) -> ffi::AVClass {
    ffi::AVClass {
        class_name: c"crustify-log-equiv-child".as_ptr(),
        option: options.as_ptr(),
        state_flags_offset: if with_flags {
            offset_of!(Child, value) as c_int
        } else {
            0
        },
        ..Default::default()
    }
}

/// The parent's class: no options, the child through `child_next`, and
/// the state flags at the parent's `state_flags` member.
fn parent_class() -> ffi::AVClass {
    ffi::AVClass {
        class_name: c"crustify-log-equiv-parent".as_ptr(),
        child_next: Some(parent_child_next),
        state_flags_offset: offset_of!(Parent, state_flags) as c_int,
        ..Default::default()
    }
}

/// `av_opt_set_int(parent, "value", value, AV_OPT_SEARCH_CHILDREN)`, and the
/// value the child is left holding.
fn c_set_child_value(parent: &mut Parent, value: i64) -> (c_int, c_int) {
    // SAFETY: `parent` is a live context whose class, child, the child's
    // class and options, and both state-flag words outlive the call; C
    // writes only the child's `value`, at the option's offset.
    let ret = unsafe {
        ffi::av_opt_set_int(
            ptr::from_mut::<Parent>(parent).cast(),
            c"value".as_ptr(),
            value,
            ffi::AV_OPT_SEARCH_CHILDREN as c_int,
        )
    };
    // SAFETY: `parent.child` addresses a live `Child` for the whole test.
    (ret, unsafe { (*parent.child).value })
}

/// One parent/child pair of contexts for [`c_set_child_value`]. The child
/// stays behind the raw pointer the parent stores, so no Rust reference
/// to it is formed while C reads and writes it through that pointer.
struct Family {
    parent: Parent,
}

impl Family {
    fn new(parent_class: &ffi::AVClass, child_class: &ffi::AVClass) -> Self {
        let child = Box::into_raw(Box::new(Child {
            class: child_class,
            value: 0,
        }));
        Self {
            parent: Parent {
                class: parent_class,
                state_flags: 0,
                child,
            },
        }
    }

    /// The child's `value` member.
    fn child_value(&self) -> c_int {
        // SAFETY: `child` is the live `Child` `new` leaked, freed only by `drop`.
        unsafe { (*self.parent.child).value }
    }

    fn set_child_value(&mut self, value: c_int) {
        // SAFETY: as for `child_value`.
        unsafe { (*self.parent.child).value = value }
    }
}

impl Drop for Family {
    fn drop(&mut self) {
        // SAFETY: `child` came from `Box::into_raw` in `new` and is reclaimed
        // once, here.
        drop(unsafe { Box::from_raw(self.parent.child) });
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn state_flags_of_the_parent_gate_a_child_option_as_in_c() {
    let options = child_options();
    let parent_class = parent_class();
    let child_class = child_class(&options, false);
    let mut c = Family::new(&parent_class, &child_class);
    let mut rust = Family::new(&parent_class, &child_class);
    let initialized = ffi::AVClassStateFlags::AV_CLASS_STATE_INITIALIZED.0;
    let mut flags = ClassStateFlags::empty();

    // The child's class has no state flags, so C reads the parent's: each
    // step writes the raw C bits next to the Rust set's.
    let steps: [(Step, c_uint); 5] = [
        (|_| {}, 0),
        (|f| f.insert(ClassStateFlags::INITIALIZED), initialized),
        (
            |f| *f |= ClassStateFlags::from_bits_retain(0x80),
            initialized | 0x80,
        ),
        (|f| f.remove(ClassStateFlags::INITIALIZED), 0x80),
        (|f| *f = ClassStateFlags::all(), initialized),
    ];
    for (i, (step, c_bits)) in steps.into_iter().enumerate() {
        step(&mut flags);
        c.parent.state_flags = c_bits;
        rust.parent.state_flags = flags.bits();
        assert_eq!(rust.parent.state_flags, c.parent.state_flags, "step {i}");

        let value = 200 + i as i64;
        let (c_ret, c_value) = c_set_child_value(&mut c.parent, value);
        let (rust_ret, rust_value) = c_set_child_value(&mut rust.parent, value);
        assert_eq!(rust_ret, c_ret, "step {i}");
        assert_eq!(rust_value, c_value, "step {i}");
        assert_eq!(rust.child_value(), c.child_value(), "step {i}");
        assert_eq!(
            c_ret < 0,
            flags.contains(ClassStateFlags::INITIALIZED),
            "step {i}"
        );
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn state_flags_of_the_child_take_precedence_over_the_parent_as_in_c() {
    let options = child_options();
    let parent_class = parent_class();
    let child_class = child_class(&options, true);
    let mut c = Family::new(&parent_class, &child_class);
    let mut rust = Family::new(&parent_class, &child_class);
    let initialized = ffi::AVClassStateFlags::AV_CLASS_STATE_INITIALIZED.0;

    // The child's flag word is its `value` member: an initialized child
    // refuses the write whatever the parent says, and an uninitialized one
    // accepts it even when the parent is initialized. The write lands in
    // the flag word itself, so each step resets both contexts.
    let steps: [(ClassStateFlags, c_uint, ClassStateFlags, c_uint); 4] = [
        (ClassStateFlags::empty(), 0, ClassStateFlags::empty(), 0),
        (
            ClassStateFlags::INITIALIZED,
            initialized,
            ClassStateFlags::empty(),
            0,
        ),
        (
            ClassStateFlags::empty(),
            0,
            ClassStateFlags::INITIALIZED,
            initialized,
        ),
        (
            ClassStateFlags::INITIALIZED | ClassStateFlags::from_bits_retain(0x80),
            initialized | 0x80,
            ClassStateFlags::INITIALIZED,
            initialized,
        ),
    ];
    for (i, (child, c_child, parent, c_parent)) in steps.into_iter().enumerate() {
        c.set_child_value(c_child as c_int);
        c.parent.state_flags = c_parent;
        rust.set_child_value(child.bits() as c_int);
        rust.parent.state_flags = parent.bits();
        assert_eq!(rust.child_value(), c.child_value(), "step {i}");
        assert_eq!(rust.parent.state_flags, c.parent.state_flags, "step {i}");

        let value = 300 + i as i64;
        let (c_ret, c_value) = c_set_child_value(&mut c.parent, value);
        let (rust_ret, rust_value) = c_set_child_value(&mut rust.parent, value);
        assert_eq!(rust_ret, c_ret, "step {i}");
        assert_eq!(rust_value, c_value, "step {i}");
        assert_eq!(
            c_ret < 0,
            child.contains(ClassStateFlags::INITIALIZED),
            "step {i}"
        );
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn without_state_flags_nothing_gates_an_option_as_in_c() {
    let initialized = ffi::AVClassStateFlags::AV_CLASS_STATE_INITIALIZED.0;
    let bits = [0, initialized, initialized | 0x80];

    // A direct context whose class locates no state flags: the word it
    // holds is never read, whatever it says.
    let options = options();
    let mut class = class(ffi::AVClassCategory::AV_CLASS_CATEGORY_NA, &options);
    class.state_flags_offset = 0;
    let mut c = Ctx {
        class: &raw const class,
        state_flags: 0,
        value: 0,
    };
    let mut rust = Ctx {
        class: &raw const class,
        state_flags: 0,
        value: 0,
    };
    for (i, raw) in bits.into_iter().enumerate() {
        c.state_flags = raw;
        rust.state_flags = ClassStateFlags::from_bits_retain(raw).bits();
        let value = 400 + i as i64;
        let (c_ret, c_value) = c_set_value(&mut c, value);
        let (rust_ret, rust_value) = c_set_value(&mut rust, value);
        assert_eq!((rust_ret, rust_value), (c_ret, c_value), "direct, step {i}");
        assert_eq!(c_ret, 0, "direct, step {i}");
    }

    // A child reached through a parent, neither class locating state flags.
    let child_options = child_options();
    let mut parent_class = parent_class();
    parent_class.state_flags_offset = 0;
    let child_class = child_class(&child_options, false);
    let mut c = Family::new(&parent_class, &child_class);
    let mut rust = Family::new(&parent_class, &child_class);
    for (i, raw) in bits.into_iter().enumerate() {
        c.parent.state_flags = raw;
        rust.parent.state_flags = ClassStateFlags::from_bits_retain(raw).bits();
        let value = 500 + i as i64;
        let (c_ret, c_value) = c_set_child_value(&mut c.parent, value);
        let (rust_ret, rust_value) = c_set_child_value(&mut rust.parent, value);
        assert_eq!((rust_ret, rust_value), (c_ret, c_value), "child, step {i}");
        assert_eq!(c_ret, 0, "child, step {i}");
    }
}

/// Raw category values at and around every boundary the device predicates
/// distinguish, plus the gap below the device range.
const DEVICE_EDGES: [c_uint; 12] = [0, 11, 12, 39, 40, 41, 42, 43, 44, 45, 46, c_uint::MAX];

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn device_predicates_match_the_c_macros() {
    let named = CATEGORIES.iter().map(|&(_, raw)| raw.0);
    for v in named.chain(DEVICE_EDGES) {
        let raw = ffi::AVClassCategory(v);
        // SAFETY: the macro shims compare a value; every category is valid.
        let (c_input, c_output) = (
            unsafe { ffi::crustify_AV_IS_INPUT_DEVICE(raw) } != 0,
            unsafe { ffi::crustify_AV_IS_OUTPUT_DEVICE(raw) } != 0,
        );
        let category = ClassCategory::from_value(v);
        assert_eq!(category.is_input_device(), c_input, "{category:?}");
        assert_eq!(category.is_output_device(), c_output, "{category:?}");
        // The macros name disjoint sets within the device range.
        assert!(!(c_input && c_output), "{category:?}");
        assert_eq!(c_input || c_output, (40..=45).contains(&v), "{category:?}");
    }
}

/// `av_vlog` through [`log`] and [`log_with_context`], against `av_log`
/// with the format `%s`, as a capturing log callback observes both.
mod vlog {
    use core::ffi::{CStr, c_char, c_int, c_void};
    use core::mem::offset_of;
    use std::sync::{Mutex, PoisonError};

    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::log::{Level, log, log_with_context};
    use ffmpeg_bindings::libavutil::libavutil::opt::ObjectRef;

    use super::LogState;

    /// Tags the lines these tests log, so the capture ignores any other test's.
    const TAG: &[u8] = b"crustify-vlog-equiv";

    /// What the callback saw: the context, the level and the formatted line.
    type Entry = (usize, c_int, Vec<u8>);

    static CAPTURED: Mutex<Vec<Entry>> = Mutex::new(Vec::new());

    /// A log callback recording each tagged line, prefix included.
    unsafe extern "C" fn capture(
        avcl: *mut c_void,
        level: c_int,
        fmt: *const c_char,
        vl: *mut ffi::__va_list_tag,
    ) {
        let mut line = [0 as c_char; 1024];
        let mut print_prefix = 1;
        // SAFETY: C hands the callback a context `av_log_format_line2` can
        // read as `av_vlog` did, a format and its `va_list`; `line` is 1024
        // writable bytes and `print_prefix` a writable `int`.
        unsafe {
            ffi::av_log_format_line2(
                avcl,
                level,
                fmt,
                vl,
                line.as_mut_ptr(),
                line.len() as c_int,
                &raw mut print_prefix,
            )
        };
        // SAFETY: `av_log_format_line2` NUL-terminates `line`.
        let bytes = unsafe { CStr::from_ptr(line.as_ptr()) }.to_bytes();
        if bytes.windows(TAG.len()).any(|w| w == TAG) {
            let mut captured = CAPTURED.lock().unwrap_or_else(PoisonError::into_inner);
            captured.push((avcl as usize, level, bytes.to_vec()));
        }
    }

    /// [`capture`] installed until drop, which restores the default callback;
    /// it holds the [`LogState`] lock every test of the log callback takes.
    struct Installed(
        #[expect(
            dead_code,
            reason = "held only for its drop, which serialises the process-wide log callback"
        )]
        LogState,
    );

    impl Installed {
        fn new() -> Self {
            let guard = LogState::lock();
            CAPTURED
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clear();
            // SAFETY: `capture` accepts any line from any thread.
            unsafe { ffi::av_log_set_callback(Some(capture)) };
            Self(guard)
        }

        /// The entries captured since the last call.
        #[expect(
            clippy::unused_self,
            reason = "the method keeps the shape of its siblings"
        )]
        fn take(&self) -> Vec<Entry> {
            core::mem::take(&mut *CAPTURED.lock().unwrap_or_else(PoisonError::into_inner))
        }
    }

    impl Drop for Installed {
        fn drop(&mut self) {
            // SAFETY: the default callback is always a valid callback.
            unsafe { ffi::av_log_set_callback(Some(ffi::av_log_default_callback)) };
        }
    }

    /// `av_log(obj, level, "%s", message)`.
    fn c_log(obj: *mut c_void, level: c_int, message: &CStr) {
        // SAFETY: `obj` is NULL or a well-formed context (see `Ctx`); `%s`
        // consumes one live NUL-terminated string.
        unsafe { ffi::av_log(obj, level, c"%s".as_ptr(), message.as_ptr()) }
    }

    const QUIET: c_int = ffi::AV_LOG_QUIET;
    const LEVELS: [c_int; 11] = [
        QUIET,
        ffi::AV_LOG_PANIC as c_int,
        ffi::AV_LOG_FATAL as c_int,
        ffi::AV_LOG_ERROR as c_int,
        ffi::AV_LOG_WARNING as c_int,
        ffi::AV_LOG_INFO as c_int,
        ffi::AV_LOG_VERBOSE as c_int,
        ffi::AV_LOG_DEBUG as c_int,
        ffi::AV_LOG_TRACE as c_int,
        ffi::AV_LOG_INFO as c_int | 134 << 8,
        -100,
    ];

    const MESSAGES: [&CStr; 4] = [
        c"crustify-vlog-equiv plain\n",
        c"crustify-vlog-equiv 100% %s %n %d\n",
        c"crustify-vlog-equiv partial",
        c"crustify-vlog-equiv \x01\x7f\xff end\n",
    ];

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn general_log_reaches_the_callback_as_av_log() {
        let cap = Installed::new();
        for level in LEVELS {
            for message in MESSAGES {
                c_log(core::ptr::null_mut(), level, message);
                let c = cap.take();
                log(Level::from_raw(level), message);
                let rust = cap.take();
                assert_eq!(c.len(), 1);
                assert_eq!(rust, c, "{level} {message:?}");
            }
        }
    }

    /// A context whose class has a log level offset and a parent context.
    #[repr(C)]
    struct Ctx {
        class: *const ffi::AVClass,
        level_offset: c_int,
        parent: *mut *const ffi::AVClass,
    }

    /// The class's name, as `av_default_item_name` would give it.
    unsafe extern "C" fn item_name(ctx: *mut c_void) -> *const c_char {
        // SAFETY: as the comment below states, for this part of the operation.
        let v = unsafe { ctx.cast::<*const ffi::AVClass>().read() };
        // SAFETY: C passes a live context, whose first member is its class.
        unsafe { (*v).class_name }
    }

    /// `bytes` with every occurrence of `from` replaced by `to`.
    fn replace(bytes: &[u8], from: &[u8], to: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i..].starts_with(from) {
                out.extend_from_slice(to);
                i += from.len();
            } else {
                out.push(bytes[i]);
                i += 1;
            }
        }
        out
    }

    fn class(version: c_int, with_offset: bool, with_parent: bool) -> ffi::AVClass {
        ffi::AVClass {
            class_name: c"crustify-vlog-ctx".as_ptr(),
            item_name: Some(item_name),
            version,
            log_level_offset_offset: if with_offset {
                offset_of!(Ctx, level_offset) as c_int
            } else {
                0
            },
            parent_log_context_offset: if with_parent {
                offset_of!(Ctx, parent) as c_int
            } else {
                0
            },
            ..Default::default()
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn context_log_reaches_the_callback_as_av_log() {
        let parent_class = class(0, false, false);
        let mut parent: *const ffi::AVClass = &raw const parent_class;
        let current = (61 << 16) | (6 << 8) | 100;
        let classes = [
            class(current, true, true),
            class(current, false, false),
            class((50 << 16) | (15 << 8) | 1, true, false),
            class((50 << 16) | (15 << 8) | 2, true, false),
            class(current, true, false),
        ];
        let cap = Installed::new();
        for cls in &classes {
            for offset in [0, -8, 16, 1000] {
                let mut mine = Ctx {
                    class: cls,
                    level_offset: offset,
                    parent: &raw mut parent,
                };
                let mut theirs = Ctx {
                    class: cls,
                    level_offset: offset,
                    parent: &raw mut parent,
                };
                for level in LEVELS {
                    for message in MESSAGES {
                        c_log((&raw mut theirs).cast(), level, message);
                        let mut c = cap.take();
                        // SAFETY: `mine` is a live context whose class names
                        // itself, its offset `int` stays far from overflow
                        // and its parent pointer addresses a classed parent.
                        let obj = unsafe { ObjectRef::from_ptr((&raw mut mine).cast()) };
                        log_with_context(obj.unwrap(), Level::from_raw(level), message);
                        let rust = cap.take();
                        assert_eq!(c.len(), 1);
                        // The contexts differ only in address.
                        let ours = &raw mut mine as usize;
                        c[0].0 = ours;
                        let theirs_at = format!("{:p}", &raw const theirs);
                        let mine_at = format!("{:p}", &raw const mine);
                        c[0].2 = replace(&c[0].2, theirs_at.as_bytes(), mine_at.as_bytes());
                        assert_eq!(rust, c, "{level} {offset} {message:?}");
                        let applies = cls.log_level_offset_offset != 0
                            && cls.version >= (50 << 16) | (15 << 8) | 2
                            && level >= ffi::AV_LOG_FATAL as c_int;
                        assert_eq!(rust[0].1, if applies { level + offset } else { level });
                    }
                }
            }
        }
    }
}

/// `default_item_name` against `av_default_item_name` on the same objects.
mod item_name {
    use core::ffi::{CStr, c_void};
    use core::ptr::null;

    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::log::default_item_name;
    use ffmpeg_bindings::libavutil::libavutil::opt::ObjectRef;

    /// A class named `name`, every other member zero.
    fn class(name: Option<&'static CStr>) -> ffi::AVClass {
        ffi::AVClass {
            class_name: name.map_or(null(), CStr::as_ptr),
            ..Default::default()
        }
    }

    /// An `AVClass`-enabled object: its class pointer, then a payload.
    #[repr(C)]
    struct Obj {
        class: *const ffi::AVClass,
        payload: [u64; 3],
    }

    /// Both sides' readings of `obj`'s item name.
    fn both(obj: &Obj) -> (*const core::ffi::c_char, Option<&'static CStr>) {
        let ctx: *const c_void = std::ptr::from_ref::<Obj>(obj).cast();
        // SAFETY: `obj` starts with a non-NULL class pointer to a live class,
        // which `av_default_item_name` only reads.
        let c = unsafe { ffi::av_default_item_name(ctx.cast_mut()) };
        // SAFETY: as above, of a named class with no options, callbacks or
        // offsets; every class here stores a literal, so the name lives for
        // `'static`.
        let r = default_item_name(unsafe { ObjectRef::from_ptr(ctx.cast_mut()) }.unwrap());
        (c, r)
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn names_read_alike_as_the_class_changes() {
        let classes = [
            class(Some(c"first")),
            class(Some(c"")),
            class(Some(c"second-class")),
        ];
        let mut obj = Obj {
            class: &raw const classes[0],
            payload: [1, 2, 3],
        };
        // The same object, switched between classes, compared after each.
        for (i, cls) in classes.iter().chain([&classes[0]]).enumerate() {
            obj.class = cls;
            let (c, r) = both(&obj);
            assert_eq!(r.map_or(null(), CStr::as_ptr), c, "step {i}");
            if let Some(name) = r {
                // SAFETY: a non-NULL `c` is the class's literal name.
                assert_eq!(name, unsafe { CStr::from_ptr(c) }, "step {i}");
            }
        }
        assert_eq!(obj.payload, [1, 2, 3]);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn heap_class_and_name_read_alike() {
        // A class and name that live on the heap, not in statics: the name
        // borrows for the scope that keeps them alive.
        let name = std::ffi::CString::new("heap-class").unwrap();
        let cls = Box::new(ffi::AVClass {
            class_name: name.as_ptr(),
            ..Default::default()
        });
        let obj = Box::new(Obj {
            class: &raw const *cls,
            payload: [0; 3],
        });
        let ctx: *const c_void = (&raw const *obj).cast();
        // SAFETY: `ctx` addresses a live object whose class pointer reaches
        // `cls`, which `av_default_item_name` only reads.
        let c = unsafe { ffi::av_default_item_name(ctx.cast_mut()) };
        // SAFETY: as above, of a named class with no options, callbacks or
        // offsets; `name` outlives every use of the result below.
        let r = default_item_name(unsafe { ObjectRef::from_ptr(ctx.cast_mut()) }.unwrap()).unwrap();
        assert_eq!(r.as_ptr(), c);
        assert_eq!(r, name.as_c_str());
    }
}

/// Serializes the tests here (and in `imgutils`) that touch the process-wide log callback, level
/// or flags; restores the defaults when dropped.
pub(crate) struct LogState(
    #[expect(
        dead_code,
        reason = "held only for its drop, which serialises the process-wide log callback"
    )]
    MutexGuard<'static, ()>,
);

impl LogState {
    pub(crate) fn lock() -> Self {
        static LOCK: Mutex<()> = Mutex::new(());
        Self(LOCK.lock().unwrap_or_else(PoisonError::into_inner))
    }
}

impl Drop for LogState {
    fn drop(&mut self) {
        log::set_default_callback();
        log::set_level(Level::INFO);
        log::set_flags(Flags::empty());
    }
}

/// Buffer sizes each captured record is formatted into.
const SIZES: [usize; 6] = [0, 1, 2, 7, 16, 4096];

/// What a capturing callback saw of one message: its level and context, the
/// `av_log_format_line2` result and buffer at each of [`SIZES`] from the
/// persistent prefix state, the `av_log_format_line` buffer at each size, and
/// the prefix state the message leaves.
#[derive(Debug, PartialEq)]
struct Line {
    level: c_int,
    context: usize,
    measured: Vec<(Option<usize>, Vec<u8>)>,
    written: Vec<Vec<u8>>,
    prefix_after: bool,
}

/// One capturing thread's state: the persistent prefix flag a real logger
/// keeps, and the lines so far.
struct Capture {
    prefix: bool,
    lines: Vec<Line>,
}

thread_local! {
    static CAPTURE: RefCell<Option<Capture>> = const { RefCell::new(None) };
}

/// The current prefix state of this thread's capture, if it captures.
fn capture_prefix() -> Option<bool> {
    CAPTURE.with(|c| c.borrow().as_ref().map(|c| c.prefix))
}

/// Appends `line` to this thread's capture and moves its prefix state.
fn capture_push(line: Line) {
    CAPTURE.with(|c| {
        let mut c = c.borrow_mut();
        let c = c.as_mut().unwrap();
        c.prefix = line.prefix_after;
        c.lines.push(line);
    });
}

/// Captures through the safe [`Record`] methods.
struct RustCapture;

impl Callback for RustCapture {
    fn log(record: Record<'_>) {
        let Some(prefix) = capture_prefix() else {
            return;
        };
        let measured = SIZES
            .iter()
            .map(|&n| {
                let mut buf = vec![0xA5; n];
                let mut p = prefix;
                (record.format_line(&mut buf, &mut p), buf)
            })
            .collect();
        let written = SIZES
            .iter()
            .map(|&n| {
                let mut buf = vec![0xA5; n];
                let mut p = prefix;
                let line = record.write_line(&mut buf, &mut p).to_bytes().to_vec();
                assert_eq!(
                    line,
                    buf.split(|&b| b == 0)
                        .next()
                        .filter(|_| n > 0)
                        .unwrap_or_default()
                );
                buf
            })
            .collect();
        let mut prefix_after = prefix;
        let whole = record.to_line(&mut prefix_after).unwrap();
        let mut p = prefix;
        let mut big = vec![0; 8192];
        record.format_line(&mut big, &mut p);
        assert_eq!(whole.as_bytes(), big.split(|&b| b == 0).next().unwrap());
        assert_eq!(p, prefix_after);
        capture_push(Line {
            level: record.level().raw(),
            context: record.context() as usize,
            measured,
            written,
            prefix_after,
        });
    }
}

/// Captures through the raw C functions, as a C callback would.
///
/// # Safety
///
/// Called by C as a log callback, with arguments valid for the default
/// formatting.
unsafe extern "C" fn c_capture(
    context: *mut c_void,
    level: c_int,
    format: *const c_char,
    args: *mut ffi::__va_list_tag,
) {
    let Some(prefix) = capture_prefix() else {
        return;
    };
    // Each call gets its own `va_copy`, which on x86-64 SysV copies the
    // `__va_list_tag`.
    // SAFETY: `args` is the live `va_list` element C passed this callback.
    let copy = || unsafe { args.read() };
    let ptr_of = |buf: &mut Vec<u8>| {
        if buf.is_empty() {
            ptr::null_mut()
        } else {
            buf.as_mut_ptr().cast::<c_char>()
        }
    };
    let measured = SIZES
        .iter()
        .map(|&n| {
            let mut buf = vec![0xA5; n];
            let mut p = c_int::from(prefix);
            let mut a = copy();
            // SAFETY: the callback's own arguments, a private `va_list` copy,
            // a buffer of `n` bytes (NULL when empty) and a writable `int`.
            let ret = unsafe {
                ffi::av_log_format_line2(
                    context,
                    level,
                    format,
                    &raw mut a,
                    ptr_of(&mut buf),
                    n as c_int,
                    &raw mut p,
                )
            };
            (usize::try_from(ret).ok(), buf)
        })
        .collect();
    let written = SIZES
        .iter()
        .map(|&n| {
            let mut buf = vec![0xA5; n];
            let mut p = c_int::from(prefix);
            let mut a = copy();
            // SAFETY: as above.
            unsafe {
                ffi::av_log_format_line(
                    context,
                    level,
                    format,
                    &raw mut a,
                    ptr_of(&mut buf),
                    n as c_int,
                    &raw mut p,
                );
            };
            buf
        })
        .collect();
    let mut p = c_int::from(prefix);
    let mut a = copy();
    let mut big = vec![0u8; 8192];
    // SAFETY: as above.
    unsafe {
        ffi::av_log_format_line2(
            context,
            level,
            format,
            &raw mut a,
            big.as_mut_ptr().cast(),
            8192,
            &raw mut p,
        )
    };
    capture_push(Line {
        level,
        context: context as usize,
        measured,
        written,
        prefix_after: p != 0,
    });
}

/// Which callback captures.
#[derive(Clone, Copy, Debug)]
enum Via {
    Rust,
    C,
}

/// Runs `f` with the `via` capture installed, and returns what this thread
/// logged. The caller holds the [`LogState`] lock.
fn capture(via: Via, f: impl FnOnce()) -> Vec<Line> {
    CAPTURE.with(|c| {
        *c.borrow_mut() = Some(Capture {
            prefix: true,
            lines: Vec::new(),
        });
    });
    match via {
        Via::Rust => log::set_callback::<RustCapture>(),
        // SAFETY: `c_capture` has the callback signature, lives for the
        // program and only captures on capturing threads.
        Via::C => unsafe { ffi::av_log_set_callback(Some(c_capture)) },
    }
    f();
    log::set_default_callback();
    CAPTURE.with(|c| c.borrow_mut().take().unwrap().lines)
}

/// Messages around every formatting edge: a `%`, empty, partial and whole
/// lines, `\r`, control characters and a line longer than the largest size.
fn messages() -> Vec<std::ffi::CString> {
    let mut m: Vec<std::ffi::CString> = [
        &b"plain\n"[..],
        b"100% %s %d %n\n",
        b"",
        b"partial ",
        b"rest\n",
        b"carriage\r",
        b"ctl \x01\x07\x1b\n",
        b"no newline",
    ]
    .iter()
    .map(|b| std::ffi::CString::new(*b).unwrap())
    .collect();
    m.push(std::ffi::CString::new(vec![b'x'; 5000]).unwrap());
    m.push(c"\n".to_owned());
    m
}

/// Levels: every named one, tinted ones (`AV_LOG_C`), and unnamed values.
const LEVELS: [c_int; 13] = [
    -8,
    0,
    8,
    16,
    24,
    32,
    40,
    48,
    56,
    16 | 134 << 8,
    -9,
    7,
    c_int::MAX,
];

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn log_delivers_what_av_log_with_percent_s_delivers() {
    let _s = LogState::lock();
    for via in [Via::Rust, Via::C] {
        let rust = capture(via, || {
            for (msg, level) in messages().iter().zip(LEVELS.iter().cycle()) {
                log::log(Level::from_raw(*level), msg);
            }
        });
        let c = capture(via, || {
            for (msg, level) in messages().iter().zip(LEVELS.iter().cycle()) {
                // SAFETY: the general log; `"%s"` consumes the one live
                // string passed.
                unsafe { ffi::av_log(ptr::null_mut(), *level, c"%s".as_ptr(), msg.as_ptr()) };
            }
        });
        let rust_fmt = capture(via, || {
            for (msg, level) in messages().iter().zip(LEVELS.iter().cycle()) {
                let text = msg.to_str().unwrap();
                log::log_fmt(Level::from_raw(*level), format_args!("{text}"));
            }
        });
        assert_eq!(rust.len(), messages().len(), "{via:?}");
        assert_eq!(rust, c, "{via:?}");
        assert_eq!(rust_fmt, c, "{via:?}");
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn log_fmt_formats_as_c_printf_and_stops_at_nul() {
    let _s = LogState::lock();
    for via in [Via::Rust, Via::C] {
        let rust = capture(via, || {
            for (i, level) in (0..12).zip(LEVELS.iter().copied().cycle()) {
                let x = f64::from(i) / 7.0;
                log::log_fmt(
                    Level::from_raw(level),
                    format_args!(
                        "{}|{:>5}|{:<4}|{:.2}|{:x}\n",
                        i * -3,
                        "ab",
                        "cd",
                        x,
                        i * 255
                    ),
                );
            }
            log::log_fmt(Level::ERROR, format_args!("before\0after\n"));
            log::log_fmt(Level::ERROR, format_args!("{}", "\0"));
        });
        let c = capture(via, || {
            for (i, level) in (0..12).zip(LEVELS.iter().copied().cycle()) {
                // SAFETY: the conversions match the arguments: an `int`, two
                // live strings, a `double` and an `unsigned`.
                unsafe {
                    ffi::av_log(
                        ptr::null_mut(),
                        level,
                        c"%d|%5s|%-4s|%.2f|%x\n".as_ptr(),
                        i * -3,
                        c"ab".as_ptr(),
                        c"cd".as_ptr(),
                        f64::from(i) / 7.0,
                        (i * 255) as c_uint,
                    );
                };
            }
            // For the unsafe operations below: the general log; `"%s"` consumes the one live
            // string.
            {
                // SAFETY: as the comment above this block states.
                unsafe {
                    ffi::av_log(
                        ptr::null_mut(),
                        Level::ERROR.raw(),
                        c"%s".as_ptr(),
                        c"before".as_ptr(),
                    );
                };
                // SAFETY: as the comment above this block states.
                unsafe {
                    ffi::av_log(
                        ptr::null_mut(),
                        Level::ERROR.raw(),
                        c"%s".as_ptr(),
                        c"".as_ptr(),
                    );
                };
            }
        });
        assert_eq!(rust.len(), 14, "{via:?}");
        assert_eq!(rust, c, "{via:?}");
    }
}

/// Logs C formats with real arguments, which the records format.
fn log_formats() {
    let levels = LEVELS.iter().copied().cycle();
    for (i, level) in (0..20).zip(levels) {
        // For the unsafe operations below: each format's conversions match the arguments passed: an
        // `int`, a live string and a `double`, or a string alone.
        if i % 3 == 2 {
            // SAFETY: as the comment above this statement states.
            unsafe {
                ffi::av_log(
                    ptr::null_mut(),
                    level,
                    c"only %s".as_ptr(),
                    c"text".as_ptr(),
                );
            };
        } else {
            // SAFETY: as the comment at the head of this block states.
            unsafe {
                ffi::av_log(
                    ptr::null_mut(),
                    level,
                    c"%d|%-6s|%8.3f%s".as_ptr(),
                    i as c_int * -7,
                    c"ab".as_ptr(),
                    f64::from(i) / 3.0,
                    if i % 2 == 0 {
                        c"\n".as_ptr()
                    } else {
                        c"".as_ptr()
                    },
                );
            };
        }
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn records_format_c_arguments_as_the_c_functions() {
    let _s = LogState::lock();
    let rust = capture(Via::Rust, log_formats);
    let c = capture(Via::C, log_formats);
    assert_eq!(rust.len(), 20);
    assert_eq!(rust, c);
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn records_report_print_level_and_ignore_print_time_like_c() {
    let _s = LogState::lock();
    for flags in [
        Flags::PRINT_LEVEL,
        Flags::PRINT_TIME | Flags::PRINT_LEVEL,
        Flags::SKIP_REPEATED,
    ] {
        log::set_flags(flags);
        let rust = capture(Via::Rust, log_formats);
        let c = capture(Via::C, log_formats);
        assert_eq!(rust, c, "{flags:?}");
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn log_once_moves_state_and_level_like_av_log_once() {
    let _s = LogState::lock();
    let script = [
        (Level::ERROR, Level::DEBUG, false),
        (Level::ERROR, Level::DEBUG, false),
        (Level::INFO, Level::TRACE, true),
        (Level::QUIET, Level::PANIC, false),
        (Level::from_raw(-100), Level::from_raw(100), true),
    ];
    for via in [Via::Rust, Via::C] {
        let mut rust_states = Vec::new();
        let rust = capture(via, || {
            for (initial, subsequent, reset) in script {
                let mut state = false;
                for msg in [c"first\n", c"again\n", c"third\n"] {
                    if reset {
                        state = false;
                    }
                    log::log_once(initial, subsequent, &mut state, msg);
                    rust_states.push(state);
                }
            }
        });
        let mut c_states = Vec::new();
        let c = capture(via, || {
            for (initial, subsequent, reset) in script {
                let mut state: c_int = 0;
                for msg in [c"first\n", c"again\n", c"third\n"] {
                    if reset {
                        state = 0;
                    }
                    // SAFETY: the general log; `state` is a writable `int`;
                    // `"%s"` consumes the one live string passed.
                    unsafe {
                        ffi::av_log_once(
                            ptr::null_mut(),
                            initial.raw(),
                            subsequent.raw(),
                            &raw mut state,
                            c"%s".as_ptr(),
                            msg.as_ptr(),
                        );
                    };
                    c_states.push(state != 0);
                }
            }
        });
        assert_eq!(rust, c, "{via:?}");
        assert_eq!(rust_states, c_states, "{via:?}");
        let levels: Vec<c_int> = rust.iter().map(|l| l.level).collect();
        assert_eq!(&levels[..3], [16, 48, 48]);
    }
}

/// A logging context: its class, the level offset its class locates, and a
/// parent context.
#[repr(C)]
struct LogCtx {
    class: *const ffi::AVClass,
    level_offset: c_int,
    parent: *mut c_void,
}

/// The current `LIBAVUTIL_VERSION_INT`, which enables every class member.
const VERSION: c_int = (ffi::LIBAVUTIL_VERSION_MAJOR << 16
    | ffi::LIBAVUTIL_VERSION_MINOR << 8
    | ffi::LIBAVUTIL_VERSION_MICRO) as c_int;

/// A class named `name` that locates `LogCtx`'s level offset and parent.
fn log_class(name: &'static CStr) -> ffi::AVClass {
    ffi::AVClass {
        class_name: name.as_ptr(),
        version: VERSION,
        log_level_offset_offset: offset_of!(LogCtx, level_offset) as c_int,
        parent_log_context_offset: offset_of!(LogCtx, parent) as c_int,
        category: ffi::AVClassCategory::AV_CLASS_CATEGORY_ENCODER,
        ..Default::default()
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn contexts_shift_levels_and_prefix_lines_like_c() {
    let _s = LogState::lock();
    let child_class = log_class(c"child");
    let parent_class = log_class(c"parent");
    let mut parent = LogCtx {
        class: &raw const parent_class,
        level_offset: 0,
        parent: ptr::null_mut(),
    };
    let mut ctx = LogCtx {
        class: &raw const child_class,
        level_offset: 8,
        parent: (&raw mut parent).cast(),
    };
    let mut orphan = LogCtx {
        class: &raw const child_class,
        level_offset: -16,
        parent: ptr::null_mut(),
    };
    // A context whose class is NULL is no `ObjectRef`, so no Rust call
    // takes one; NULL is the general log.
    let contexts: [*mut c_void; 3] = [
        (&raw mut ctx).cast(),
        (&raw mut orphan).cast(),
        ptr::null_mut(),
    ];
    let msgs = [c"one\n", c"part ", c"two\n", c"%p\n"];
    for via in [Via::Rust, Via::C] {
        let mut states = (Vec::new(), Vec::new());
        let rust = capture(via, || {
            for &cx in &contexts {
                for level in [Level::QUIET, Level::PANIC, Level::FATAL, Level::INFO] {
                    let mut once = false;
                    for msg in msgs {
                        let text = msg.to_str().unwrap();
                        // SAFETY: `cx` is NULL, the general log, or a live
                        // `LogCtx` of a named class with no callbacks or
                        // options, an `int` at its level offset far from
                        // overflow and a NULL or valid parent context;
                        // nothing writes them.
                        if let Some(o) = unsafe {
                            ffmpeg_bindings::libavutil::libavutil::opt::ObjectRef::from_ptr(cx)
                        } {
                            log::log_with_context(o, level, msg);
                            log::log_fmt_with_context(o, level, format_args!("{text}"));
                            log::log_once_with_context(o, level, Level::TRACE, &mut once, msg);
                        } else {
                            log::log(level, msg);
                            log::log_fmt(level, format_args!("{text}"));
                            log::log_once(level, Level::TRACE, &mut once, msg);
                        }
                        states.0.push(once);
                    }
                }
            }
        });
        let c = capture(via, || {
            for &cx in &contexts {
                for level in [Level::QUIET, Level::PANIC, Level::FATAL, Level::INFO] {
                    let mut once: c_int = 0;
                    for msg in msgs {
                        // For the unsafe operations below: as above; `"%s"` consumes the one live
                        // string passed, and `once` is a writable `int`.
                        {
                            // SAFETY: as the comment above this block states.
                            unsafe { ffi::av_log(cx, level.raw(), c"%s".as_ptr(), msg.as_ptr()) };
                            // SAFETY: as the comment above this block states.
                            unsafe { ffi::av_log(cx, level.raw(), c"%s".as_ptr(), msg.as_ptr()) };
                            // SAFETY: as the comment above this block states.
                            unsafe {
                                ffi::av_log_once(
                                    cx,
                                    level.raw(),
                                    Level::TRACE.raw(),
                                    &raw mut once,
                                    c"%s".as_ptr(),
                                    msg.as_ptr(),
                                );
                            };
                        }
                        states.1.push(once != 0);
                    }
                }
            }
        });
        assert_eq!(rust, c, "{via:?}");
        assert_eq!(states.0, states.1, "{via:?}");
        // The offsets moved the levels from FATAL up only.
        let ctx_levels: Vec<c_int> = rust.iter().take(48).step_by(3).map(|l| l.level).collect();
        assert_eq!(&ctx_levels[..4], [-8, -8, -8, -8]);
        assert_eq!(&ctx_levels[8..12], [16, 16, 16, 16]);
        let prefixed = String::from_utf8_lossy(&rust[0].measured[5].1);
        assert!(prefixed.starts_with("[parent @ 0x"), "{prefixed}");
        assert!(prefixed.contains("] [child @ 0x"), "{prefixed}");
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn level_and_flags_round_trip_through_c() {
    let _s = LogState::lock();
    for raw in [-8, 0, 16, 32, 56, 99, -1000, c_int::MIN, c_int::MAX] {
        log::set_level(Level::from_raw(raw));
        // SAFETY: an atomic load.
        assert_eq!(unsafe { ffi::av_log_get_level() }, raw);
        // SAFETY: an atomic store.
        unsafe { ffi::av_log_set_level(raw ^ 1) };
        assert_eq!(log::level(), Level::from_raw(raw ^ 1));
    }
    for bits in [0, 1, 2, 4, 8, 15, 0x100, -1, c_int::MIN] {
        log::set_flags(Flags::from_bits_retain(bits));
        // SAFETY: an atomic load.
        assert_eq!(unsafe { ffi::av_log_get_flags() }, bits);
        // SAFETY: an atomic store.
        unsafe { ffi::av_log_set_flags(bits ^ 2) };
        assert_eq!(log::flags().bits(), bits ^ 2);
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn cleared_callback_drops_messages_like_a_null_c_callback() {
    let _s = LogState::lock();
    let rust = capture(Via::Rust, || {
        log::clear_callback();
        log::log(Level::PANIC, c"dropped\n");
        log::set_callback::<RustCapture>();
        log::log(Level::PANIC, c"kept\n");
    });
    let c = capture(Via::Rust, || {
        // SAFETY: C checks for a NULL callback before calling it.
        unsafe { ffi::av_log_set_callback(None) };
        // SAFETY: the general log with a literal format.
        unsafe { ffi::av_log(ptr::null_mut(), Level::PANIC.raw(), c"dropped\n".as_ptr()) };
        log::set_callback::<RustCapture>();
        // SAFETY: as above.
        unsafe { ffi::av_log(ptr::null_mut(), Level::PANIC.raw(), c"kept\n".as_ptr()) };
    });
    assert_eq!(rust.len(), 1);
    assert_eq!(rust, c);
}

/// Forwards every record to the default callback.
struct Forward;

impl Callback for Forward {
    fn log(record: Record<'_>) {
        record.log_default();
    }
}

/// Selects the child's callback for [`default_callback_output_is_identical`].
const LOG_VIA: &str = "CRUSTIFY_EQUIV_LOG_VIA";

/// The script both children run: settings and messages that reach every
/// branch of the default callback a context-free message can: level
/// filtering, colourless output, the level prefix, repeat collapsing (the
/// `\n` report, as stderr is no terminal), empty lines, `\r` lines and
/// control-character sanitizing.
fn default_callback_script() {
    log::set_level(Level::VERBOSE);
    log::set_flags(Flags::SKIP_REPEATED | Flags::PRINT_LEVEL);
    for _ in 0..3 {
        log::log(Level::WARNING, c"same\n");
    }
    log::log(Level::DEBUG, c"dropped by level\n");
    log::log_fmt(Level::ERROR, format_args!("{} {:>4}\n", "formatted", 7));
    log::log(Level::ERROR, c"partial ");
    log::log(Level::ERROR, c"line\n");
    log::log(Level::INFO, c"ctl \x01\x7f\x1b\n");
    log::log(Level::from_raw(Level::ERROR.raw() | 200 << 8), c"tinted\n");
    log::log(Level::VERBOSE, c"cr\r");
    log::log(Level::VERBOSE, c"cr\r");
    log::set_flags(Flags::empty());
    log::log(Level::FATAL, c"plain\n");
    log::log(Level::FATAL, c"plain\n");
    // Repeated empty lines are not collapsed.
    log::set_flags(Flags::SKIP_REPEATED);
    log::log(Level::INFO, c"");
    log::log(Level::INFO, c"");
    log::log(Level::INFO, c"after empty\n");
    log::set_flags(Flags::empty());
    log::log_once(Level::INFO, Level::DEBUG, &mut false, c"once\n");
    log::set_level(Level::QUIET);
    log::log(Level::PANIC, c"quiet\n");
    log::set_level(Level::TRACE);
    log::log(Level::QUIET, c"");
    log::clear_callback();
    log::log(Level::PANIC, c"no callback\n");
}

/// `Record::log_default` prints byte for byte what `av_log_default_callback`
/// prints for the same messages. Its output and repeat state are
/// process-wide, so each callback runs in a child process running nothing
/// else, and the parent compares their stderr.
#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn default_callback_output_is_identical() {
    const NAME: &str = "log::default_callback_output_is_identical";
    match std::env::var(LOG_VIA).as_deref() {
        Err(_) => {
            let run = |via: &str| {
                let out = std::process::Command::new(std::env::current_exe().unwrap())
                    .args(["--exact", NAME, "--test-threads=1", "--nocapture"])
                    .env(LOG_VIA, via)
                    .env("AV_LOG_FORCE_NOCOLOR", "1")
                    .output()
                    .unwrap();
                assert!(out.status.success(), "{via} child failed: {out:?}");
                out.stderr
            };
            let (c, rust) = (run("c"), run("rust"));
            let text = String::from_utf8_lossy(&c);
            assert!(text.contains("[warning] same\n"), "{text}");
            assert!(text.contains("Last message repeated 2 times\n"), "{text}");
            assert!(
                !text.contains("dropped") && !text.contains("quiet"),
                "{text}"
            );
            assert!(text.contains("ctl ?\x7f?\n"), "{text}");
            assert_eq!(String::from_utf8_lossy(&rust), text);
        }
        Ok("c") => {
            // SAFETY: C's own default callback.
            unsafe { ffi::av_log_set_callback(Some(ffi::av_log_default_callback)) };
            default_callback_script();
        }
        Ok(_) => {
            log::set_callback::<Forward>();
            default_callback_script();
        }
    }
}

/// The `log_with_context` route to a raw class's `ClassRef`.
#[path = "../common/class.rs"]
pub(crate) mod class_fixture;

/// Equivalence of `AVClass`'s getters and callable handles with C's readers
/// of the same classes: `av_default_item_name`, `av_default_get_category`,
/// `av_opt_next`, `av_opt_child_class_iterate`, `av_opt_child_next` and
/// `av_opt_query_ranges`, on Rust-built classes, libavutil's public IAMF
/// classes and their children, and test classes with callbacks.
mod class {
    use core::ffi::{CStr, c_char, c_int, c_void};
    use core::ptr::{self, NonNull};
    use std::ffi::CString;

    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::log::{Class, ClassCategory, ClassRef};
    use ffmpeg_bindings::libavutil::libavutil::opt::{ObjectRef, RangeFlags};

    use super::LogState;
    use super::class_fixture::with_class;
    use crate::opt::fixture::{CB_FAKE, CB_KEYS, RANGED, UNKNOWN, UNRANGED};

    /// A `const AVClass *` slot: the fake object C's option readers take,
    /// and a context `log_with_context` accepts.
    struct Fake(*const ffi::AVClass);

    impl Fake {
        fn obj(&mut self) -> *mut c_void {
            (&raw mut self.0).cast()
        }
    }

    /// The names of the options `av_opt_next` walks for `obj`.
    fn c_options(obj: *mut c_void) -> Vec<*const c_char> {
        let mut names = Vec::new();
        let mut prev = ptr::null();
        loop {
            // SAFETY: `obj` is a `const AVClass *` slot to a well-formed
            // class; `prev` is NULL or an entry of its table.
            prev = unsafe { ffi::av_opt_next(obj, prev) };
            if prev.is_null() {
                return names;
            }
            // SAFETY: a live table entry.
            names.push(unsafe { (*prev).name });
        }
    }

    /// The classes `av_opt_child_class_iterate` yields for `class`.
    fn c_children(class: *const ffi::AVClass) -> Vec<*const ffi::AVClass> {
        let mut iter = ptr::null_mut();
        let mut out = Vec::new();
        loop {
            // SAFETY: a well-formed class and its own iteration state.
            let child = unsafe { ffi::av_opt_child_class_iterate(class, &raw mut iter) };
            if child.is_null() {
                return out;
            }
            out.push(child);
        }
    }

    fn rust_options(c: ClassRef<'_>) -> Vec<*const c_char> {
        c.options()
            .iter()
            .map(|o| o.name().map_or(ptr::null(), CStr::as_ptr))
            .collect()
    }

    /// Every getter of `c` against the raw class `k` it was reached for, and
    /// its options and children against C's walks, recursively.
    fn compare(c: ClassRef<'_>, k: *const ffi::AVClass) {
        // SAFETY: `k` is a live, well-formed class; fields are read by value.
        let raw = unsafe { k.read() };
        assert_eq!(c.name().map_or(ptr::null(), CStr::as_ptr), raw.class_name);
        assert_eq!(c.version(), raw.version);
        assert_eq!(c.category().value(), raw.category.0);
        let offset = |o: Option<core::num::NonZeroI32>| o.map_or(0, |o| o.get());
        assert_eq!(
            offset(c.log_level_offset_offset()),
            raw.log_level_offset_offset
        );
        assert_eq!(
            offset(c.parent_log_context_offset()),
            raw.parent_log_context_offset
        );
        assert_eq!(offset(c.state_flags_offset()), raw.state_flags_offset);
        assert_eq!(c.item_name().is_some(), raw.item_name.is_some());
        assert_eq!(c.get_category().is_some(), raw.get_category.is_some());
        assert_eq!(c.query_ranges().is_some(), raw.query_ranges.is_some());
        assert_eq!(c.child_next().is_some(), raw.child_next.is_some());

        let mut fake = Fake(k);
        assert_eq!(rust_options(c), c_options(fake.obj()));
        // The class names a fake object of it as `av_default_item_name`
        // does: by the class name.
        // SAFETY: a fake object of `k`, whose `item_name` reads only the
        // class pointer (`av_default_item_name`, as every class here).
        let c_name = unsafe { ffi::av_default_item_name(fake.obj()) };
        assert_eq!(c.name().map_or(ptr::null(), CStr::as_ptr), c_name);
        // SAFETY: as above; `av_default_get_category` reads the class's
        // `category`.
        let c_category = unsafe { ffi::av_default_get_category(fake.obj()) };
        assert_eq!(c.category().value(), c_category.0);

        let children = c_children(k);
        let rust: Vec<_> = c.child_classes().collect();
        assert_eq!(rust.len(), children.len());
        for (child, k) in rust.into_iter().zip(children) {
            compare(child, k);
        }
    }

    static OWN: Class = Class::new(c"equiv-class", ClassCategory::BITSTREAM_FILTER);
    static NAMELESS: Class = Class::new(c"", ClassCategory::from_value(77));

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn rust_classes_read_as_c_reads_them() {
        let _s = LogState::lock();
        for class in [&OWN, &NAMELESS] {
            compare(class.as_ref(), class.as_c_ptr());
            let mut fake = Fake(class.as_c_ptr());
            // SAFETY: a context whose class is a live Rust-built class.
            let same = unsafe { with_class(fake.obj(), |c| c.unwrap().ptr_eq(class.as_ref())) };
            assert!(same);
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn iamf_classes_read_as_c_reads_them() {
        let _s = LogState::lock();
        let getters: [unsafe extern "C" fn() -> *const ffi::AVClass; 3] = [
            ffi::av_iamf_audio_element_get_class,
            ffi::av_iamf_mix_presentation_get_class,
            ffi::av_iamf_param_definition_get_class,
        ];
        for get in getters {
            // SAFETY: each returns a pointer to a static class.
            let k = unsafe { get() };
            let mut fake = Fake(k);
            // SAFETY: `fake` is a context whose class is a static FFmpeg
            // class with no offsets.
            unsafe { with_class(fake.obj(), |c| compare(c.unwrap(), k)) };
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn a_classless_context_has_no_class_as_c_reads_it() {
        let _s = LogState::lock();
        let mut fake = Fake(ptr::null());
        // SAFETY: a context whose class pointer is NULL.
        assert!(unsafe { with_class(fake.obj(), |c| c.is_none()) });
        // C's option walk finds no class, hence no options, either.
        assert!(c_options(fake.obj()).is_empty());
    }

    /// An object: its class pointer, then two children.
    #[repr(C)]
    struct Parent {
        class: *const ffi::AVClass,
        children: [*mut c_void; 2],
    }

    unsafe extern "C" fn next_child(obj: *mut c_void, prev: *mut c_void) -> *mut c_void {
        // SAFETY: `obj` is a `Parent`.
        let children = unsafe { (*obj.cast::<Parent>()).children };
        match children.iter().position(|&c| c == prev) {
            _ if prev.is_null() => children[0],
            Some(i) if i + 1 < children.len() => children[i + 1],
            _ => ptr::null_mut(),
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn child_next_walks_as_av_opt_child_next() {
        let _s = LogState::lock();
        let child_class = ffi::AVClass {
            class_name: c"child".as_ptr(),
            ..Default::default()
        };
        let parent_class = ffi::AVClass {
            class_name: c"parent".as_ptr(),
            child_next: Some(next_child),
            ..Default::default()
        };
        let mut kids = [Fake(&raw const child_class), Fake(&raw const child_class)];
        let mut parent = Parent {
            class: &raw const parent_class,
            children: [kids[0].obj(), kids[1].obj()],
        };
        let p = (&raw mut parent).cast::<c_void>();
        let mut c_children = Vec::new();
        let mut prev = ptr::null_mut();
        // SAFETY: `p` starts with a pointer to `parent_class`, whose
        // `child_next` accepts it and NULL or a child it returned.
        while let Some(next) = NonNull::new(unsafe { ffi::av_opt_child_next(p, prev) }) {
            // SAFETY: a child, led by a pointer to `child_class`.
            let class = unsafe { *next.as_ptr().cast::<*const ffi::AVClass>() };
            // SAFETY: as above; the class's name is a literal.
            c_children.push(unsafe { (*class).class_name });
            prev = next.as_ptr();
        }
        // SAFETY: as above; the parent and its children are named classes
        // with no options or offsets, which nothing writes during the borrow.
        let obj = unsafe { ObjectRef::from_ptr(p) }.unwrap();
        let rust: Vec<*const c_char> = obj
            .children()
            .map(|c| c.class().name().unwrap().as_ptr())
            .collect();
        assert_eq!(rust, c_children);
        assert_eq!(rust.len(), kids.len());
    }

    /// A range list's contents: counts, then each range's fields.
    type List = (
        c_int,
        c_int,
        Vec<Option<(Option<CString>, [f64; 4], c_int)>>,
    );

    fn list(p: *const ffi::AVOptionRanges) -> List {
        // For the unsafe operations below: `p` is a live list whose `range` array holds
        // `nb_ranges * nb_components` pointers, each NULL or a live range.
        {
            // SAFETY: as the comment above this block states.
            let n = (unsafe { (*p).nb_ranges } * unsafe { (*p).nb_components }).max(0) as usize;
            // SAFETY: as the comment above this block states: a read of the array.
            let array = unsafe { (*p).range };
            let ranges = (0..n)
                .map(|i| {
                    // SAFETY: as the comment above this block states, entry `i < n`.
                    let at = unsafe { array.add(i) };
                    // SAFETY: as above, initialised.
                    let r = unsafe { *at };
                    (!r.is_null()).then(|| {
                        // SAFETY: as above, NULL or a live range: a copy of its fields.
                        let r = unsafe { r.read() };
                        (
                            // SAFETY: as above: NULL or a NUL-terminated string.
                            (!r.str_.is_null())
                                // SAFETY: as the comment above this statement states.
                                .then(|| unsafe { CStr::from_ptr(r.str_) }.to_owned()),
                            [r.value_min, r.value_max, r.component_min, r.component_max],
                            r.is_range,
                        )
                    })
                })
                .collect();
            // SAFETY: as the comment above this block states.
            (
                unsafe { (*p).nb_ranges },
                unsafe { (*p).nb_components },
                ranges,
            )
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn query_ranges_matches_av_opt_query_ranges() {
        let _s = LogState::lock();
        let fake = ffi::AV_OPT_SEARCH_FAKE_OBJ as c_int;
        let multi = ffi::AV_OPT_MULTI_COMPONENT_RANGE as c_int;
        let obj = CB_FAKE.as_obj();
        // `obj` is a `const AVClass *` to the fixture's class, whose callback honours
        // `av_opt_query_ranges`' contract and accepts the fake object with
        // `AV_OPT_SEARCH_FAKE_OBJ`.
        let check = |c: Option<ClassRef<'_>>| {
            let class = c.unwrap();
            assert!(class.query_ranges().is_some());
            let keys = CB_KEYS
                .iter()
                .chain(&RANGED)
                .chain(&UNRANGED)
                .chain(&UNKNOWN);
            for (flags, rflags) in [
                (fake, RangeFlags::NONE),
                (fake | multi, RangeFlags::MULTI_COMPONENT),
            ] {
                for key in keys.clone() {
                    let rust = class.option_ranges(key, rflags);
                    let mut out = ptr::null_mut();
                    // SAFETY: as stated before `check`: the fixture's class accepts the fake
                    // object; `out` is a writable slot.
                    let ret =
                        unsafe { ffi::av_opt_query_ranges(&raw mut out, obj, key.as_ptr(), flags) };
                    match rust {
                        Ok(r) => {
                            assert!(ret >= 0, "{key:?}");
                            assert_eq!(list(r.as_c_ptr()), list(out), "{key:?} {flags}");
                            // SAFETY: the list C just returned, freed once.
                            unsafe { ffi::av_opt_freep_ranges(&raw mut out) };
                        }
                        Err(e) => assert_eq!(e, ret, "{key:?} {flags}"),
                    }
                }
            }
        };
        // SAFETY: as above, `obj` leads with a pointer to a live class.
        unsafe { with_class(obj, check) };
    }
}

/// The safe [`default_category`](log::default_category) against raw
/// `av_default_get_category` (core wave 1, batch 2), over independently built
/// contexts of every category and of unnamed values.
mod default_category {
    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::log::{ClassCategory, default_category};

    use super::{CATEGORIES, Ctx, c_category, class, options};

    /// The safe wrapper over a context of `class`.
    fn mine(class: &ffi::AVClass) -> ClassCategory {
        let ctx = Ctx {
            class,
            state_flags: 0,
            value: 0,
        };
        // SAFETY: `ctx` starts with an aligned pointer to the live `class`,
        // whose options address `ctx`'s members; nothing writes either during
        // the call.
        let obj = unsafe {
            ffmpeg_bindings::libavutil::libavutil::opt::ObjectRef::from_ptr(
                (&raw const ctx).cast_mut().cast(),
            )
        };
        default_category(obj.unwrap())
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn default_category_is_av_default_get_category() {
        let opts = options();
        let unnamed = [17, 40, 46, 1000, u32::MAX].map(ffi::AVClassCategory);
        for raw in CATEGORIES.iter().map(|&(_, raw)| raw).chain(unnamed) {
            let (theirs, ours) = (class(raw, &opts), class(raw, &opts));
            assert_eq!(
                mine(&ours),
                ClassCategory::from(c_category(&theirs)),
                "{raw:?}"
            );
            assert_eq!(mine(&ours).value(), raw.0);
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn a_class_rewritten_between_calls_reads_alike() {
        let opts = options();
        let (mut ours, mut theirs) = (class(CATEGORIES[0].1, &opts), class(CATEGORIES[0].1, &opts));
        for &(_, raw) in CATEGORIES.iter().rev() {
            ours.category = raw;
            theirs.category = raw;
            assert_eq!(mine(&ours), ClassCategory::from(c_category(&theirs)));
        }
    }
}
