//! UB tests for `libavutil/log.h`'s context logging with raw contexts, each
//! adopted through the unsafe `ObjectRef::from_ptr` within its contract:
//! parented contexts, item-name and category callbacks, level offsets up to
//! the overflow bound, through a formatting Rust callback and through the
//! default callback in every colour mode. A NULL context, or one whose class
//! is NULL, is no object: it logs through the general log.

use core::ffi::{CStr, c_char, c_int, c_void};
use core::mem::offset_of;
use core::ptr;
use std::cell::RefCell;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::thread;

use ffmpeg::libavutil::ffi;
use ffmpeg::libavutil::log::{self, Callback, Flags, Level, Record};
use ffmpeg::libavutil::opt::ObjectRef;

/// The handle of the raw context `ctx`; `None` for NULL or a context whose
/// class pointer is NULL, which is no object.
///
/// # Safety
///
/// `ctx` is NULL, or satisfies `ObjectRef`'s contract for `'a` when its
/// class pointer is not NULL.
pub(crate) unsafe fn object<'a>(ctx: *mut c_void) -> Option<ObjectRef<'a>> {
    // SAFETY: the caller's contract.
    unsafe { ObjectRef::from_ptr(ctx) }
}

/// `log_with_context` in the context of `ctx`, or `log` for no object.
///
/// # Safety
///
/// As [`object`], for the call.
pub(crate) unsafe fn log_in(ctx: *mut c_void, level: Level, message: &CStr) {
    // SAFETY: the caller's contract.
    match unsafe { object(ctx) } {
        Some(obj) => log::log_with_context(obj, level, message),
        None => log::log(level, message),
    }
}

/// `log_fmt_with_context` in the context of `ctx`, or `log_fmt` for no
/// object.
///
/// # Safety
///
/// As [`object`], for the call.
unsafe fn log_fmt_in(ctx: *mut c_void, level: Level, args: core::fmt::Arguments<'_>) {
    // SAFETY: the caller's contract.
    match unsafe { object(ctx) } {
        Some(obj) => log::log_fmt_with_context(obj, level, args),
        None => log::log_fmt(level, args),
    }
}

/// `log_once_with_context` in the context of `ctx`, or `log_once` for no
/// object.
///
/// # Safety
///
/// As [`object`], for the call.
unsafe fn log_once_in(ctx: *mut c_void, a: Level, b: Level, state: &mut bool, message: &CStr) {
    // SAFETY: the caller's contract.
    match unsafe { object(ctx) } {
        Some(obj) => log::log_once_with_context(obj, a, b, state, message),
        None => log::log_once(a, b, state, message),
    }
}

/// Serializes the tests here that touch the process-wide log callback, level
/// or flags; restores the defaults when dropped.
pub(crate) struct LogState(#[allow(dead_code)] MutexGuard<'static, ()>);

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
    (c_int::MAX - 56, &[c_int::MIN, -8, 7, 56]),
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
            unsafe { log_in((&raw mut *ctx).cast(), Level::from_raw(level), msg) };
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn null_context_is_the_general_log() {
        let _s = LogState::lock();
        let lines = capture(|| {
            // SAFETY: a NULL context is always valid.
            unsafe { log_in(ptr::null_mut(), Level::ERROR, c"general\n") };
        });
        assert_eq!(lines.len(), 1);
        assert_eq!((lines[0].0, lines[0].1), (Level::ERROR, 0));
        assert_eq!(lines[0].2.as_bytes(), b"general\n");
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
    fn offsets_shift_levels_from_fatal_up() {
        let _s = LogState::lock();
        let classes = classes();
        let mut ctx = Ctx {
            class: &classes[0],
            level_offset: c_int::MAX - 56,
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
    #[cfg_attr(miri, ignore = "calls C")]
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
                        unsafe { log_in(a as *mut c_void, Level::ERROR, c"shared\n") };
                    }
                });
            }
        });
        drop((ctxs, parents));
    }

    /// The default callback reads names and categories, and with colour
    /// indexes its table by category.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
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

/// The level check of the context routines: a message whose level C would
/// shift past `INT_MAX` with the object's level offset is dropped before C
/// adds it, in each routine, at the bound and past it.
mod overflowing_shifts {
    use super::*;

