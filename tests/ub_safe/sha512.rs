//! Safe lifecycle exercises: no raw or unsafe FFI fixtures.
use ffmpeg_bindings::libavutil::libavutil::sha512::{Algorithm, Sha512, Sha512Box};

const ALGORITHMS: [Algorithm; 4] = [
    Algorithm::Sha512224,
    Algorithm::Sha512256,
    Algorithm::Sha384,
    Algorithm::Sha512,
];

#[test]
#[cfg_attr(miri, ignore = "calls C")]
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
#[cfg_attr(miri, ignore = "calls C")]
fn allocation_failure() {
    // The allocator limit is global. Isolate the failure in a child process,
    // so other tests' live contexts and allocations are unaffected.
    const CHILD: &str = "CRUSTIFY_BATCH7_SHA512_OOM";
    if std::env::var_os(CHILD).is_some() {
        ffmpeg_bindings::libavutil::libavutil::mem::set_max_alloc(0);
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

use ffmpeg_bindings::libavutil::libavutil::sha512::context_size;

#[test]
#[cfg_attr(miri, ignore = "reads a C global")]
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

#[test]
#[cfg_attr(miri, ignore = "calls C")]
pub(super) fn batch2_foreign_writes_across_reborrows() {
    let data: Vec<u8> = (0..1025).map(|i| i as u8).collect();
    for algorithm in ALGORITHMS {
        let mut owner = Sha512::new(algorithm).unwrap();
        for length in [0, 1, 55, 56, 63, 64, 65, 111, 112, 127, 128, 129, 1025] {
            let mut parent = owner.as_mut();
            parent.init(algorithm);
            for chunk in data[..length].chunks(7) {
                let mut input = chunk.to_vec();
                parent.as_mut().update(&input);
                input.fill(0); // input is not retained across the next reborrow
            }
            parent.as_mut().update(&[]);
            let digest = parent.as_mut().finalize();
            assert!(digest[algorithm.digest_len()..].iter().all(|&b| b == 0));
            // Repeated final/update remains memory-safe, though not a new message.
            parent.as_mut().finalize();
            parent.as_mut().update(b"after final");
        }
    }
}
