use ffmpeg::libavutil::camellia::{Camellia, context_size};

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn reinitialize_across_reborrows() {
    let mut context = {
        let key = vec![0x42; 16].into_boxed_slice();
        Camellia::new(&key).unwrap()
    }; // C may not retain the key.
    for len in [24, 32, 16, 32, 24] {
        let key = vec![0xab; len].into_boxed_slice();
        let mut outer = context.as_mut();
        outer.as_mut().init(&key).unwrap();
        let _shared = outer.as_ref();
        outer.init(&key).unwrap();
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn concurrent_constructors_serialize_global_table() {
    let threads: Vec<_> = (0..8)
        .map(|byte| {
            std::thread::spawn(move || {
                for len in [16, 24, 32, 16, 32] {
                    let mut context = Camellia::new(&vec![byte; len]).unwrap();
                    context.as_mut().init(&[byte; 24]).unwrap();
                }
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
}

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

use ffmpeg::libavutil::aes::Direction;

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn processing_across_reborrows_and_unaligned_buffers() {
    for key_len in [16, 24, 32] {
        for direction in [Direction::Encrypt, Direction::Decrypt] {
            let key = vec![0xa7; key_len];
            let mut owner = Camellia::new(&key).unwrap();
            drop(key);
            let mut source = [0x5a; 16 * 8 + 3];
            let mut destination = vec![0x99; source.len()];
            for cbc in [false, true] {
                let mut iv = [0xf3; 16];
                for blocks in [0, 1, 4, 2, 8] {
                    let len = blocks * 16;
                    let mut outer = owner.as_mut();
                    {
                        let handle = outer.as_mut();
                        handle
                            .as_ref()
                            .crypt(
                                &mut destination[1..1 + len],
                                &source[2..2 + len],
                                cbc.then_some(&mut iv),
                                direction,
                            )
                            .unwrap();
                    }
                    outer
                        .as_ref()
                        .crypt_in_place(&mut source[1..1 + len], cbc.then_some(&mut iv), direction)
                        .unwrap();
                }
                owner
                    .as_ref()
                    .crypt_in_place(&mut [], cbc.then_some(&mut iv), direction)
                    .unwrap();
            }
        }
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn parallel_initialization_and_processing_share_lock() {
    let threads: Vec<_> = (0..8)
        .map(|seed| {
            std::thread::spawn(move || {
                for _ in 0..16 {
                    let mut owner = Camellia::new(&[seed; 32]).unwrap();
                    let mut data = [seed; 64];
                    owner
                        .as_ref()
                        .crypt_in_place(&mut data, None, Direction::Encrypt)
                        .unwrap();
                    owner.as_mut().init(&[seed; 16]).unwrap();
                    owner
                        .as_ref()
                        .crypt_in_place(&mut data, Some(&mut [0; 16]), Direction::Decrypt)
                        .unwrap();
                }
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
}