    /// The levels each routine logged at, for `ctx` and every level of
    /// `levels`; `log_once_with_context` from both starting states, whose
    /// state ends set either way.
    fn logged(ctx: &mut Ctx, levels: &[c_int]) -> Vec<c_int> {
        // SAFETY: a live `Ctx` of `classes()[0]`, named, with an `int` at its
        // level offset within the bound and no parent; nothing writes it
        // while the handle lives.
        let obj = unsafe { object((&raw mut *ctx).cast()) }.unwrap();
        let lines = capture(|| {
            for &raw in levels {
                let level = Level::from_raw(raw);
                log::log_with_context(obj, level, c"plain\n");
                log::log_fmt_with_context(obj, level, format_args!("fmt\n"));
                for start in [false, true] {
                    let mut state = start;
                    log::log_once_with_context(obj, level, level, &mut state, c"once\n");
                    assert!(state);
                }
            }
        });
        lines.iter().map(|l| l.0.raw()).collect()
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn shifts_past_int_max_are_dropped() {
        let _s = LogState::lock();
        let classes = classes();
        let bound = c_int::MAX - ffi::AV_LOG_TRACE as c_int;
        let mut ctx = Ctx {
            class: &classes[0],
            level_offset: bound,
            parent: ptr::null_mut(),
        };
        // Below FATAL nothing is added; from FATAL up to TRACE the sum stays
        // within `int`; past TRACE it would overflow, and is dropped.
        let levels = [c_int::MIN, -8, 7, 8, 56, 57, 1000, c_int::MAX];
        let got = logged(&mut ctx, &levels);
        let want: Vec<c_int> = [c_int::MIN, -8, 7, bound + 8, c_int::MAX]
            .iter()
            .flat_map(|&l| [l; 4])
            .collect();
        assert_eq!(got, want);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn negative_offsets_never_overflow() {
        let _s = LogState::lock();
        let classes = classes();
        let mut ctx = Ctx {
            class: &classes[0],
            level_offset: c_int::MIN,
            parent: ptr::null_mut(),
        };
        let got = logged(&mut ctx, &[8, c_int::MAX]);
        let want: Vec<c_int> = [c_int::MIN + 8, -1].iter().flat_map(|&l| [l; 4]).collect();
        assert_eq!(got, want);
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
                log_fmt_in(
                    (&raw mut *ctx).cast(),
                    Level::from_raw(level),
                    format_args!("{n:>4} %s %n {:.3}\0hidden\n", 1.0 / 3.0),
                )
            };
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn null_context_is_the_general_log() {
        let _s = LogState::lock();
        let lines = capture(|| {
            // SAFETY: a NULL context is always valid.
            unsafe { log_fmt_in(ptr::null_mut(), Level::ERROR, format_args!("{}\n", 7)) };
        });
        assert_eq!(lines.len(), 1);
        assert_eq!((lines[0].0, lines[0].1), (Level::ERROR, 0));
        assert_eq!(lines[0].2.as_bytes(), b"7\n");
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
                            log_fmt_in(a as *mut c_void, Level::ERROR, format_args!("thread {t}\n"))
                        };
                    }
                });
            }
        });
        drop((ctxs, parents));
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
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
                            log_once_in(
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
    #[cfg_attr(miri, ignore = "calls C")]
    fn null_context_moves_the_state() {
        let _s = LogState::lock();
        let mut state = false;
        let lines = capture(|| {
            for _ in 0..3 {
                // SAFETY: a NULL context is always valid.
                unsafe {
                    log_once_in(
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
                                log_once_in(
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    use ffmpeg::libavutil::log::Level;

    use super::log_in;

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
    /// As [`log_in`], for every level.
    unsafe fn log_all(obj: *mut c_void, levels: &[c_int], message: &CStr) {
        for &level in levels {
            // SAFETY: the caller's contract.
            unsafe { log_in(obj, Level::from_raw(level), message) };
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
    fn offsets_of_old_classes_are_ignored() {
        let old = class(OFFSET_VERSION - 1, true, false);
        let mut ctx = Ctx::boxed(&*old, c_int::MAX);
        // SAFETY: the class predates level offsets, so the `c_int::MAX` offset is
        // never added; the levels the default callback drops.
        unsafe { log_all(ctx.as_obj(), &[DEBUG, c_int::MAX], c"old class") };
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
                            log_in(
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

/// UB tests for `libavutil/log.h`'s `default_item_name`, on raw objects
/// adopted within `ObjectRef::from_ptr`'s contract: static and heap classes,
/// objects larger than their class pointer, and concurrent readers.
mod item_name {
    use core::ffi::{CStr, c_void};
    use std::ffi::CString;

    use ffmpeg::libavutil::ffi;

    /// `default_item_name` of the raw object `ctx`.
    ///
    /// # Safety
    ///
    /// `ctx` satisfies `ObjectRef`'s contract for `'a`, with a class.
    unsafe fn default_item_name<'a>(ctx: *const c_void) -> Option<&'a CStr> {
        // SAFETY: the caller's contract.
        let obj = unsafe { super::object(ctx.cast_mut()) }.expect("a class");
        ffmpeg::libavutil::log::default_item_name(obj)
    }

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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
    fn bare_class_pointer_is_the_smallest_object() {
        let fake: *const ffi::AVClass = &NAMED.0;
        // SAFETY: `fake` is readable memory starting with a non-NULL class
        // pointer to the live static class.
        let name = unsafe { default_item_name((&raw const fake).cast()) };
        assert_eq!(name, Some(c"ub-unsafe-class"));
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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

/// The `log_with_context` route to a raw class's `ClassRef`.
#[path = "../common/class.rs"]
pub(crate) mod class_fixture;

/// UB tests of `AVClass`'s callbacks, run on raw objects adopted within
/// `ObjectRef::from_ptr`'s contract: `item_name` on Rust-built and C-named
/// classes, NULL names from the callback included, `get_category` returning
/// every category, `query_ranges` succeeding with one or several components
/// and failing, from several threads, and `child_next` walking an object's
/// children to the end.
mod class_callbacks {
    use core::ffi::{CStr, c_char, c_int, c_void};
    use core::ptr;
    use std::thread;

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::log::{Class, ClassCategory};
    use ffmpeg::libavutil::opt::RangeFlags;

    use super::class_fixture::with_class;
    use super::{LogState, object};
    use crate::opt::fixture::{CB_FAKE, CB_KEYS, EINVAL, RANGED, UNKNOWN};

    /// An object of a class: its class pointer first, then `children`.
    #[repr(C)]
    struct Obj {
        class: *const ffi::AVClass,
        children: [*mut c_void; 2],
    }

    impl Obj {
        fn new(class: *const ffi::AVClass) -> Self {
            Self {
                class,
                children: [ptr::null_mut(); 2],
            }
        }

        fn as_obj(&mut self) -> *mut c_void {
            (self as *mut Self).cast()
        }
    }

    /// A raw class named `name`, every other member zero.
    fn raw(name: Option<&'static CStr>) -> ffi::AVClass {
        ffi::AVClass {
            class_name: name.map_or(ptr::null(), CStr::as_ptr),
            ..Default::default()
        }
    }

    static OWN: Class = Class::new(c"ub-unsafe-class", ClassCategory::DEMUXER);
    static EMPTY: Class = Class::new(c"", ClassCategory::NA);

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn item_name_names_objects_of_rust_classes() {
        for class in [&OWN, &EMPTY] {
            let mut obj = Obj::new(class.as_c_ptr());
            // `Class::new` leaves `item_name` NULL, which C's logging reads as
            // `av_default_item_name`: name the object the way C then does.
            assert!(class.as_ref().item_name().is_none());
            use ffmpeg::libavutil::log::default_item_name;
            // SAFETY: `obj` is a live object whose first member points to
            // `class`, built by `Class::new`; the name is its `'static`
            // literal.
            let obj = unsafe { object(obj.as_obj()) }.unwrap();
            let got: Option<&'static CStr> = default_item_name(obj);
            assert_eq!(got, class.as_ref().name());
            assert_eq!(
                got.unwrap().as_ptr(),
                class.as_ref().name().unwrap().as_ptr()
            );
        }
    }

    unsafe extern "C" fn null_name(_: *mut c_void) -> *const c_char {
        ptr::null()
    }

    unsafe extern "C" fn any_category(obj: *mut c_void) -> ffi::AVClassCategory {
        // SAFETY: `obj` is an `Obj`, whose first child slot holds the value.
        let v = unsafe { (*obj.cast::<Obj>()).children[0] }.addr();
        ffi::AVClassCategory(v as u32)
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn item_name_and_category_follow_the_callbacks() {
        let mut null = raw(Some(c"null-item"));
        null.item_name = Some(null_name);
        null.get_category = Some(any_category);
        let mut obj = Obj::new(&null);
        let o = obj.as_obj();
        // Every category below `AV_CLASS_CATEGORY_NB`, as the contract asks.
        for v in [0, 7, 45] {
            // SAFETY: `o` is the live `obj`, written only between borrows.
            unsafe { (*o.cast::<Obj>()).children[0] = ptr::without_provenance_mut(v) };
            // SAFETY: `o` starts with a pointer to a named class without
            // options or offsets, whose callbacks accept it, do not log, and
            // return NULL and a category below NB; nothing writes it while the
            // handle lives.
            let handle = unsafe { object(o) }.unwrap();
            assert_eq!(handle.item_name(), None);
            assert_eq!(handle.category().value(), v as u32);
        }
        // Without callbacks: the class name and category.
        let plain = Obj::new(OWN.as_c_ptr());
        // SAFETY: `plain` starts with a pointer to a `Class::new` class.
        let handle = unsafe { object((&raw const plain).cast_mut().cast()) }.unwrap();
        assert_eq!(handle.item_name(), Some(c"ub-unsafe-class"));
        assert_eq!(handle.category(), ClassCategory::DEMUXER);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn query_ranges_builds_fails_and_reports_a_missing_list() {
        let _s = LogState::lock();
        let fake = ffi::AV_OPT_SEARCH_FAKE_OBJ as c_int;
        let multi = ffi::AV_OPT_MULTI_COMPONENT_RANGE as c_int;
        let obj = CB_FAKE.as_obj();
        // SAFETY: `obj` is a `const AVClass *` to the fixture's well-formed
        // class, a context `log_with_context` accepts; its callback builds
        // lists per `av_opt_query_ranges`' contract and accepts the fake
        // object with `AV_OPT_SEARCH_FAKE_OBJ`.
        unsafe {
            with_class(obj, |c| {
                let class = c.unwrap();
                assert!(class.query_ranges().is_some());
                for (flags, rflags) in [
                    (fake, RangeFlags::NONE),
                    (fake | multi, RangeFlags::MULTI_COMPONENT),
                ] {
                    for key in CB_KEYS.iter().chain(&RANGED).chain(&UNKNOWN) {
                        match class.option_ranges(key, rflags) {
                            Ok(list) => {
                                let r = list.as_ref();
                                let n = r.range_count() * r.component_count();
                                assert_eq!(r.ranges().len(), n);
                                if *key == c"size" {
                                    assert_eq!(
                                        r.component_count(),
                                        if flags & multi != 0 { 3 } else { 1 }
                                    );
                                }
                            }
                            Err(e) => assert!(e < 0 && (e != EINVAL || *key == c"fail")),
                        }
                    }
                }
                // Several threads at once: each list is independent.
                thread::scope(|s| {
                    for _ in 0..4 {
                        s.spawn(move || {
                            for key in RANGED {
                                drop(
                                    class
                                        .option_ranges(key, RangeFlags::MULTI_COMPONENT)
                                        .unwrap(),
                                );
                            }
                        });
                    }
                });
            });
        }
    }

    unsafe extern "C" fn next_child(obj: *mut c_void, prev: *mut c_void) -> *mut c_void {
        // SAFETY: `obj` is an `Obj` with two children.
        let children = unsafe { (*obj.cast::<Obj>()).children };
        match children.iter().position(|&c| c == prev) {
            _ if prev.is_null() => children[0],
            Some(i) if i + 1 < children.len() => children[i + 1],
            _ => ptr::null_mut(),
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn child_next_walks_the_children_to_the_end() {
        let _s = LogState::lock();
        let child_class = raw(Some(c"child"));
        let mut parent_class = raw(Some(c"parent"));
        parent_class.child_next = Some(next_child);
        let other_class = raw(Some(c"other-child"));
        let mut kids = [Obj::new(&child_class), Obj::new(&other_class)];
        let mut parent = Obj::new(&parent_class);
        parent.children = [kids[0].as_obj(), kids[1].as_obj()];
        let p = parent.as_obj();
        // SAFETY: `p` starts with a pointer to `parent_class`, named, whose
        // `child_next` accepts `p` and NULL or a child it returned, children
        // `p` owns of named classes; nothing writes them while it lives.
        let obj = unsafe { object(p) }.unwrap();
        let walked: Vec<_> = obj.children().map(|c| c.item_name().unwrap()).collect();
        assert_eq!(walked, [c"child", c"other-child"]);
    }
}

/// `default_category` (core wave 1, batch 2), on raw objects within
/// `ObjectRef::from_ptr`'s contract: static and heap classes, the smallest and an embedded class
/// pointer slot, every category value, a class switched between calls, and
/// concurrent readers.
mod default_category {
    use core::ffi::c_void;

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::log::ClassCategory;

    /// `default_category` of the raw object `ctx`.
    ///
    /// # Safety
    ///
    /// `ctx` satisfies `ObjectRef`'s contract for the call, with a class.
    unsafe fn default_category(ctx: *const c_void) -> ClassCategory {
        // SAFETY: the caller's contract.
        let obj = unsafe { super::object(ctx.cast_mut()) }.expect("a class");
        ffmpeg::libavutil::log::default_category(obj)
    }

    /// A class, `Sync` to live in a `static`.
    struct Class(ffi::AVClass);
    // SAFETY: the class is immutable and its name is a string literal.
    unsafe impl Sync for Class {}

    static DEMUXER: Class = Class(ffi::AVClass {
        class_name: c"ub-unsafe-demuxer".as_ptr(),
        category: ffi::AVClassCategory::AV_CLASS_CATEGORY_DEMUXER,
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
                payload: [0x3c; 64],
            }
        }

        fn ctx(&self) -> *const c_void {
            (self as *const Self).cast()
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn static_class_reads_its_category() {
        let obj = Obj::new(&DEMUXER.0);
        // SAFETY: `obj` starts with a non-NULL pointer to the live static
        // class; nothing writes either.
        let got = unsafe { default_category(obj.ctx()) };
        assert_eq!(got, ClassCategory::DEMUXER);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn bare_class_pointer_is_the_smallest_object() {
        let fake: *const ffi::AVClass = &DEMUXER.0;
        // SAFETY: `fake` is a readable, aligned slot holding a non-NULL
        // pointer to the live static class.
        let got = unsafe { default_category((&raw const fake).cast()) };
        assert_eq!(got, ClassCategory::DEMUXER);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn class_pointer_slot_inside_a_larger_object() {
        /// An object whose class pointer is an aligned member, not its
        /// allocation's first byte: the context points at the member.
        #[repr(C)]
        struct Outer {
            prefix: [u8; 40],
            class: *const ffi::AVClass,
            payload: u64,
        }

        let outer = Box::new(Outer {
            prefix: [0x5a; 40],
            class: &DEMUXER.0,
            payload: 9,
        });
        // SAFETY: the context is the aligned `class` member of the live
        // `outer`, holding a non-NULL pointer to the static class.
        let got = unsafe { default_category((&raw const outer.class).cast()) };
        assert_eq!(got, ClassCategory::DEMUXER);
        assert_eq!((outer.prefix[39], outer.payload), (0x5a, 9));
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn every_category_value_of_a_heap_class() {
        let values = (0..=u32::from(u8::MAX)).chain([1 << 16, u32::MAX - 1, u32::MAX]);
        for v in values {
            let cls = Box::new(ffi::AVClass {
                class_name: c"ub-unsafe-heap".as_ptr(),
                category: ffi::AVClassCategory(v),
                ..Default::default()
            });
            let obj = Box::new(Obj::new(&*cls));
            // SAFETY: `obj` starts with a non-NULL pointer to the live `cls`;
            // both are dropped only after the call.
            let got = unsafe { default_category(obj.ctx()) };
            assert_eq!(got.value(), v);
            drop(obj);
            drop(cls);
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn object_class_switches_between_calls() {
        let other = ffi::AVClass {
            class_name: c"other".as_ptr(),
            category: ffi::AVClassCategory::AV_CLASS_CATEGORY_ENCODER,
            ..Default::default()
        };
        let mut obj = Obj::new(&DEMUXER.0);
        for (i, cls) in [&DEMUXER.0, &other, &DEMUXER.0].into_iter().enumerate() {
            obj.class = cls;
            // SAFETY: `obj` starts with a non-NULL pointer to a live class; no
            // other thread writes `obj`.
            let got = unsafe { default_category(obj.ctx()) };
            assert_eq!(got, ClassCategory::from(cls.category), "step {i}");
        }
        assert_eq!(obj.payload, [0x3c; 64]);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn concurrent_readers_share_an_object() {
        /// An object, `Sync` to share across threads.
        struct Shared(Obj);
        // SAFETY: the threads below only read the object and its static class.
        unsafe impl Sync for Shared {}

        let shared = Shared(Obj::new(&DEMUXER.0));
        std::thread::scope(|s| {
            for _ in 0..4 {
                let shared = &shared;
                s.spawn(move || {
                    for _ in 0..1_000 {
                        // SAFETY: the object starts with a non-NULL pointer to
                        // the static class; every thread only reads it.
                        let got = unsafe { default_category(shared.0.ctx()) };
                        assert_eq!(got, ClassCategory::DEMUXER);
                    }
                });
            }
        });
    }
}
