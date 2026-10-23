use ffmpeg_bindings::libavutil::libavutil::aes::{Aes, AesRef, Direction, InitError};

fn shared_views(view: AesRef<'_>) {
    let _copy = view;
    let _second = view;
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn lifecycle_reborrows() {
    for direction in [Direction::Encrypt, Direction::Decrypt] {
        for len in [16, 24, 32] {
            let mut owner = {
                let key = vec![0x55; len];
                Aes::new(&key, direction).unwrap()
            }; // C must not retain this key.
            shared_views(owner.as_ref());
            let mut handle = owner.as_mut();
            shared_views(handle.as_ref());
            for len in [32, 16, 24] {
                let mut reborrow = handle.as_mut();
                reborrow.init(&vec![0xaa; len], direction).unwrap();
                shared_views(reborrow.as_ref());
            }
            handle.init(&[7; 16], Direction::Encrypt).unwrap();
            shared_views(handle.as_ref());
        }
    } // owners free initialized C storage exactly once.
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn invalid_keys_do_not_invalidate_owner() {
    let mut owner = Aes::new(&[0; 16], Direction::Encrypt).unwrap();
    for len in 0..=64 {
        if matches!(len, 16 | 24 | 32) {
            continue;
        }
        assert_eq!(
            owner.as_mut().init(&vec![0; len], Direction::Decrypt),
            Err(InitError::InvalidKeyLength)
        );
        shared_views(owner.as_ref());
    }
    owner.as_mut().init(&[3; 32], Direction::Decrypt).unwrap();
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn parallel_constructors() {
    // Owners never cross threads. Independent safe construction does share C's
    // static AES lookup tables, whose initialization must use ff_thread_once.
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
    let threads: Vec<_> = (0..8)
        .map(|seed| {
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                let mut owner = Aes::new(&[seed; 24], Direction::Encrypt).unwrap();
                owner
                    .as_mut()
                    .init(&[seed; 32], Direction::Decrypt)
                    .unwrap();
                let mut ctr = ffmpeg_bindings::libavutil::libavutil::aes_ctr::AesCtr::new(&[seed; 16]).unwrap();
                ctr.as_mut().init(&[seed + 1; 16]).unwrap();
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
}

use ffmpeg_bindings::libavutil::libavutil::aes::context_size;

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
fn processing_across_reborrows_and_unaligned_buffers() {
    for key_len in [16, 24, 32] {
        for direction in [Direction::Encrypt, Direction::Decrypt] {
            let key = vec![0xa7; key_len];
            let mut owner = Aes::new(&key, direction).unwrap();
            drop(key);
            let mut source = [0x5a; 16 * 8 + 3];
            let mut destination = vec![0x99; source.len()];
            for cbc in [false, true] {
                let mut iv = [0xf3; 16];
                for blocks in [0, 1, 4, 2, 8] {
                    let len = blocks * 16;
                    let mut outer = owner.as_mut();
                    {
                        let mut handle = outer.as_mut();
                        handle
                            .crypt(
                                &mut destination[1..1 + len],
                                &source[2..2 + len],
                                cbc.then_some(&mut iv),
                            )
                            .unwrap();
                    }
                    outer
                        .crypt_in_place(&mut source[1..1 + len], cbc.then_some(&mut iv))
                        .unwrap();
                }
                owner
                    .as_mut()
                    .crypt_in_place(&mut [], cbc.then_some(&mut iv))
                    .unwrap();
            }
        }
    }
}
