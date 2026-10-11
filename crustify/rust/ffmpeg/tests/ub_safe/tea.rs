use ffmpeg::libavutil::tea::context_size;

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

use ffmpeg::libavutil::tea::Tea;

#[test]
fn constructors_and_exclusive_reinitialization() {
    for rounds in [i32::MIN, -1, 0, 1, 16, 63, 64, i32::MAX] {
        let mut owner = Tea::new(&[0; 16], rounds).unwrap();
        let shared = owner.as_ref();
        let copy = shared;
        let _ = (shared, copy);
        let mut exclusive = owner.as_mut();
        exclusive.as_mut().init(&[255; 16], rounds);
        let _ = exclusive.as_ref();
        exclusive.init(&[1; 16], 64);
    }
    // Keys are copied, never retained across Rust reborrows or their drop.
    let mut owner = {
        let key = [42; 16];
        Tea::new(&key, 64).unwrap()
    };
    for _ in 0..100 {
        let mut key = [0; 16];
        owner.as_mut().init(&key, 16);
        key.fill(255);
        owner.as_mut().init(&key, 64);
    }
    drop(owner);
}
