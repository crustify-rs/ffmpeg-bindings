//! UB tests for `libavutil/log.h`'s unsafe context logging, each call within
//! its `# Safety` contract: NULL, class-less and parented contexts, item-name
//! and category callbacks, level offsets up to the overflow bound, through a
//! formatting Rust callback and through the default callback in every colour
//! mode.

use core::ffi::{CStr, c_char, c_int, c_void};
use core::mem::offset_of;
use core::ptr;
use std::cell::RefCell;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::thread;

use ffmpeg::libavutil::ffi;
use ffmpeg::libavutil::log::{self, Callback, Flags, Level, Record};

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

/// A logging context: its class, the level offset its class locates, and a
/// parent context.
#[repr(C)]
struct Ctx {
    class: *const ffi::AVClass,
    level_offset: c_int,
    parent: *mut c_void,
}

/// The current `LIBAVUTIL_VERSION_INT`, which enables every class member.
const VERSION: c_int = (ffi::LIBAVUTIL_VERSION_MAJOR << 16
    | ffi::LIBAVUTIL_VERSION_MINOR << 8
    | ffi::LIBAVUTIL_VERSION_MICRO) as c_int;

/// Names a context by its level offset, through a static string.
unsafe extern "C" fn item_name(ctx: *mut c_void) -> *const c_char {
    // SAFETY: C calls this only with a `Ctx` of a class naming it.
    let offset = unsafe { (*ctx.cast::<Ctx>()).level_offset };
    if offset % 2 == 0 {
        c"even"
    } else {
        c"odd-named-item"
    }
    .as_ptr()
}

/// Reports the highest category below `AV_CLASS_CATEGORY_NB`.
unsafe extern "C" fn get_category(_: *mut c_void) -> ffi::AVClassCategory {
    ffi::AVClassCategory::AV_CLASS_CATEGORY_DEVICE_INPUT
}

/// The classes the contexts use: plain, with callbacks, with a category C
/// rejects (`NB` and above), too old for offsets and categories, and of a
/// release (micro 100) too old for categories, which C checks apart.
fn classes() -> [ffi::AVClass; 5] {
    let base = ffi::AVClass {
        class_name: c"plain".as_ptr(),
        version: VERSION,
        log_level_offset_offset: offset_of!(Ctx, level_offset) as c_int,
        parent_log_context_offset: offset_of!(Ctx, parent) as c_int,
        category: ffi::AVClassCategory::AV_CLASS_CATEGORY_MUXER,
        ..Default::default()
    };
    [
        base,
        ffi::AVClass {
            class_name: c"callbacks".as_ptr(),
            item_name: Some(item_name),
            get_category: Some(get_category),
            ..base
        },
        ffi::AVClass {
            class_name: c"bad-category".as_ptr(),
            category: ffi::AVClassCategory(1000),
            ..base
        },
        ffi::AVClass {
            class_name: c"old".as_ptr(),
            version: 50 << 16,
            ..base
        },
        ffi::AVClass {
            class_name: c"old-release".as_ptr(),
            version: 50 << 16 | 100,
            ..base
        },
    ]
}

/// Level offsets, with the levels each may be logged at without overflowing
/// an `int`: C adds the offset from `FATAL` up.
const OFFSETS: [(c_int, &[c_int]); 5] = [
    (0, &[c_int::MIN, -8, 0, 8, 56, c_int::MAX]),
    (8, &[-8, 0, 8, 48]),
    (-16, &[-8, 8, 16, c_int::MAX]),
    (c_int::MAX - 8, &[c_int::MIN, -8, 7, 8]),
    (c_int::MIN, &[-8, 8, 56, c_int::MAX]),
];

