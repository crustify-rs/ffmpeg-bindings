//! UB tests of `libavutil/log.h`'s category and state-flag values through the
//! safe API: every raw value, named or not, at the integer extremes; and of
//! logging: records formatted into exact-size buffers, re-entrant and
//! concurrent callbacks, and the default callback at every level, flag and
//! colour mode.

use core::ffi::{CStr, c_int, c_uint};
use std::cell::{Cell, RefCell};
use std::ffi::CString;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;

use ffmpeg::libavutil::ffi;
use ffmpeg::libavutil::log::{
    self, Callback, ClassCategory, ClassStateFlags, Flags, Level, Record,
};

/// Raw values at and around every boundary the wrappers distinguish.
const EDGES: [c_uint; 10] = [0, 1, 11, 12, 39, 40, 45, 46, c_uint::MAX - 1, c_uint::MAX];

#[test]
fn any_category_value_is_valid() {
    for v in EDGES {
        let c = ClassCategory::from_value(v);
        let raw: ffi::AVClassCategory = c.into();
        assert_eq!(ClassCategory::from(raw).value(), v);
        let _ = c.name();
        let _ = format!("{c:?}");
    }
}

/// The device predicates reach C; kept apart from the FFI-free tests above,
/// which also run under Miri.
#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn device_predicates_accept_any_value() {
    for v in EDGES {
        let c = ClassCategory::from_value(v);
        assert!(!(c.is_input_device() && c.is_output_device()));
    }
}

#[test]
fn any_state_flag_bits_are_valid() {
    for a in EDGES {
        let fa = ClassStateFlags::from_bits_retain(a);
        let _ = format!("{fa:?}");
        let _ = ClassStateFlags::from_bits(a);
        let _ = ClassStateFlags::from_bits_truncate(a);
        let raw: ffi::AVClassStateFlags = fa.into();
        assert_eq!(ClassStateFlags::from(raw), fa);
        for b in EDGES {
            let fb = ClassStateFlags::from_bits_retain(b);
            assert_eq!((fa | fb).bits(), a | b);
            assert_eq!((fa & fb).bits(), a & b);
            assert_eq!((fa - fb).bits(), a & !b);
            let mut f = fa;
            f.set(fb, true);
            f.set(fb, false);
            assert_eq!(f, fa - fb);
        }
    }
}

#[test]
fn values_cross_threads() {
    let c = ClassCategory::DEVICE_INPUT;
    let f = ClassStateFlags::INITIALIZED;
    let (c2, f2) = std::thread::spawn(move || (c, f | ClassStateFlags::from_bits_retain(2)))
        .join()
        .unwrap();
    assert_eq!(c2, c);
    assert_eq!(f2.bits(), 3);
}

