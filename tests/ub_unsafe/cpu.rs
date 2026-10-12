//! UB tests for `libavutil/cpu.h`'s unsafe `force_cpu_flags`, each call within
//! its `# Safety` contract: every forced set is a subset of what the running
//! CPU reported (or `-1`), and nothing in this binary picks code paths or
//! buffer alignment from the flags.

use core::ffi::c_int;
use std::sync::Mutex;
use std::thread;

use ffmpeg::libavutil::cpu::{cpu_flags, force_cpu_flags};

/// Serialises the tests that force the process-wide set.
static FORCED: Mutex<()> = Mutex::new(());

/// The flags the running CPU supports, detected afresh.
fn detected() -> c_int {
    // SAFETY: `-1` names no extension and restores detection.
    unsafe { force_cpu_flags(-1) };
    cpu_flags()
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn every_subset_of_the_detected_cpu_flags() {
    let _g = FORCED.lock().unwrap();
    let d = detected();
    for bit in 0..31 {
        for set in [d & (1 << bit), d & !(1 << bit), 0, d] {
            // SAFETY: `set` names only extensions the CPU reported.
            unsafe { force_cpu_flags(set) };
            assert_eq!(cpu_flags() & !1, set & !1, "{set:#x}");
        }
    }
    // SAFETY: `-1` restores detection.
    unsafe { force_cpu_flags(-1) };
    assert_eq!(cpu_flags(), d);
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn concurrent_forcing_and_reading() {
    let _g = FORCED.lock().unwrap();
    let d = detected();
    let sets = [0, d, d & 0x5555_5555, d & 0x2aaa_aaaa, -1];
    let handles: Vec<_> = (0..8)
        .map(|t| {
            thread::spawn(move || {
                for i in 0..500 {
                    if (i + t) % 3 == 0 {
                        // SAFETY: each set is a subset of `d`, or `-1`.
                        unsafe { force_cpu_flags(sets[(i + t) % sets.len()]) };
                    } else {
                        let f = cpu_flags();
                        assert_eq!(f & !(d | 1), 0, "{f:#x} is no subset of {d:#x}");
                    }
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    // SAFETY: `-1` restores detection.
    unsafe { force_cpu_flags(-1) };
    assert_eq!(cpu_flags(), d);
}
