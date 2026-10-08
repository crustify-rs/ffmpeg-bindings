//! Equivalence of `libavutil/log.h`'s category and state-flag values with the
//! raw C enums, as C reads them back from independently owned `AVClass`
//! contexts, and of the logging functions and callback records with the C
//! functions they wrap.

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::mem::offset_of;
use core::ptr::{self, addr_of_mut};
use std::cell::RefCell;
use std::sync::{Mutex, MutexGuard, PoisonError};

use ffmpeg::libavutil::ffi;
use ffmpeg::libavutil::log::{
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
    use std::sync::{Mutex, PoisonError};

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::log::{Level, log, log_with_context};

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

    /// [`capture`] installed until drop, which restores the default callback;
    /// it holds the [`LogState`] lock every test of the log callback takes.
    struct Installed(#[allow(dead_code)] LogState);

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
                        unsafe {
                            log_with_context(
                                (&raw mut mine).cast(),
                                Level::from_raw(level),
                                message,
                            )
                        };
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
            unsafe { log_with_context(core::ptr::null_mut(), Level::from_raw(level), MESSAGES[0]) };
            let ctx = cap.take();
            log(Level::from_raw(level), MESSAGES[0]);
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

/// Serializes the tests here that touch the process-wide log callback, level
/// or flags; restores the defaults when dropped.
struct LogState(#[allow(dead_code)] MutexGuard<'static, ()>);

impl LogState {
    fn lock() -> Self {
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
                let mut buf = vec![0xa5; n];
                let mut p = prefix;
                (record.format_line(&mut buf, &mut p), buf)
            })
            .collect();
        let written = SIZES
            .iter()
            .map(|&n| {
                let mut buf = vec![0xa5; n];
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
            let mut buf = vec![0xa5; n];
            let mut p = c_int::from(prefix);
            let mut a = copy();
            // SAFETY: the callback's own arguments, a private `va_list` copy,
            // a buffer of `n` bytes (NULL when empty) and a writable `int`.
            let ret = unsafe {
                ffi::av_log_format_line2(
                    context,
                    level,
                    format,
                    &mut a,
                    ptr_of(&mut buf),
                    n as c_int,
                    &mut p,
                )
            };
            (usize::try_from(ret).ok(), buf)
        })
        .collect();
    let written = SIZES
        .iter()
        .map(|&n| {
            let mut buf = vec![0xa5; n];
            let mut p = c_int::from(prefix);
            let mut a = copy();
            // SAFETY: as above.
            unsafe {
                ffi::av_log_format_line(
                    context,
                    level,
                    format,
                    &mut a,
                    ptr_of(&mut buf),
                    n as c_int,
                    &mut p,
                )
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
            &mut a,
            big.as_mut_ptr().cast(),
            8192,
            &mut p,
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
        })
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
                    )
                };
            }
            // SAFETY: the general log; `"%s"` consumes the one live string.
            unsafe {
                ffi::av_log(
                    ptr::null_mut(),
                    Level::ERROR.raw(),
                    c"%s".as_ptr(),
                    c"before".as_ptr(),
                );
                ffi::av_log(
                    ptr::null_mut(),
                    Level::ERROR.raw(),
                    c"%s".as_ptr(),
                    c"".as_ptr(),
                );
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
        // SAFETY: each format's conversions match the arguments passed: an
        // `int`, a live string and a `double`, or a string alone.
        unsafe {
            if i % 3 == 2 {
                ffi::av_log(
                    ptr::null_mut(),
                    level,
                    c"only %s".as_ptr(),
                    c"text".as_ptr(),
                );
            } else {
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
                )
            };
        }
    }
}

#[test]
fn records_format_c_arguments_as_the_c_functions() {
    let _s = LogState::lock();
    let rust = capture(Via::Rust, log_formats);
    let c = capture(Via::C, log_formats);
    assert_eq!(rust.len(), 20);
    assert_eq!(rust, c);
}

#[test]
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
                            &mut state,
                            c"%s".as_ptr(),
                            msg.as_ptr(),
                        )
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
fn contexts_shift_levels_and_prefix_lines_like_c() {
    let _s = LogState::lock();
    let child_class = log_class(c"child");
    let parent_class = log_class(c"parent");
    let mut parent = LogCtx {
        class: &parent_class,
        level_offset: 0,
        parent: ptr::null_mut(),
    };
    let mut ctx = LogCtx {
        class: &child_class,
        level_offset: 8,
        parent: (&raw mut parent).cast(),
    };
    let mut orphan = LogCtx {
        class: &child_class,
        level_offset: -16,
        parent: ptr::null_mut(),
    };
    let mut classless = LogCtx {
        class: ptr::null(),
        level_offset: 0,
        parent: ptr::null_mut(),
    };
    let contexts: [*mut c_void; 4] = [
        (&raw mut ctx).cast(),
        (&raw mut orphan).cast(),
        (&raw mut classless).cast(),
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
                        // SAFETY: `cx` is NULL or a live `LogCtx` whose class
                        // pointer is NULL or a well-formed class with no
                        // callbacks, an `int` at its level offset and a NULL
                        // or valid parent context; nothing writes them.
                        unsafe {
                            log::log_with_context(cx, level, msg);
                            let text = msg.to_str().unwrap();
                            log::log_fmt_with_context(cx, level, format_args!("{text}"));
                            log::log_once_with_context(cx, level, Level::TRACE, &mut once, msg);
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
                        // SAFETY: as above; `"%s"` consumes the one live string
                        // passed, and `once` is a writable `int`.
                        unsafe {
                            ffi::av_log(cx, level.raw(), c"%s".as_ptr(), msg.as_ptr());
                            ffi::av_log(cx, level.raw(), c"%s".as_ptr(), msg.as_ptr());
                            ffi::av_log_once(
                                cx,
                                level.raw(),
                                Level::TRACE.raw(),
                                &mut once,
                                c"%s".as_ptr(),
                                msg.as_ptr(),
                            );
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
/// `\n` report, as stderr is no terminal), `\r` lines and control-character
/// sanitizing.
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