/// Every context shape over `classes`, with `offset`: NULL, a NULL class, a
/// class without a parent, with a parent of each class, and with a parent
/// whose class is NULL. Returned with the parents, which must outlive them;
/// neither vector grows after it is built, so the addresses stay put.
fn contexts(classes: &[ffi::AVClass; 5], offset: c_int) -> (Vec<Ctx>, Vec<Ctx>) {
    let mut parents: Vec<Ctx> = classes
        .iter()
        .map(|c| Ctx {
            class: c,
            level_offset: 1,
            parent: ptr::null_mut(),
        })
        .chain([Ctx {
            class: ptr::null(),
            level_offset: 0,
            parent: ptr::null_mut(),
        }])
        .collect();
    let mut out = vec![Ctx {
        class: ptr::null(),
        level_offset: offset,
        parent: ptr::null_mut(),
    }];
    for class in classes {
        out.push(Ctx {
            class,
            level_offset: offset,
            parent: ptr::null_mut(),
        });
        for p in &mut parents {
            out.push(Ctx {
                class,
                level_offset: offset,
                parent: (&raw mut *p).cast(),
            });
        }
    }
    (out, parents)
}

thread_local! {
    /// Lines this thread formatted while it captures.
    static LINES: RefCell<Option<Vec<(Level, usize, CStrBuf)>>> = const { RefCell::new(None) };
}

type CStrBuf = std::ffi::CString;

/// Formats each record a capturing thread logs through every method.
struct FormatAll;

impl Callback for FormatAll {
    fn log(record: Record<'_>) {
        if LINES.with(|l| l.borrow().is_none()) {
            return;
        }
        let mut prefix = true;
        let line = record.to_line(&mut prefix).unwrap_or_default();
        for n in [0, 1, 5, 40] {
            let mut buf = vec![0u8; n].into_boxed_slice();
            let _ = record.format_line(&mut buf, &mut true);
            let _ = record.write_line(&mut buf, &mut false);
        }
        let entry = (record.level(), record.context() as usize, line);
        LINES.with(|l| l.borrow_mut().as_mut().unwrap().push(entry));
    }
}

/// Runs `f` with [`FormatAll`] installed; returns this thread's lines.
fn capture(f: impl FnOnce()) -> Vec<(Level, usize, CStrBuf)> {
    LINES.with(|l| *l.borrow_mut() = Some(Vec::new()));
    log::set_callback::<FormatAll>();
    f();
    log::set_default_callback();
    LINES.with(|l| l.borrow_mut().take().unwrap())
}

/// Forwards every record to the default callback.
struct Forward;

impl Callback for Forward {
    fn log(record: Record<'_>) {
        record.log_default();
    }
}

/// Set in the child process an isolated test re-runs itself in.
const ISOLATED: &str = "CRUSTIFY_UB_ISOLATED";

/// Re-runs test `name` in a child per colour mode, its stderr discarded, and
/// returns `true` in the child.
fn in_colour_mode_children(name: &str) -> bool {
    if std::env::var_os(ISOLATED).is_some() {
        return true;
    }
    let modes: [&[(&str, &str)]; 3] = [
        &[("AV_LOG_FORCE_NOCOLOR", "1")],
        &[("AV_LOG_FORCE_COLOR", "1")],
        &[("AV_LOG_FORCE_COLOR", "1"), ("AV_LOG_FORCE_256COLOR", "1")],
    ];
    for env in modes {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", name, "--test-threads=1"])
            .env(ISOLATED, "1")
            .envs(env.iter().copied())
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(status.success(), "{env:?}: {status}");
    }
    false
}

/// `log::log_with_context`, within its contract.
mod log_with_context {
    use super::*;

    /// Logs `msg` with `ctx` at each level its offset allows.
    fn log_all(ctx: &mut Ctx, msg: &CStr) {
        let levels = OFFSETS
            .iter()
            .find(|(o, _)| *o == ctx.level_offset)
            .unwrap()
            .1;
        for &level in levels {
            // SAFETY: `ctx` is a live `Ctx` whose class is NULL or one of
            // `classes()`: names and callbacks valid for it, a category the
            // callback keeps below NB, an `int` at the level offset that
            // `level` plus it does not overflow from FATAL up, and a NULL or
            // live parent context; nothing writes them during the call.
            unsafe { log::log_with_context((&raw mut *ctx).cast(), Level::from_raw(level), msg) };
        }
    }

