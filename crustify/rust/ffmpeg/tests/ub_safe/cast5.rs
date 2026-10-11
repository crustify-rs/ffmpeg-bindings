use ffmpeg::libavutil::cast5::{Cast5, InitError};

#[test]
fn keys_are_copied_and_handles_reborrow_exclusively() {
    for initial_len in 5..=16 {
        let mut owner = {
            let key = vec![0xa5; initial_len];
            Cast5::new(&key).unwrap()
        }; // No key storage remains alive here.
        let shared = owner.as_ref();
        let _shared_copy = shared;
        let mut exclusive = owner.as_mut();
        for len in 5..=16 {
            let key = vec![len as u8; len];
            exclusive.as_mut().init(&key).unwrap();
            let _shared_reborrow = exclusive.as_ref();
        }
        for len in [0, 1, 4, 17, 64] {
            assert_eq!(
                exclusive.init(&vec![0; len]),
                Err(InitError::InvalidKeyLength)
            );
        }
        exclusive.init(&[0; 5]).unwrap();
    } // Each allocation freed once, after all of its handles expire.
}

#[test]
fn repeated_construction_and_destruction() {
    for i in 0..256 {
        let mut owner = Cast5::new(&[i as u8; 16]).unwrap();
        owner.as_mut().init(&[i as u8; 5]).unwrap();
    }
}

#[test]
fn allocation_failure_returns_an_error_without_an_owner() {
    // Isolate the process-wide allocator limit from every other test.
    const ISOLATED: &str = "CRUSTIFY_CAST5_ALLOC_FAILURE_CHILD";
    if std::env::var_os(ISOLATED).is_none() {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "cast5::allocation_failure_returns_an_error_without_an_owner",
                "--test-threads=1",
            ])
            .env(ISOLATED, "1")
            .status()
            .unwrap();
        assert!(status.success());
        return;
    }
    ffmpeg::libavutil::mem::set_max_alloc(0);
    assert!(matches!(
        Cast5::new(&[0; 5]),
        Err(InitError::AllocationFailed)
    ));
    // Invalid input is still rejected before even attempting allocation.
    assert!(matches!(
        Cast5::new(&[0; 4]),
        Err(InitError::InvalidKeyLength)
    ));
    ffmpeg::libavutil::mem::set_max_alloc(i32::MAX as usize);
    drop(Cast5::new(&[0; 16]).unwrap());
}

use ffmpeg::libavutil::cast5::context_size;

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

use ffmpeg::libavutil::aes::Direction;

#[test]
fn processing_across_reborrows_and_unaligned_buffers() {
    for key_len in [5, 10, 11, 16] {
        for direction in [Direction::Encrypt, Direction::Decrypt] {
            let key = vec![0xa7; key_len];
            let mut owner = Cast5::new(&key).unwrap();
            drop(key);
            let mut source = [0x5a; 8 * 8 + 3];
            let mut destination = vec![0x99; source.len()];
            for cbc in [false, true] {
                let mut iv = [0xf3; 8];
                for blocks in [0, 1, 4, 2, 8] {
                    let len = blocks * 8;
                    let mut outer = owner.as_mut();
                    {
                        let handle = outer.as_mut();
                        handle
                            .as_ref()
                            .crypt_cbc(
                                &mut destination[1..1 + len],
                                &source[2..2 + len],
                                cbc.then_some(&mut iv),
                                direction,
                            )
                            .unwrap();
                    }
                    outer
                        .as_ref()
                        .crypt_cbc_in_place(
                            &mut source[1..1 + len],
                            cbc.then_some(&mut iv),
                            direction,
                        )
                        .unwrap();
                }
                owner
                    .as_ref()
                    .crypt_cbc_in_place(&mut [], cbc.then_some(&mut iv), direction)
                    .unwrap();
            }
            owner
                .as_ref()
                .crypt(&mut destination[1..17], &source[2..18], direction)
                .unwrap();
            owner
                .as_ref()
                .crypt_in_place(&mut source[1..17], direction)
                .unwrap();
        }
    }
}
