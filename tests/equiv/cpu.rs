//! Equivalence of `libavutil/cpu.h`'s safe CPU queries, and of its unsafe
//! `force_cpu_flags`, with the raw C functions, across the process-global
//! overrides each side sets.

use core::ffi::c_int;
use std::process::Command;

use ffmpeg::libavutil::cpu::{
    cpu_count, cpu_flags, cpu_max_align, force_cpu_count, force_cpu_flags,
};
use ffmpeg::libavutil::ffi;

/// Set in the child process an isolated test re-runs itself in.
const ISOLATED: &str = "CRUSTIFY_EQUIV_ISOLATED";

/// Whether this is the isolated child of test `name`; in the parent, runs the
/// child, asserts it passed, and returns `false`.
fn isolated(name: &str) -> bool {
    if std::env::var_os(ISOLATED).is_some() {
        return true;
    }
    let status = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--test-threads=1", "--nocapture"])
        .env(ISOLATED, "1")
        .status()
        .unwrap();
    assert!(status.success(), "isolated run failed: {status}");
    false
}

/// `av_cpu_count` in C.
fn c_count() -> i32 {
    // SAFETY: `av_cpu_count` takes no arguments and reads C atomics.
    unsafe { ffi::av_cpu_count() }
}

/// `av_cpu_max_align` in C.
fn c_align() -> usize {
    // SAFETY: `av_cpu_max_align` takes no arguments and reads C atomics.
    unsafe { ffi::av_cpu_max_align() }
}

/// The CPU count override is process-global, so this test sets it only in a
/// child process running nothing else.
#[test]
fn forced_counts_set_by_either_side_read_the_same() {
    if !isolated("cpu::forced_counts_set_by_either_side_read_the_same") {
        return;
    }
    let detected = c_count();
    assert_eq!(cpu_count(), detected);
    for count in [1, 2, 3, 64, i32::MAX, 0, -1, i32::MIN, 5] {
        // Rust sets, both read.
        force_cpu_count(count);
        let expect = if count > 0 { count } else { detected };
        assert_eq!((cpu_count(), c_count()), (expect, expect), "{count}");
        // C sets, both read.
        // SAFETY: `av_cpu_force_count` stores an integer in a C atomic.
        unsafe { ffi::av_cpu_force_count(count) };
        assert_eq!((cpu_count(), c_count()), (expect, expect), "{count}");
    }
    force_cpu_count(0);
    assert_eq!(cpu_count(), detected);
}

/// The CPU flags mask is process-global, so this test forces it only in a
/// child process running nothing else.
#[test]
fn max_align_follows_the_forced_cpu_flags() {
    if !isolated("cpu::max_align_follows_the_forced_cpu_flags") {
        return;
    }
    let detected = c_align();
    assert_eq!(cpu_max_align(), detected);
    // SAFETY: `av_get_cpu_flags` takes no arguments and reads C atomics.
    let flags = unsafe { ffi::av_get_cpu_flags() };
    // The detected flags below each bit, from none to all of them, so every
    // alignment tier is crossed with the lower flags it builds on.
    let mut masks: Vec<i32> = (0..=32).map(|b| flags & ((1i64 << b) - 1) as i32).collect();
    masks.dedup();
    for mask in masks {
        // SAFETY: `av_force_cpu_flags` stores the mask in a C atomic; nothing
        // else runs in this process.
        unsafe { ffi::av_force_cpu_flags(mask) };
        let a = cpu_max_align();
        assert_eq!(a, c_align(), "flags {mask:#x}");
        if mask == 0 {
            assert_eq!(a, 8);
        }
    }
    // SAFETY: as above; -1 restores detection.
    unsafe { ffi::av_force_cpu_flags(-1) };
    assert_eq!((cpu_max_align(), c_align()), (detected, detected));
}

/// Both sides' reading of the flag set.
fn read_both() -> (c_int, c_int) {
    // SAFETY: `av_get_cpu_flags` takes no arguments and reads C atomics.
    (cpu_flags(), unsafe { ffi::av_get_cpu_flags() })
}

/// The flags the running CPU supports, detected afresh.
fn detected_flags() -> c_int {
    // SAFETY: `-1` names no extension and restores detection; nothing else
    // runs in the isolated child that calls this.
    unsafe { force_cpu_flags(-1) };
    cpu_flags()
}