    #[test]
    fn null_context_is_the_general_log() {
        let _s = LogState::lock();
        let lines = capture(|| {
            // SAFETY: a NULL context is always valid.
            unsafe { log::log_with_context(ptr::null_mut(), Level::ERROR, c"general\n") };
        });
        assert_eq!(lines.len(), 1);
        assert_eq!((lines[0].0, lines[0].1), (Level::ERROR, 0));
        assert_eq!(lines[0].2.as_bytes(), b"general\n");
    }

    #[test]
    fn every_context_shape_formats_through_a_rust_callback() {
        let _s = LogState::lock();
        let classes = classes();
        log::set_flags(Flags::PRINT_LEVEL);
        for (offset, levels) in OFFSETS {
            let (mut ctxs, _parents) = contexts(&classes, offset);
            let lines = capture(|| {
                for ctx in &mut ctxs {
                    log_all(ctx, c"part ");
                    log_all(ctx, c"end\n");
                }
            });
            assert_eq!(lines.len(), ctxs.len() * levels.len() * 2);
        }
    }

    #[test]
    fn offsets_shift_levels_from_fatal_up() {
        let _s = LogState::lock();
        let classes = classes();
        let mut ctx = Ctx {
            class: &classes[0],
            level_offset: c_int::MAX - 8,
            parent: ptr::null_mut(),
        };
        let lines = capture(|| log_all(&mut ctx, c"x\n"));
        let levels: Vec<c_int> = lines.iter().map(|l| l.0.raw()).collect();
        assert_eq!(levels, [c_int::MIN, -8, 7, c_int::MAX]);
        let mut old = Ctx {
            class: &classes[3],
            level_offset: c_int::MIN,
            parent: ptr::null_mut(),
        };
        let lines = capture(|| log_all(&mut old, c"x\n"));
        let levels: Vec<c_int> = lines.iter().map(|l| l.0.raw()).collect();
        assert_eq!(levels, [-8, 8, 56, c_int::MAX], "too old for offsets");
    }

    #[test]
    fn contexts_are_shared_across_logging_threads() {
        let _s = LogState::lock();
        let classes = classes();
        let (ctxs, parents) = contexts(&classes, 8);
        let addrs: Vec<usize> = ctxs.iter().map(|c| &raw const *c as usize).collect();
        log::set_callback::<FormatAll>();
        thread::scope(|s| {
            for _ in 0..4 {
                let addrs = &addrs;
                s.spawn(move || {
                    for &a in addrs {
                        // SAFETY: as in `log_all`; the contexts outlive the
                        // scope and no thread writes them.
                        unsafe {
                            log::log_with_context(a as *mut c_void, Level::ERROR, c"shared\n")
                        };
                    }
                });
            }
        });
        drop((ctxs, parents));
    }

    /// The default callback reads names and categories, and with colour
    /// indexes its table by category.
    #[test]
    fn every_context_shape_prints_through_the_default_callback() {
        if !in_colour_mode_children(
            "log::log_with_context::every_context_shape_prints_through_the_default_callback",
        ) {
            return;
        }
        let classes = classes();
        log::set_callback::<Forward>();
        log::set_level(Level::TRACE);
        log::set_flags(Flags::PRINT_LEVEL | Flags::SKIP_REPEATED);
        for (offset, _) in OFFSETS {
            let (mut ctxs, _parents) = contexts(&classes, offset);
            for ctx in &mut ctxs {
                log_all(ctx, c"default\n");
            }
        }
    }
}

/// `log::log_fmt_with_context` (`av_log`), within its contract.
mod log_fmt_with_context {
    use super::*;

