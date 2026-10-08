//! Equivalence of `libavutil/cpu.h`'s safe CPU queries with the raw C
//! functions, across the process-global overrides each side sets.

use std::process::Command;

use ffmpeg::libavutil::cpu::{cpu_count, cpu_max_align, force_cpu_count};
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