/// Whether this run is under BSan, whose runtime makes exhaustive loops over
/// FFI calls take minutes.
fn under_bsan() -> bool {
    std::env::var("CRUSTIFY_FFMPEG_VARIANT").as_deref() == Ok("bsan")
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

/// What `format_line` returned and wrote into one buffer, and what
/// `write_line` returned for the same size.
type Sized = (Option<usize>, Box<[u8]>, Vec<u8>);

/// One record as formatted into every buffer size from 0 to its length + 2,
/// each an exact-size allocation: what `format_line` returned and wrote, and
/// what `write_line` returned.
struct Formatted {
    whole: Option<CString>,
    sized: Vec<Sized>,
}

thread_local! {
    /// Records formatted on this thread while it captures.
    static FORMATTED: RefCell<Option<Vec<Formatted>>> = const { RefCell::new(None) };
}

/// Formats every record a capturing thread logs; checks happen outside, as a
/// panic here would abort.
struct FormatAll;

impl Callback for FormatAll {
    fn log(record: Record<'_>) {
        if FORMATTED.with(|f| f.borrow().is_none()) {
            return;
        }
        let mut prefix = true;
        let whole = record.to_line(&mut prefix);
        let len = whole.as_ref().map_or(0, |w| w.as_bytes().len());
        let sized = (0..=len + 2)
            .filter(|&n| n < if under_bsan() { 6 } else { 20 } || n + 3 > len)
            .map(|n| {
                let mut buf = vec![0xa5_u8; n].into_boxed_slice();
                let mut p = true;
                let ret = record.format_line(&mut buf, &mut p);
                let mut out = vec![0xa5_u8; n].into_boxed_slice();
                let mut p = true;
                let written = record.write_line(&mut out, &mut p).to_bytes().to_vec();
                (ret, buf, written)
            })
            .collect();
        let _ = (record.level(), record.context(), format!("{record:?}"));
        FORMATTED.with(|f| {
            f.borrow_mut()
                .as_mut()
                .unwrap()
                .push(Formatted { whole, sized })
        });
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn records_format_into_exact_size_buffers() {
    if !isolated("log::records_format_into_exact_size_buffers") {
        return;
    }
    let _s = LogState::lock();
    let mut msgs: Vec<CString> = [0, 1, 2, 3, 7, 8, 15, 16, 31, 63, 64, 65, 79]
        .map(|n| CString::new(vec![b'm'; n]).unwrap())
        .into();
    msgs.push(CString::new("%s%n%p %d\n").unwrap());
    // Longer than the 64 KiB C keeps of a message. BSan's runtime grows
    // without bound over Rust views of C-written buffers that large, so it
    // gets a 3 KB one.
    let long = if under_bsan() { 3_000 } else { 70_000 };
    msgs.push(CString::new(vec![b'L'; long]).unwrap());
    FORMATTED.with(|f| *f.borrow_mut() = Some(Vec::new()));
    log::set_flags(Flags::PRINT_LEVEL);
    log::set_callback::<FormatAll>();
    for (i, m) in msgs.iter().enumerate() {
        log::log(
            Level::from_raw([c_int::MIN, -8, 16, 56, c_int::MAX][i % 5]),
            m,
        );
    }
    log::set_default_callback();
    let seen = FORMATTED.with(|f| f.borrow_mut().take().unwrap());
    assert_eq!(seen.len(), msgs.len());
    for f in &seen {
        let whole = f.whole.as_ref().unwrap().as_bytes();
        assert!(whole.len() <= 65_535 + 16);
        for (ret, buf, written) in &f.sized {
            assert_eq!(*ret, Some(whole.len()));
            if !buf.is_empty() {
                let n = buf.len() - 1;
                assert_eq!(&buf[..n.min(whole.len())], &whole[..n.min(whole.len())]);
                assert_eq!(written.as_slice(), &whole[..n.min(whole.len())]);
            } else {
                assert!(written.is_empty());
            }
        }
    }
}

thread_local! {
    /// How deep this thread is in nested logging calls.
    static DEPTH: Cell<u32> = const { Cell::new(0) };
    /// Whether [`Reenter`] and [`Other`] act on this thread's messages: other
    /// tests log concurrently, and their messages must neither count nor swap
    /// the callback mid-test.
    static REENTERING: Cell<bool> = const { Cell::new(false) };
}

/// Messages [`Reenter`] handled.
static REENTERED: AtomicUsize = AtomicUsize::new(0);

/// Logs again from inside the callback, twice nested, reads and writes the
/// settings, and swaps the callback for [`Other`] and back mid-call.
struct Reenter;

impl Callback for Reenter {
    fn log(record: Record<'_>) {
        if !REENTERING.with(Cell::get) {
            return;
        }
        REENTERED.fetch_add(1, Ordering::Relaxed);
        let depth = DEPTH.with(|d| d.replace(d.get() + 1));
        let mut p = true;
        let _ = record.to_line(&mut p);
        if depth < 2 {
            log::log(Level::ERROR, c"nested\n");
            let mut once = false;
            log::log_once(Level::ERROR, Level::DEBUG, &mut once, c"nested once\n");
            log::set_level(log::level());
            log::set_flags(log::flags());
            log::set_callback::<Other>();
            log::log(Level::ERROR, c"to other\n");
            log::set_callback::<Reenter>();
        }
        let _ = record.to_line(&mut p);
        DEPTH.with(|d| d.set(depth));
    }
}

/// Messages [`Other`] handled.
static OTHER: AtomicUsize = AtomicUsize::new(0);

/// Counts and formats, nothing more.
struct Other;

impl Callback for Other {
    fn log(record: Record<'_>) {
        if !REENTERING.with(Cell::get) {
            return;
        }
        OTHER.fetch_add(1, Ordering::Relaxed);
        let mut buf = [0u8; 8];
        let _ = record.write_line(&mut buf, &mut true);
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn callbacks_reenter_logging_and_swap_themselves() {
    if !isolated("log::callbacks_reenter_logging_and_swap_themselves") {
        return;
    }
    let _s = LogState::lock();
    let before = (
        REENTERED.load(Ordering::Relaxed),
        OTHER.load(Ordering::Relaxed),
    );
    REENTERING.with(|r| r.set(true));
    log::set_callback::<Reenter>();
    log::log(Level::WARNING, c"outer\n");
    log::set_default_callback();
    REENTERING.with(|r| r.set(false));
    // outer, then at depth 1 and 2: nested + nested once each spawning one
    // more level until depth 2.
    assert_eq!(REENTERED.load(Ordering::Relaxed) - before.0, 7);
    assert_eq!(OTHER.load(Ordering::Relaxed) - before.1, 3);
}

/// Counts messages for the concurrency test.
static COUNT_A: AtomicUsize = AtomicUsize::new(0);
static COUNT_B: AtomicUsize = AtomicUsize::new(0);

struct CountA;
impl Callback for CountA {
    fn log(record: Record<'_>) {
        COUNT_A.fetch_add(1, Ordering::Relaxed);
        let _ = record.to_line(&mut true);
    }
}

struct CountB;
impl Callback for CountB {
    fn log(record: Record<'_>) {
        COUNT_B.fetch_add(1, Ordering::Relaxed);
        let mut buf = [0u8; 32];
        let _ = record.format_line(&mut buf, &mut false);
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn logging_races_with_callback_level_and_flag_changes() {
    if !isolated("log::logging_races_with_callback_level_and_flag_changes") {
        return;
    }
    let _s = LogState::lock();
    log::set_callback::<CountA>();
    let stop = Arc::new(AtomicBool::new(false));
    let loggers: Vec<_> = (0..4)
        .map(|t| {
            thread::spawn(move || {
                let mut once = false;
                for i in 0..400 {
                    let msg = CString::new(format!("thread {t} message {i}\n")).unwrap();
                    log::log(Level::from_raw(i % 64 - 8), &msg);
                    log::log_once(Level::INFO, Level::DEBUG, &mut once, c"once\n");
                    let _ = (log::level(), log::flags());
                }
            })
        })
        .collect();
    let toggler = {
        let stop = stop.clone();
        thread::spawn(move || {
            let mut i = 0u32;
            while !stop.load(Ordering::Relaxed) {
                match i % 3 {
                    0 => log::set_callback::<CountB>(),
                    1 => log::clear_callback(),
                    _ => log::set_callback::<CountA>(),
                }
                log::set_level(Level::from_raw(i as c_int % 70 - 8));
                log::set_flags(Flags::from_bits_retain(i as c_int & 0xf));
                i = i.wrapping_add(1);
                thread::yield_now();
            }
        })
    };
    for l in loggers {
        l.join().unwrap();
    }
    stop.store(true, Ordering::Relaxed);
    toggler.join().unwrap();
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn log_once_takes_either_starting_state() {
    let _s = LogState::lock();
    log::clear_callback();
    for start in [false, true] {
        let mut state = start;
        for _ in 0..3 {
            log::log_once(
                Level::from_raw(c_int::MIN),
                Level::from_raw(c_int::MAX),
                &mut state,
                c"",
            );
            assert!(state);
        }
    }
}

/// Set in the child process an isolated test re-runs itself in.
const ISOLATED: &str = "CRUSTIFY_UB_ISOLATED";

/// Runs the test `name` alone in a child process, so that no other test
/// logs through the callback it installs; true in that child.
///
/// A callback is process-wide: other tests' messages go through it too,
/// which is sound (they are counted or dropped) but makes BSan report a
/// false use-after-free in `av_strtod` on a `cpu::parse_caps` thread
/// logging through it concurrently (the pointer libc's `strtod` hands back
/// carries stale provenance while the byte it reads is inside a live
/// block; formats wave-0 review, batch 1).
pub(crate) fn isolated(name: &str) -> bool {
    if std::env::var_os(ISOLATED).is_some() {
        return true;
    }
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--test-threads=1"])
        .env(ISOLATED, "1")
        .status()
        .unwrap();
    assert!(status.success(), "isolated run failed: {status}");
    false
}

/// Forwards every record to the default callback.
struct Forward;

impl Callback for Forward {
    fn log(record: Record<'_>) {
        record.log_default();
    }
}

/// The default callback, reached through `Record::log_default` from several
/// threads at once, at every level, tint and flag, in each colour mode C
/// picks from the environment. Its colour mode is fixed on first use and its
/// output is stderr, so each mode runs in a child process whose stderr is
/// discarded.
#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn default_callback_takes_every_level_flag_and_colour_mode() {
    const NAME: &str = "log::default_callback_takes_every_level_flag_and_colour_mode";
    if std::env::var_os(ISOLATED).is_none() {
        let modes: [&[(&str, &str)]; 5] = [
            &[("AV_LOG_FORCE_NOCOLOR", "1")],
            &[("AV_LOG_FORCE_COLOR", "1")],
            &[("AV_LOG_FORCE_COLOR", "1"), ("AV_LOG_FORCE_256COLOR", "1")],
            &[
                ("AV_LOG_FORCE_NOCOLOR", "1"),
                ("AV_LOG_FORCE_256COLOR", "1"),
            ],
            &[("TERM", "xterm-256color")],
        ];
        for env in modes {
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", NAME, "--test-threads=1"])
                .env(ISOLATED, "1")
                .envs(env.iter().copied())
                .stderr(std::process::Stdio::null())
                .status()
                .unwrap();
            assert!(status.success(), "{env:?}: {status}");
        }
        return;
    }
    log::set_callback::<Forward>();
    log::set_level(Level::TRACE);
    let levels = [
        c_int::MIN,
        -9,
        -8,
        -1,
        0,
        7,
        8,
        16,
        24,
        32,
        40,
        48,
        56,
        63,
        64,
        255,
        256,
    ];
    let tints = [0, 1 << 8, 134 << 8, 255 << 8];
    let flag_sets = [0, 1, 2, 3, 4, 8, 12, 15];
    let msgs: [&CStr; 6] = [
        c"same\n",
        c"same\n",
        c"partial ",
        c"cr\r",
        c"ctl \x01\x0e\x1f\n",
        c"",
    ];
    let threads: Vec<_> = (0..4)
        .map(|t| {
            thread::spawn(move || {
                for (i, &level) in levels.iter().enumerate() {
                    for (j, &tint) in tints.iter().enumerate() {
                        if t == 0 {
                            log::set_flags(Flags::from_bits_retain(
                                flag_sets[(i + j) % flag_sets.len()],
                            ));
                        }
                        let level = if level >= 0 { level | tint } else { level };
                        log::log(Level::from_raw(level), msgs[(i + j + t) % msgs.len()]);
                    }
                }
                let long = CString::new(vec![b'x'; 2000]).unwrap();
                log::log(Level::ERROR, &long);
                log::log(Level::ERROR, &long);
            })
        })
        .collect();
    for t in threads {
        t.join().unwrap();
    }
    log::set_level(Level::QUIET);
    log::log(Level::PANIC, c"");
}

/// `av_vlog` through the safe general log, with the default callback.
mod vlog {
    use core::ffi::c_int;
    use std::ffi::CString;
    use std::thread;

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::log::{Level, log};

    const DEBUG: c_int = ffi::AV_LOG_DEBUG as c_int;

    /// Levels the default callback drops, keeping the output quiet: it masks
    /// a non-negative level to its low byte, and drops it above the default
    /// `AV_LOG_INFO`.
    const DROPPED: [c_int; 7] = [
        DEBUG,
        ffi::AV_LOG_TRACE as c_int,
        DEBUG | 134 << 8,
        64,
        255,
        0x7fff_ff40,
        c_int::MAX,
    ];

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn any_message_is_accepted() {
        let long = CString::new(vec![b'%'; 4000]).unwrap();
        for level in DROPPED {
            for message in [c"", c"%s%n%p%d", c"no newline", long.as_c_str()] {
                log(Level::from_raw(level), message);
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn printed_lines_are_formatted_whole() {
        let long = CString::new(format!("crustify ub_safe vlog: {}\n", "%".repeat(1500))).unwrap();
        log(Level::INFO, c"crustify ub_safe vlog: 100% %s %n\n");
        log(Level::INFO, &long);
        log(Level::INFO, c"crustify ub_safe vlog: \x01\x7f partial");
        log(Level::INFO, c"\n");
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn logging_from_many_threads_at_once() {
        let workers: Vec<_> = (0..8)
            .map(|i| {
                thread::spawn(move || {
                    let message =
                        CString::new(format!("crustify ub_safe vlog thread {i}\n")).unwrap();
                    for j in 0..200 {
                        log(
                            if j == 0 {
                                Level::INFO
                            } else {
                                Level::from_raw(DEBUG)
                            },
                            &message,
                        );
                    }
                })
            })
            .collect();
        for w in workers {
            w.join().unwrap();
        }
    }
}

/// UB tests of `AVClass` through the safe API: a Rust class built by
/// `Class::new` and C's `cpuflags` class reached through `Record::class`,
/// every getter, the option table and child iteration, and handles shared
/// across threads.
mod class {
    use core::ffi::CStr;
    use std::cell::RefCell;
    use std::thread;

    use ffmpeg::libavutil::cpu;
    use ffmpeg::libavutil::log::{self, Callback, Class, ClassCategory, ClassRef, Record};

    use super::LogState;

    static OWN: Class = Class::new(c"ub-safe-class", ClassCategory::ENCODER);

    /// Every getter, collected; the callable handles stay uncalled (calling
    /// them is unsafe).
    fn read(c: ClassRef<'_>) -> (Option<Vec<u8>>, usize, usize, i32, String) {
        let _ = (
            c.item_name(),
            c.get_category(),
            c.query_ranges(),
            c.child_next(),
            c.log_level_offset_offset(),
            c.parent_log_context_offset(),
            c.state_flags_offset(),
            c.category(),
            format!("{c:?}"),
        );
        let names: Vec<Option<&CStr>> = c.options().iter().map(|o| o.name()).collect();
        assert!(names.iter().all(Option::is_some));
        let children = c
            .child_classes()
            .inspect(|child| assert!(child.name().is_some()))
            .count();
        (
            c.name().map(|n| n.to_bytes().to_vec()),
            names.len(),
            children,
            c.version(),
            format!("{:?}", c.category()),
        )
    }

    #[test]
    fn a_rust_class_reads_back() {
        let c = OWN.as_ref();
        let (name, options, children, version, category) = read(c);
        assert_eq!(name.as_deref(), Some(&b"ub-safe-class"[..]));
        assert_eq!((options, children), (0, 0));
        assert!(version > 0);
        assert_eq!(category, "ClassCategory::ENCODER");
        assert!(c.ptr_eq(OWN.as_ref()));
        // A local class, read and dropped.
        let local = Class::new(c"", ClassCategory::from_value(u32::MAX));
        assert_eq!(read(local.as_ref()).0.as_deref(), Some(&b""[..]));
        assert!(!local.as_ref().ptr_eq(c));
    }

    #[test]
    fn class_handles_cross_threads() {
        thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(|| {
                    let c: ClassRef<'static> = OWN.as_ref();
                    for _ in 0..100 {
                        let _ = read(c);
                    }
                });
            }
        });
    }

    /// A class's name, option count and child count.
    type Seen = (Option<Vec<u8>>, usize, usize);

    thread_local! {
        /// What `Grab` read from this thread's messages' classes, while set.
        static SEEN: RefCell<Option<Vec<Seen>>> = const { RefCell::new(None) };
    }

    /// Reads the class of every message logged on a capturing thread, and
    /// re-reads it from a scoped thread while the callback runs.
    struct Grab;

    impl Callback for Grab {
        fn log(record: Record<'_>) {
            if SEEN.with(|s| s.borrow().is_none()) {
                return;
            }
            let Some(class) = record.class() else {
                return;
            };
            let (name, options, children, _, _) = read(class);
            let again = thread::scope(|s| s.spawn(move || read(class)).join().unwrap());
            assert_eq!(
                (name.clone(), options, children),
                (again.0, again.1, again.2)
            );
            SEEN.with(|s| {
                s.borrow_mut()
                    .as_mut()
                    .unwrap()
                    .push((name, options, children))
            });
        }
    }

    /// `av_parse_cpu_caps` evaluates flags against its own static class,
    /// `cpuflags`, and logs an unknown token in that class's context.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn the_cpuflags_class_reaches_a_callback() {
        if !super::isolated("log::class::the_cpuflags_class_reaches_a_callback") {
            return;
        }
        let _s = LogState::lock();
        SEEN.with(|s| *s.borrow_mut() = Some(Vec::new()));
        log::set_callback::<Grab>();
        let mut flags = 0;
        assert!(cpu::parse_cpu_caps(&mut flags, c"no-such-flag").is_err());
        log::set_default_callback();
        let seen = SEEN.with(|s| s.borrow_mut().take()).unwrap();
        // The expression evaluator logs in its own context too.
        let cpuflags: Vec<_> = seen
            .into_iter()
            .filter(|(name, ..)| name.as_deref() == Some(&b"cpuflags"[..]))
            .collect();
        assert!(!cpuflags.is_empty());
        for (_, options, children) in cpuflags {
            assert!(options > 1, "the flags option and its constants");
            assert_eq!(children, 0);
        }
    }
}

/// A class need not be static. Its shared and exclusive handles retain the
/// local descriptor's lifetime while moving between scoped threads.
#[test]
fn local_class_handles_can_be_reborrowed_on_scoped_threads() {
    use ffmpeg::libavutil::log::{Class, ClassCategory};

    let mut class = Class::new(c"local-class", ClassCategory::FILTER);
    std::thread::scope(|scope| {
        let r = class.as_ref();
        let copy = r;
        scope.spawn(move || assert_eq!(r.name(), Some(c"local-class")));
        scope.spawn(move || assert_eq!(copy.category(), ClassCategory::FILTER));
    });
    let mut m = class.as_mut();
    std::thread::scope(|scope| {
        let mut reborrow = m.as_mut();
        scope.spawn(move || {
            assert_eq!(reborrow.as_mut().as_ref().name(), Some(c"local-class"));
            assert!(reborrow.as_ref().options().is_empty());
        });
    });
    assert_eq!(m.as_ref().category(), ClassCategory::FILTER);
}