    /// Logs a formatted line with `ctx` at each level its offset allows.
    fn log_all(ctx: &mut Ctx, n: usize) {
        let levels = OFFSETS
            .iter()
            .find(|(o, _)| *o == ctx.level_offset)
            .unwrap()
            .1;
        for &level in levels {
            // SAFETY: as for `log_with_context` in its suite's `log_all`.
            unsafe {
                log::log_fmt_with_context(
                    (&raw mut *ctx).cast(),
                    Level::from_raw(level),
                    format_args!("{n:>4} %s %n {:.3}\0hidden\n", 1.0 / 3.0),
                )
            };
        }
    }

    #[test]
    fn null_context_is_the_general_log() {
        let _s = LogState::lock();
        let lines = capture(|| {
            // SAFETY: a NULL context is always valid.
            unsafe {
                log::log_fmt_with_context(ptr::null_mut(), Level::ERROR, format_args!("{}\n", 7))
            };
        });
        assert_eq!(lines.len(), 1);
        assert_eq!((lines[0].0, lines[0].1), (Level::ERROR, 0));
        assert_eq!(lines[0].2.as_bytes(), b"7\n");
    }

    #[test]
    fn every_context_shape_formats_through_a_rust_callback() {
        let _s = LogState::lock();
        let classes = classes();
        for (offset, levels) in OFFSETS {
            let (mut ctxs, _parents) = contexts(&classes, offset);
            let lines = capture(|| {
                for (n, ctx) in ctxs.iter_mut().enumerate() {
                    log_all(ctx, n);
                }
            });
            assert_eq!(lines.len(), ctxs.len() * levels.len());
            // The text stops at the NUL, as C's `%s` reads it.
            assert!(lines.iter().all(|l| l.2.as_bytes().ends_with(b" 0.333")));
        }
    }

    #[test]
    fn contexts_are_shared_across_logging_threads() {
        let _s = LogState::lock();
        let classes = classes();
        let (ctxs, parents) = contexts(&classes, 8);
        let addrs: Vec<usize> = ctxs.iter().map(|c| &raw const *c as usize).collect();
        log::set_callback::<FormatAll>();
        thread::scope(|s| {
            for t in 0..4 {
                let addrs = &addrs;
                s.spawn(move || {
                    for &a in addrs {
                        // SAFETY: as in `log_all`; the contexts outlive the
                        // scope and no thread writes them.
                        unsafe {
                            log::log_fmt_with_context(
                                a as *mut c_void,
                                Level::ERROR,
                                format_args!("thread {t}\n"),
                            )
                        };
                    }
                });
            }
        });
        drop((ctxs, parents));
    }

    #[test]
    fn every_context_shape_prints_through_the_default_callback() {
        if !in_colour_mode_children(
            "log::log_fmt_with_context::every_context_shape_prints_through_the_default_callback",
        ) {
            return;
        }
        let classes = classes();
        log::set_callback::<Forward>();
        log::set_level(Level::TRACE);
        for (offset, _) in OFFSETS {
            let (mut ctxs, _parents) = contexts(&classes, offset);
            for (n, ctx) in ctxs.iter_mut().enumerate() {
                log_all(ctx, n);
            }
        }
    }
}

/// `log::log_once_with_context`, within its contract.
mod log_once_with_context {
    use super::*;

    /// `log_once_with_context` for each level `ctx`'s offset allows, as
    /// initial and subsequent level, from both starting states; returns the
    /// states after each call.
    fn once_all(ctx: &mut Ctx) -> Vec<bool> {
        let levels = OFFSETS
            .iter()
            .find(|(o, _)| *o == ctx.level_offset)
            .unwrap()
            .1;
        let mut states = Vec::new();
        for &a in levels {
            for &b in levels {
                for start in [false, true] {
                    let mut state = start;
                    for _ in 0..2 {
                        // SAFETY: as for `log_with_context` in `log_all`;
                        // `state` is this call's own.
                        unsafe {
                            log::log_once_with_context(
                                (&raw mut *ctx).cast(),
                                Level::from_raw(a),
                                Level::from_raw(b),
                                &mut state,
                                c"once\n",
                            )
                        };
                        states.push(state);
                    }
                }
            }
        }
        states
    }

