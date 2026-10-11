use ffmpeg::libavutil::twofish::context_size;

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

use ffmpeg::libavutil::twofish::{InitError, InitStatus, Twofish};

#[test]
fn constructors_and_exclusive_reinitialization() {
    let mut owner = {
        let key = [42; 32];
        Twofish::new(&key, 256).unwrap().0
    };
    for bits in [
        0,
        1,
        7,
        8,
        127,
        128,
        129,
        191,
        192,
        193,
        255,
        256,
        257,
        i32::MAX,
    ] {
        let length = (bits.min(256) / 8) as usize;
        let mut key = vec![123; length];
        let (constructed, status) = Twofish::new(&key, bits).unwrap();
        assert_eq!(
            status,
            if [128, 192, 256].contains(&bits) {
                InitStatus::Exact
            } else {
                InitStatus::Adjusted
            }
        );
        let shared = constructed.as_ref();
        let copy = shared;
        let _ = (shared, copy);
        drop(constructed);
        let mut exclusive = owner.as_mut();
        assert_eq!(exclusive.as_mut().init(&key, bits).unwrap(), status);
        let _ = exclusive.as_ref();
        key.fill(255);
        assert_eq!(exclusive.init(&key, bits).unwrap(), status);
    }
    assert!(matches!(
        Twofish::new(&[], -1),
        Err(InitError::NegativeKeyBits)
    ));
    assert!(matches!(
        Twofish::new(&[0; 31], 256),
        Err(InitError::KeyTooShort {
            required: 32,
            actual: 31
        })
    ));
    assert_eq!(
        owner.as_mut().init(&[], i32::MIN),
        Err(InitError::NegativeKeyBits)
    );
    assert_eq!(
        owner.as_mut().init(&[], 8),
        Err(InitError::KeyTooShort {
            required: 1,
            actual: 0
        })
    );
}
