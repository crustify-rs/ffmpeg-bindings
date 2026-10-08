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

/// `av_vlog` through [`log`] and [`log_with_context`], against `av_log`
/// with the format `%s`, as a capturing log callback observes both.
mod vlog {
    use core::ffi::{CStr, c_char, c_int, c_void};
    use core::mem::offset_of;
    use std::sync::{Mutex, MutexGuard, PoisonError};

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::log::{log, log_with_context};

    /// Tags the lines these tests log, so the capture ignores any other test's.
    const TAG: &[u8] = b"crustify-vlog-equiv";

    /// What the callback saw: the context, the level and the formatted line.
    type Entry = (usize, c_int, Vec<u8>);

    static CAPTURED: Mutex<Vec<Entry>> = Mutex::new(Vec::new());

    /// Serializes the tests that install [`capture`], a process-wide setting.
    static INSTALLED: Mutex<()> = Mutex::new(());

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
                &mut print_prefix,
            )
        };
        // SAFETY: `av_log_format_line2` NUL-terminates `line`.
        let bytes = unsafe { CStr::from_ptr(line.as_ptr()) }.to_bytes();
        if bytes.windows(TAG.len()).any(|w| w == TAG) {
            let mut captured = CAPTURED.lock().unwrap_or_else(PoisonError::into_inner);
            captured.push((avcl as usize, level, bytes.to_vec()));
        }
    }

    /// [`capture`] installed until drop, which restores the default callback.
    struct Installed(#[allow(dead_code)] MutexGuard<'static, ()>);

    impl Installed {
        fn new() -> Self {
            let guard = INSTALLED.lock().unwrap_or_else(PoisonError::into_inner);
            CAPTURED
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clear();
            // SAFETY: `capture` accepts any line from any thread.
            unsafe { ffi::av_log_set_callback(Some(capture)) };
            Self(guard)
        }

        /// The entries captured since the last call.
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
    fn general_log_reaches_the_callback_as_av_log() {
        let cap = Installed::new();
        for level in LEVELS {
            for message in MESSAGES {
                c_log(core::ptr::null_mut(), level, message);
                let c = cap.take();
                log(level, message);
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
        // SAFETY: C passes a live context, whose first member is its class.
        unsafe { (*ctx.cast::<*const ffi::AVClass>().read()).class_name }
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
    fn context_log_reaches_the_callback_as_av_log() {
        let parent_class = class(0, false, false);
        let mut parent: *const ffi::AVClass = &parent_class;
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
                    parent: &mut parent,
                };
                let mut theirs = Ctx {
                    class: cls,
                    level_offset: offset,
                    parent: &mut parent,
                };
                for level in LEVELS {
                    for message in MESSAGES {
                        c_log((&raw mut theirs).cast(), level, message);
                        let mut c = cap.take();
                        // SAFETY: `mine` is a live context whose class names
                        // itself, its offset `int` stays far from overflow
                        // and its parent pointer addresses a classed parent.
                        unsafe { log_with_context((&raw mut mine).cast(), level, message) };
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

    #[test]
    fn null_context_is_the_general_log() {
        let cap = Installed::new();
        for level in LEVELS {
            // SAFETY: a NULL context is the general log.
            unsafe { log_with_context(core::ptr::null_mut(), level, MESSAGES[0]) };
            let ctx = cap.take();
            log(level, MESSAGES[0]);
            assert_eq!(cap.take(), ctx);
            assert_eq!(ctx.len(), 1);
            assert_eq!(ctx[0].0, 0);
        }
    }
}

/// `default_item_name` against `av_default_item_name` on the same objects.
mod item_name {
    use core::ffi::{CStr, c_void};
    use core::ptr::null;

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::log::default_item_name;

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
        let ctx: *const c_void = (obj as *const Obj).cast();
        // SAFETY: `obj` starts with a non-NULL class pointer to a live class,
        // which `av_default_item_name` only reads.
        let c = unsafe { ffi::av_default_item_name(ctx.cast_mut()) };
        // SAFETY: as above; every class here stores a literal or NULL, so the
        // name lives for `'static`.
        let r = unsafe { default_item_name(ctx) };
        (c, r)
    }

    #[test]
    fn names_read_alike_as_the_class_changes() {
        let classes = [
            class(Some(c"first")),
            class(None),
            class(Some(c"")),
            class(Some(c"second-class")),
        ];
        let mut obj = Obj {
            class: &classes[0],
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
    fn heap_class_and_name_read_alike() {
        // A class and name that live on the heap, not in statics: the name
        // borrows for the scope that keeps them alive.
        let name = std::ffi::CString::new("heap-class").unwrap();
        let cls = Box::new(ffi::AVClass {
            class_name: name.as_ptr(),
            ..Default::default()
        });
        let obj = Box::new(Obj {
            class: &*cls,
            payload: [0; 3],
        });
        let ctx: *const c_void = (&*obj as *const Obj).cast();
        // SAFETY: `ctx` addresses a live object whose class pointer reaches
        // `cls`, which `av_default_item_name` only reads.
        let c = unsafe { ffi::av_default_item_name(ctx.cast_mut()) };
        // SAFETY: as above; `name` outlives every use of the result below.
        let r = unsafe { default_item_name(ctx) }.unwrap();
        assert_eq!(r.as_ptr(), c);
        assert_eq!(r, name.as_c_str());
    }
}