    #[test]
    fn null_context_moves_the_state() {
        let _s = LogState::lock();
        let mut state = false;
        let lines = capture(|| {
            for _ in 0..3 {
                // SAFETY: a NULL context is always valid.
                unsafe {
                    log::log_once_with_context(
                        ptr::null_mut(),
                        Level::ERROR,
                        Level::DEBUG,
                        &mut state,
                        c"o\n",
                    )
                };
            }
        });
        assert!(state);
        let levels: Vec<Level> = lines.iter().map(|l| l.0).collect();
        assert_eq!(levels, [Level::ERROR, Level::DEBUG, Level::DEBUG]);
    }

    #[test]
    fn every_context_shape_and_state() {
        let _s = LogState::lock();
        let classes = classes();
        for (offset, _) in OFFSETS {
            let (mut ctxs, _parents) = contexts(&classes, offset);
            let lines = capture(|| {
                for ctx in &mut ctxs {
                    assert!(once_all(ctx).into_iter().all(|s| s));
                }
            });
            assert!(!lines.is_empty());
        }
    }

    #[test]
    fn states_are_per_thread_and_contexts_shared() {
        let _s = LogState::lock();
        let classes = classes();
        let (ctxs, parents) = contexts(&classes, -16);
        let addrs: Vec<usize> = ctxs.iter().map(|c| &raw const *c as usize).collect();
        log::set_callback::<FormatAll>();
        thread::scope(|s| {
            for _ in 0..4 {
                let addrs = &addrs;
                s.spawn(move || {
                    for &a in addrs {
                        let mut state = false;
                        for _ in 0..3 {
                            // SAFETY: as in `once_all`; the contexts outlive
                            // the scope, no thread writes them, and `state`
                            // is this thread's own.
                            unsafe {
                                log::log_once_with_context(
                                    a as *mut c_void,
                                    Level::WARNING,
                                    Level::TRACE,
                                    &mut state,
                                    c"t\n",
                                )
                            };
                        }
                        assert!(state);
                    }
                });
            }
        });
        drop((ctxs, parents));
    }

    #[test]
    fn every_context_shape_prints_through_the_default_callback() {
        if !in_colour_mode_children(
            "log::log_once_with_context::every_context_shape_prints_through_the_default_callback",
        ) {
            return;
        }
        let classes = classes();
        log::set_callback::<Forward>();
        log::set_level(Level::TRACE);
        let (mut ctxs, _parents) = contexts(&classes, 0);
        for ctx in &mut ctxs {
            once_all(ctx);
        }
    }
}

/// `log::log_with_context` (`av_vlog`) through the default log callback,
/// within its contract: level offsets up to the overflow bound, old classes,
/// parents and threads.
mod vlog {
    use core::ffi::{CStr, c_char, c_int, c_void};
    use core::mem::offset_of;
    use core::ptr;
    use std::thread;

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::log::{Level, log_with_context};

    const INFO: c_int = ffi::AV_LOG_INFO as c_int;
    const DEBUG: c_int = ffi::AV_LOG_DEBUG as c_int;
    const FATAL: c_int = ffi::AV_LOG_FATAL as c_int;

    /// The first class version `av_vlog` applies a level offset for.
    const OFFSET_VERSION: c_int = (50 << 16) | (15 << 8) | 2;
    /// A version the default callback asks for a category.
    const CATEGORY_VERSION: c_int = (61 << 16) | (6 << 8) | 100;

    /// A context with a level offset and a parent slot, heap-allocated so a read
    /// past it reaches the sanitizers.
    #[repr(C)]
    struct Ctx {
        class: *const ffi::AVClass,
        level_offset: c_int,
        parent: *mut *const ffi::AVClass,
    }

