//! UB tests of `libavutil/cpu.h`'s safe CPU queries: the process-global
//! override written and read from many threads at once, and the flag set read
//! concurrently while the first reads may race to detect the CPU.

use std::sync::Barrier;

use ffmpeg_bindings::libavutil::libavutil::cpu::{cpu_count, cpu_flags, cpu_max_align, force_cpu_count};

/// The only test touching the process-global override, so the sequential
/// checks see no other writer.
#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn override_is_shared_soundly_across_threads() {
    let detected = cpu_count();
    assert!(detected >= 1);
    let align = cpu_max_align();

    // Writers and readers race on the C atomics; every read is a value some
    // writer stored, or detection.
    let barrier = Barrier::new(8);
    std::thread::scope(|s| {
        for t in 0..8i32 {
            let barrier = &barrier;
            s.spawn(move || {
                barrier.wait();
                for i in 0..2_000 {
                    if t % 2 == 0 {
                        force_cpu_count(if i % 3 == 0 { 0 } else { t + 1 });
                    } else {
                        let n = cpu_count();
                        assert!(n == detected || (1..=8).contains(&n), "{n}");
                        assert_eq!(cpu_max_align(), align);
                    }
                }
            });
        }
    });

    for count in [1, i32::MAX, 0, -5, i32::MIN] {
        force_cpu_count(count);
        assert_eq!(cpu_count(), if count > 0 { count } else { detected });
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn cpu_flags_read_concurrently_agree() {
    let reads: Vec<_> = (0..8)
        .map(|_| std::thread::spawn(|| (0..1000).map(|_| cpu_flags()).collect::<Vec<_>>()))
        .collect();
    let all: Vec<i32> = reads.into_iter().flat_map(|h| h.join().unwrap()).collect();
    assert!(all.iter().all(|&f| f == all[0] && f != -1));
}

// ---- av_parse_cpu_caps ----

/// UB tests of `libavutil/cpu.h`'s [`parse_cpu_caps`]: strings that drive C's
/// option evaluator into undefined behaviour (out-of-range float-to-integer
/// conversions) are refused or evaluated without it, and calls run
/// concurrently.
mod parse_caps {
    use core::ffi::CStr;

    use ffmpeg_bindings::libavutil::libavutil::cpu::parse_cpu_caps;

    /// Strings reaching UB in C: expression functions converting a huge double
    /// to `int` (`ld`, `st`, `bitand`), and `+`/`-` operands C converts to
    /// `int64_t` out of range (`max`, `1e300`, `inf`), alone and after valid
    /// tokens.
    const HOSTILE: [&CStr; 16] = [
        c"ld(1e300)",
        c"st(1e300,1)",
        c"bitand(1e300,1)",
        c"+1e300",
        c"-1e300",
        c"+max",
        c"-max",
        c"+inf",
        c"-INF",
        c"+nan",
        c"sse2+1e300",
        c"sse2-max+mmx",
        c"+1Y",
        c"++1e300",
        c"+-1e300",
        c"+(1e300)",
    ];

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn hostile_strings_fail_without_ub() {
        for s in HOSTILE {
            let mut flags = 0x10;
            assert!(parse_cpu_caps(&mut flags, s).is_err(), "{s:?}");
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn token_edges_and_lengths_are_sound() {
        for len in [0, 1, 254, 255, 256, 1000] {
            for fill in ["9", "a", ".", "+", "-", "+a", "1-"] {
                let text = format!("{}\0", fill.repeat(len));
                let s = CStr::from_bytes_with_nul(text.as_bytes()).unwrap();
                let mut flags = 0;
                let _ = parse_cpu_caps(&mut flags, s);
            }
        }
        let mut flags = 0;
        for s in [
            c"all",
            c"none",
            c"-1",
            c"4294967295",
            c"default",
            c"+sse2",
            c"-sse2",
        ] {
            parse_cpu_caps(&mut flags, s).unwrap();
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn concurrent_parses_do_not_race() {
        std::thread::scope(|scope| {
            for hostile in HOSTILE.into_iter().take(8) {
                scope.spawn(move || {
                    let mut flags = 0;
                    for _ in 0..50 {
                        parse_cpu_caps(&mut flags, c"sse2+avx-mmx").unwrap();
                        assert!(parse_cpu_caps(&mut flags, hostile).is_err());
                    }
                });
            }
        });
    }
}
