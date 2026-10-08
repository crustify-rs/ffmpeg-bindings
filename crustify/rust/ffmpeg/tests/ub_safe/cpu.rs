//! UB tests of `libavutil/cpu.h`'s safe CPU queries: the process-global
//! override written and read from many threads at once.

use std::sync::Barrier;

use ffmpeg::libavutil::cpu::{cpu_count, cpu_max_align, force_cpu_count};

/// The only test touching the process-global override, so the sequential
/// checks see no other writer.
#[test]
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