    impl Ctx {
        fn boxed(class: *const ffi::AVClass, level_offset: c_int) -> Box<Self> {
            Box::new(Self {
                class,
                level_offset,
                parent: ptr::null_mut(),
            })
        }

        fn as_obj(&mut self) -> *mut c_void {
            (self as *mut Self).cast()
        }
    }

    /// The context's class name, as `av_default_item_name` gives it.
    unsafe extern "C" fn item_name(ctx: *mut c_void) -> *const c_char {
        // SAFETY: C passes a live context, whose first member is its class.
        unsafe { (*ctx.cast::<*const ffi::AVClass>().read()).class_name }
    }

    /// A category read from the context.
    unsafe extern "C" fn get_category(_ctx: *mut c_void) -> ffi::AVClassCategory {
        ffi::AVClassCategory::AV_CLASS_CATEGORY_FILTER
    }

    fn class(version: c_int, offset: bool, parent: bool) -> Box<ffi::AVClass> {
        Box::new(ffi::AVClass {
            class_name: c"crustify-vlog-unsafe".as_ptr(),
            item_name: Some(item_name),
            version,
            log_level_offset_offset: if offset {
                offset_of!(Ctx, level_offset) as c_int
            } else {
                0
            },
            parent_log_context_offset: if parent {
                offset_of!(Ctx, parent) as c_int
            } else {
                0
            },
            get_category: Some(get_category),
            ..Default::default()
        })
    }

    /// Logs `message` at each of `levels` in `obj`'s context.
    ///
    /// # Safety
    ///
    /// As [`log_with_context`], for every level.
    unsafe fn log_all(obj: *mut c_void, levels: &[c_int], message: &CStr) {
        for &level in levels {
            // SAFETY: the caller's contract.
            unsafe { log_with_context(obj, Level::from_raw(level), message) };
        }
    }

    #[test]
    fn null_context_and_null_class() {
        // SAFETY: NULL is the general log.
        unsafe {
            log_all(
                ptr::null_mut(),
                &[DEBUG, ffi::AV_LOG_TRACE as c_int],
                c"%s%n",
            )
        };
        let mut ctx = Ctx::boxed(ptr::null(), 0);
        // SAFETY: a context whose class pointer is NULL.
        unsafe {
            log_all(
                ctx.as_obj(),
                &[DEBUG, INFO],
                c"crustify ub_unsafe vlog: classless\n",
            )
        };
    }

    #[test]
    fn level_offsets_up_to_the_overflow_bound() {
        let cls = class(OFFSET_VERSION, true, false);
        let trace = ffi::AV_LOG_TRACE as c_int;
        for offset in [c_int::MIN, -1000, -FATAL, 0, 1, 1000, c_int::MAX - trace] {
            let mut ctx = Ctx::boxed(&*cls, offset);
            // SAFETY: `level + offset` stays within `c_int` for these levels, from
            // `FATAL + c_int::MIN` up to exactly `c_int::MAX`.
            unsafe {
                log_all(
                    ctx.as_obj(),
                    &[FATAL, DEBUG, trace],
                    c"crustify ub_unsafe vlog: %%%s\n",
                )
            };
        }
        let mut ctx = Ctx::boxed(&*cls, c_int::MAX - 0xff);
        // SAFETY: levels below `AV_LOG_FATAL` take no offset, and `0xff` sums to
        // exactly `c_int::MAX`.
        unsafe {
            log_all(
                ctx.as_obj(),
                &[ffi::AV_LOG_PANIC as c_int, ffi::AV_LOG_QUIET - 1000, 0xff],
                c"crustify ub_unsafe vlog: below FATAL\n",
            )
        };
    }

    #[test]
    fn offsets_of_old_classes_are_ignored() {
        let old = class(OFFSET_VERSION - 1, true, false);
        let mut ctx = Ctx::boxed(&*old, c_int::MAX);
        // SAFETY: the class predates level offsets, so the `c_int::MAX` offset is
        // never added; the levels the default callback drops.
        unsafe { log_all(ctx.as_obj(), &[DEBUG, c_int::MAX], c"old class") };
    }

