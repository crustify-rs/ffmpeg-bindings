//! Safe lifecycle exercises: no raw or unsafe FFI fixtures.
use ffmpeg::libavutil::sha512::{Algorithm, Sha512, Sha512Box};

const ALGORITHMS: [Algorithm; 4] = [
    Algorithm::Sha512224,
    Algorithm::Sha512256,
    Algorithm::Sha384,
    Algorithm::Sha512,
];

#[test]
fn owners_and_reborrows() {
    for algorithm in ALGORITHMS {
        for _ in 0..32 {
            let mut owner = Sha512::new(algorithm).unwrap();
            let shared = owner.as_ref();
            let copied = shared;
            assert_eq!(format!("{shared:?}"), format!("{copied:?}"));
            let mut exclusive = owner.as_mut();
            assert!(!format!("{exclusive:?}").is_empty());
            for next in ALGORITHMS {
                exclusive.as_mut().init(next);
                let shared = exclusive.as_ref();
                assert!(!format!("{shared:?}").is_empty());
            }
        }
    }
    let mut owner = Sha512Box::new().unwrap();
    owner.as_mut().init(Algorithm::Sha512);
}

#[test]
fn allocation_failure() {
    // The allocator limit is global. Isolate the failure in a child process,
    // so other tests' live contexts and allocations are unaffected.
    const CHILD: &str = "CRUSTIFY_BATCH7_SHA512_OOM";
    if std::env::var_os(CHILD).is_some() {
        ffmpeg::libavutil::mem::set_max_alloc(0);
        assert!(Sha512::new(Algorithm::Sha512).is_none());
        assert!(Sha512Box::new().is_none());
    } else {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .env(CHILD, "1")
            .args(["--exact", "sha512::allocation_failure"])
            .status()
            .unwrap();
        assert!(status.success());
    }
}

use ffmpeg::libavutil::sha512::context_size;

#[test]
fn batch10_immutable_size_across_threads() {
    let size: &'static i32 = context_size();
    assert!(*size > 0);
    let workers: Vec<_> = (0..4)
        .map(|_| {
            std::thread::spawn(move || {
                for _ in 0..100 {
                    assert!(std::ptr::eq(size, context_size()));
                }
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
}