/// The CPU flags are process-global, so the flag tests force them only in a
/// child process running nothing else.
#[test]
fn detected_flags_read_alike() {
    if !isolated("cpu::detected_flags_read_alike") {
        return;
    }
    let d = detected_flags();
    assert_eq!(read_both(), (d, d));
    assert_ne!(d, -1);
}

#[test]
fn forced_flag_subsets_read_alike_after_every_step() {
    if !isolated("cpu::forced_flag_subsets_read_alike_after_every_step") {
        return;
    }
    let d = detected_flags();
    let mmx = ffi::AV_CPU_FLAG_MMX as c_int;
    let masks = [0, 1, 3, 0xff, 0x7fff, !0x10, !0, 0x5555_5555, 0x2aaa_aaaa];
    for (step, mask) in masks.into_iter().enumerate() {
        let want = d & mask;
        // SAFETY: `want` names only extensions the running CPU reported, and
        // nothing else runs in this process.
        unsafe { force_cpu_flags(want) };
        let (r, c) = read_both();
        assert_eq!(r, c, "step {step}");
        // C adds MMX to a set with a later x86 extension and no MMX.
        assert_eq!(r & !mmx, want & !mmx, "step {step}");

        // The same set forced through C reads back alike.
        // SAFETY: as above.
        unsafe { ffi::av_force_cpu_flags(want) };
        assert_eq!(read_both(), (r, r), "step {step}: C side");
    }
    // SAFETY: `-1` restores detection.
    unsafe { force_cpu_flags(-1) };
    assert_eq!(read_both(), (d, d));
}

#[test]
fn mmx_is_implied_alike() {
    if !isolated("cpu::mmx_is_implied_alike") {
        return;
    }
    let d = detected_flags();
    let (mmx, sse) = (ffi::AV_CPU_FLAG_MMX as c_int, ffi::AV_CPU_FLAG_SSE as c_int);
    if cfg!(target_arch = "x86_64") && d & (mmx | sse) == mmx | sse {
        // SAFETY: SSE is supported (and MMX, which C adds back); nothing else
        // runs in this process.
        unsafe { force_cpu_flags(sse) };
        assert_eq!(cpu_flags(), sse | mmx);
        // SAFETY: as above.
        unsafe { ffi::av_force_cpu_flags(sse) };
        assert_eq!(cpu_flags(), sse | mmx);
    }
    // SAFETY: `-1` restores detection.
    unsafe { force_cpu_flags(-1) };
}

// ---- av_parse_cpu_caps ----

/// Equivalence of `libavutil/cpu.h`'s [`parse_cpu_caps`] with
/// `av_parse_cpu_caps`, on independent flag words, for the strings the
/// wrapper passes on.
mod parse_caps {
    use core::ffi::CStr;

    use ffmpeg::libavutil::cpu::parse_cpu_caps;
    use ffmpeg::libavutil::ffi;

    /// `av_parse_cpu_caps` in C, on its own flag word.
    fn c_parse(flags: &mut u32, s: &CStr) -> Result<(), i32> {
        // SAFETY: `flags` is a live, writable word and `s` a live NUL-terminated
        // string, both only used for the call. Every string the tests pass has
        // tokens of letters, digits, `_` and `.` whose `+`/`-` operands lie
        // below 2^63 in magnitude, which C converts to `int64_t` without
        // overflow.
        let ret = unsafe { ffi::av_parse_cpu_caps(flags, s.as_ptr()) };
        if ret < 0 { Err(ret) } else { Ok(()) }
    }

    /// Flags strings across features, models, keywords, numbers, operators and
    /// failures, x86 names first.
    const STRINGS: [&CStr; 24] = [
        c"sse2",
        c"+avx",
        c"-mmx",
        c"all",
        c"none",
        c"+sse4.1-sse2",
        c"k8",
        c"pentium4+avx2",
        c"0x7",
        c"+0x10",
        c"bogus",
        c"+sse2+bogus",
        c"-all",
        c"3dnow",
        c"mmx+",
        c"",
        c"1",
        c"-1",
        c"+3dnowext-cmov",
        c"default",
        c"+min",
        c"1e3",
        c"4294967295",
        c"4294967296",
    ];