    #[test]
    fn printed_lines_name_the_context_and_its_parent() {
        let parent_class = class(CATEGORY_VERSION, false, false);
        let mut parent: *const ffi::AVClass = &*parent_class;
        let child_class = class(CATEGORY_VERSION, true, true);
        let mut ctx = Ctx::boxed(&*child_class, -8);
        for slot in [ptr::null_mut(), &mut parent as *mut _] {
            ctx.parent = slot;
            // SAFETY: a well-formed context whose parent slot is NULL or points
            // at a classed parent; the offset keeps every level in range.
            unsafe {
                log_all(
                    ctx.as_obj(),
                    &[INFO + 8, DEBUG],
                    c"crustify ub_unsafe vlog: child\n",
                )
            };
        }
        let mut null_parent: *const ffi::AVClass = ptr::null();
        ctx.parent = &mut null_parent;
        // SAFETY: the parent slot points to a NULL class pointer.
        unsafe {
            log_all(
                ctx.as_obj(),
                &[INFO + 8],
                c"crustify ub_unsafe vlog: orphan\n",
            )
        };
    }

    #[test]
    fn one_context_logged_from_many_threads() {
        let cls = class(CATEGORY_VERSION, true, false);
        let ctx = Ctx::boxed(&*cls, 16);
        let addr = Box::into_raw(ctx) as usize;
        let workers: Vec<_> = (0..8)
            .map(|i| {
                thread::spawn(move || {
                    for j in 0..200 {
                        let level = if i == 0 && j == 0 { FATAL } else { DEBUG };
                        // SAFETY: the context and class outlive the threads,
                        // which only read them; the offset keeps levels in range.
                        unsafe {
                            log_with_context(
                                addr as *mut c_void,
                                Level::from_raw(level),
                                c"crustify ub_unsafe vlog: threads\n",
                            )
                        };
                    }
                })
            })
            .collect();
        for w in workers {
            w.join().unwrap();
        }
        // SAFETY: `addr` came from `Box::into_raw`, and the threads are done.
        drop(unsafe { Box::from_raw(addr as *mut Ctx) });
        drop(cls);
    }
}

/// UB tests for `libavutil/log.h`'s unsafe `default_item_name`, called within
/// its `# Safety` contract: static and heap classes, NULL names, objects
/// larger than their class pointer, and concurrent readers.
mod item_name {
    use core::ffi::{CStr, c_void};
    use core::ptr::null;
    use std::ffi::CString;

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::log::default_item_name;

    /// A class, `Sync` to live in a `static`.
    struct Class(ffi::AVClass);
    // SAFETY: the class is immutable and its name is a string literal.
    unsafe impl Sync for Class {}

    static NAMED: Class = Class(ffi::AVClass {
        class_name: c"ub-unsafe-class".as_ptr(),
        // SAFETY: every other member is an integer, a nullable pointer or an
        // `Option` of a function pointer, for which all-zero is valid (NULL).
        ..unsafe { core::mem::zeroed() }
    });

    /// An `AVClass`-enabled object with a payload after its class pointer.
    #[repr(C)]
    struct Obj {
        class: *const ffi::AVClass,
        payload: [u8; 64],
    }

    impl Obj {
        fn new(class: *const ffi::AVClass) -> Self {
            Self {
                class,
                payload: [0xa5; 64],
            }
        }

        fn ctx(&self) -> *const c_void {
            (self as *const Self).cast()
        }
    }

