use ffmpeg_bindings::libavutil::libavutil::tea::context_size;

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
use ffmpeg_bindings::libavutil::libavutil::tea::Tea;

#[test]
#[cfg_attr(miri, ignore = "calls C")]
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

#[test]
#[cfg_attr(miri, ignore = "calls C")]
pub(super) fn batch2_foreign_writes_across_reborrows() {
    let mut owner = Tea::new(&[7; 16], 64).unwrap();
    for cbc in [false, true] {
        for length in [0, 8, 16, 1032] {
            let mut data = vec![0x79; length + 1];
            let original = data.clone();
            let mut iv = [7; 8];
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
    let owner = Tea::new(&[9; 16], 64).unwrap();
    let shared = owner.as_ref();
    let copied = shared;
    let mut first = [0; 24];
    let mut second = first;
    shared.crypt(&mut first, &[1; 24], None, Encrypt).unwrap();
    copied.crypt(&mut second, &[1; 24], None, Encrypt).unwrap();
    assert_eq!(first, second);
}
