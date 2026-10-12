//! Safe lifecycle exercises: no raw or unsafe FFI fixtures.
use ffmpeg::libavutil::sha::{Algorithm, Sha, ShaBox};

const ALGORITHMS: [Algorithm; 3] = [Algorithm::Sha1, Algorithm::Sha224, Algorithm::Sha256];

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn owners_and_reborrows() {
    for algorithm in ALGORITHMS {
        for _ in 0..32 {
            let mut owner = Sha::new(algorithm).unwrap();
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
    let mut owner = ShaBox::new().unwrap();
    owner.as_mut().init(Algorithm::Sha256);
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn allocation_failure() {
    // The allocator limit is global. Isolate the failure in a child process,
    // so other tests' live contexts and allocations are unaffected.
    const CHILD: &str = "CRUSTIFY_BATCH7_SHA_OOM";
    if std::env::var_os(CHILD).is_some() {
        ffmpeg::libavutil::mem::set_max_alloc(0);
        assert!(Sha::new(Algorithm::Sha256).is_none());
        assert!(ShaBox::new().is_none());
    } else {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .env(CHILD, "1")
            .args(["--exact", "sha::allocation_failure"])
            .status()
            .unwrap();
        assert!(status.success());
    }
}

use ffmpeg::libavutil::sha::context_size;

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
        let mut owner = Sha::new(algorithm).unwrap();
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