    #[test]
    fn single_strings_match_c_from_several_starting_words() {
        for start in [0, 1, 0x10, 0xFFFF, u32::MAX] {
            for s in STRINGS {
                let (mut c, mut r) = (start, start);
                let cret = c_parse(&mut c, s);
                let rret = parse_cpu_caps(&mut r, s);
                assert_eq!((rret, r), (cret, c), "{s:?} from {start:#x}");
            }
        }
    }

    #[test]
    fn sequences_match_c_after_each_step() {
        for rotate in 0..STRINGS.len() {
            let (mut c, mut r) = (0x5a5a, 0x5a5a);
            for s in STRINGS.iter().cycle().skip(rotate).take(STRINGS.len() * 2) {
                let cret = c_parse(&mut c, s);
                let rret = parse_cpu_caps(&mut r, s);
                assert_eq!((rret, r), (cret, c), "{s:?} at rotation {rotate}");
            }
        }
    }

    /// `+`/`-` operands that are not 32-bit flag sets themselves: C truncates
    /// each to an `int64_t` and range-checks only the combination, so a
    /// fractional, wide, or named operand may clear bits, set some, or fail.
    /// Regression: the wrapper once evaluated such operands through C's
    /// unprefixed path, which rounds to the nearest integer, so `-0.999`
    /// cleared bit 0 and `+1.9995` set bit 1 where C leaves bit 0 alone.
    const OPERANDS: [&CStr; 30] = [
        c"-0.999",
        c"+1.9995",
        c"-1.9995",
        c"+2.5",
        c"+0.5",
        c"-0.5",
        c"+0.001",
        c"+4294967294.999",
        c"+5d",
        c"-3h",
        c"+1dB",
        c"-10dB",
        c"+0.25k",
        c"-5G",
        c"-0x100000000",
        c"+1e18",
        c"-1e18",
        c"-9223372036854774784",
        c"+PI",
        c"-E",
        c"+PHI",
        c"+QP2LAMBDA",
        c"-min",
        c"+min",
        c"+3dnow",
        c"-3dnowext",
        c"+all",
        c"-none",
        c"+nan",
        c"sse2+2.5-0.999+PI",
    ];

    #[test]
    fn fractional_wide_and_named_operands_match_c() {
        for start in [0, 1, 0x55, 0xFFFF, u32::MAX] {
            for s in OPERANDS {
                let (mut c, mut r) = (start, start);
                let cret = c_parse(&mut c, s);
                let rret = parse_cpu_caps(&mut r, s);
                assert_eq!((rret, r), (cret, c), "{s:?} from {start:#x}");
            }
        }
        // Each outcome occurs: a changed word, an unchanged one, and a failure.
        let outcomes: Vec<(bool, bool)> = OPERANDS
            .iter()
            .map(|s| {
                let mut c = 0x55;
                let ok = c_parse(&mut c, s).is_ok();
                (ok, c != 0x55)
            })
            .collect();
        assert!(outcomes.contains(&(true, true)));
        assert!(outcomes.contains(&(true, false)));
        assert!(outcomes.contains(&(false, false)));
    }

    #[test]
    fn operand_sequences_match_c_after_each_step() {
        for rotate in 0..OPERANDS.len() {
            let (mut c, mut r) = (0xA5A5, 0xA5A5);
            for s in OPERANDS.iter().cycle().skip(rotate).take(OPERANDS.len() + 3) {
                let cret = c_parse(&mut c, s);
                let rret = parse_cpu_caps(&mut r, s);
                assert_eq!((rret, r), (cret, c), "{s:?} at rotation {rotate}");
            }
        }
    }

    #[test]
    fn long_tokens_split_as_in_c() {
        for len in [254, 255, 256, 300, 511, 600] {
            for fill in ["0", "9"] {
                let text = format!("{}3\0", fill.repeat(len));
                let s = CStr::from_bytes_with_nul(text.as_bytes()).unwrap();
                let (mut c, mut r) = (1, 1);
                assert_eq!(
                    (parse_cpu_caps(&mut r, s), r),
                    (c_parse(&mut c, s), c),
                    "{len} x {fill}"
                );
            }
        }
    }
}
