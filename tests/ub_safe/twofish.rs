#![expect(
    clippy::cast_sign_loss,
    reason = "test inputs are chosen within range, and the casts mirror the C conversions under comparison"
)]
use ffmpeg_bindings::libavutil::libavutil::twofish::context_size;

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

use ffmpeg_bindings::libavutil::libavutil::aes::Direction::{Decrypt, Encrypt};
use ffmpeg_bindings::libavutil::libavutil::twofish::{InitError, InitStatus, Twofish};

#[test]
#[cfg_attr(miri, ignore = "calls C")]
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

#[test]
#[cfg_attr(miri, ignore = "calls C")]
pub(super) fn batch2_foreign_writes_across_reborrows() {
    let mut owner = Twofish::new(&[7; 16], 128).unwrap().0;
    for cbc in [false, true] {
        for length in [0, 16, 32, 2064] {
            let mut data = vec![0x79; length + 1];
            let original = data.clone();
            let mut iv = [7; 16];
            let initial_iv = iv;
            // Unaligned byte buffers are legal; C uses unaligned byte-order loads.
            let mut parent = owner.as_mut();
            parent
                .as_mut()
                .as_ref()
                .crypt_in_place(
                    &mut data[1..],
                    if cbc { Some(&mut iv) } else { None },
                    Encrypt,
                )
                .unwrap();
            iv = initial_iv;
            parent
                .as_mut()
                .as_ref()
                .crypt_in_place(
                    &mut data[1..],
                    if cbc { Some(&mut iv) } else { None },
                    Decrypt,
                )
                .unwrap();
            assert_eq!(data, original);
            let mut output = vec![0; length + 1];
            parent
                .as_mut()
                .as_ref()
                .crypt(
                    &mut output[1..],
                    &data[1..],
                    if cbc { Some(&mut iv) } else { None },
                    Encrypt,
                )
                .unwrap();
            data.fill(0); // C must not retain the input
            parent
                .as_mut()
                .as_ref()
                .crypt_in_place(
                    &mut output[1..],
                    if cbc { Some(&mut iv) } else { None },
                    Decrypt,
                )
                .unwrap();
        }
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn batch2_copied_shared_key_schedule() {
    let owner = Twofish::new(&[9; 16], 128).unwrap().0;
    let shared = owner.as_ref();
    let copied = shared;
    let mut first = [0; 48];
    let mut second = first;
    shared.crypt(&mut first, &[1; 48], None, Encrypt).unwrap();
    copied.crypt(&mut second, &[1; 48], None, Encrypt).unwrap();
    assert_eq!(first, second);
}