    #[test]
    fn static_class_name_lives_for_static() {
        let name: &'static CStr = {
            let obj = Obj::new(&NAMED.0);
            // SAFETY: `obj` starts with a non-NULL pointer to the live static
            // class, whose name is a literal, so any lifetime holds.
            unsafe { default_item_name(obj.ctx()) }.unwrap()
        };
        // The name outlives the object that reached it.
        assert_eq!(name, c"ub-unsafe-class");
    }

    #[test]
    fn bare_class_pointer_is_the_smallest_object() {
        let fake: *const ffi::AVClass = &NAMED.0;
        // SAFETY: `fake` is readable memory starting with a non-NULL class
        // pointer to the live static class.
        let name = unsafe { default_item_name((&raw const fake).cast()) };
        assert_eq!(name, Some(c"ub-unsafe-class"));
    }

    #[test]
    fn class_pointer_slot_inside_a_larger_object() {
        /// An object whose class pointer is an aligned member, but not its
        /// allocation's first byte: the context points at the member.
        #[repr(C)]
        struct Outer {
            prefix: [u8; 24],
            class: *const ffi::AVClass,
            payload: u64,
        }

        let outer = Box::new(Outer {
            prefix: [0x5a; 24],
            class: &NAMED.0,
            payload: 7,
        });
        let ctx: *const c_void = (&raw const outer.class).cast();
        // SAFETY: `ctx` is the aligned `class` member of the live `outer`,
        // holding a non-NULL pointer to the static class whose name is a
        // literal; no other thread writes either.
        let name = unsafe { default_item_name(ctx) };
        assert_eq!(name, Some(c"ub-unsafe-class"));
        assert_eq!((outer.prefix[0], outer.payload), (0x5a, 7));
    }

    #[test]
    fn null_class_name_reads_as_none() {
        let cls = ffi::AVClass {
            class_name: null(),
            ..Default::default()
        };
        let obj = Obj::new(&cls);
        // SAFETY: `obj` starts with a non-NULL pointer to the live `cls`, which
        // stores NULL as its name.
        assert_eq!(unsafe { default_item_name(obj.ctx()) }, None);
    }

    #[test]
    fn heap_name_is_read_within_its_owner() {
        for text in ["", "h", "heap-allocated-class-name", &"n".repeat(4000)] {
            let name = CString::new(text).unwrap();
            let cls = Box::new(ffi::AVClass {
                class_name: name.as_ptr(),
                ..Default::default()
            });
            let obj = Box::new(Obj::new(&*cls));
            {
                // SAFETY: `obj` starts with a non-NULL pointer to the live `cls`,
                // whose name `name` stays alive and unmodified for this block.
                let read = unsafe { default_item_name(obj.ctx()) }.unwrap();
                assert_eq!(read.to_bytes(), text.as_bytes());
            }
            drop(obj);
            drop(cls);
            drop(name);
        }
    }

    #[test]
    fn object_class_switches_between_calls() {
        let other = ffi::AVClass {
            class_name: c"other".as_ptr(),
            ..Default::default()
        };
        let mut obj = Obj::new(&NAMED.0);
        for (i, cls) in [&NAMED.0, &other, &NAMED.0].into_iter().enumerate() {
            obj.class = cls;
            // SAFETY: `obj` starts with a non-NULL pointer to a live class whose
            // name is a literal; no other thread writes `obj`.
            let name = unsafe { default_item_name(obj.ctx()) }.unwrap();
            assert_eq!(name.as_ptr(), cls.class_name, "step {i}");
        }
        assert_eq!(obj.payload, [0xa5; 64]);
    }

    #[test]
    fn concurrent_readers_share_an_object() {
        /// An object, `Sync` to share across threads.
        struct Shared(Obj);
        // SAFETY: the threads below only read the object and its static class.
        unsafe impl Sync for Shared {}

        let shared = Shared(Obj::new(&NAMED.0));
        std::thread::scope(|s| {
            for _ in 0..4 {
                let shared = &shared;
                s.spawn(move || {
                    for _ in 0..1_000 {
                        // SAFETY: the object starts with a non-NULL pointer to the
                        // static class; every thread only reads it.
                        let name = unsafe { default_item_name(shared.0.ctx()) };
                        assert_eq!(name, Some(c"ub-unsafe-class"));
                    }
                });
            }
        });
    }
}
