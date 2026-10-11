use ffmpeg::libavutil::blowfish::{Blowfish, BlowfishBox};

#[test]
fn foreign_initialization_across_reborrows() {
    for len in [1, 2, 8, 24, 56, 72, 128] {
        let key = vec![0x5a; len].into_boxed_slice();
        let mut inline = Blowfish::new(&key).unwrap();
        let mut boxed = Blowfish::new_boxed(&key).unwrap();
        let mut zeroed = BlowfishBox::new_zeroed().unwrap();
        assert_eq!(zeroed.as_ref().p(), [0; 18]);
        assert_eq!(zeroed.as_ref().s(), [[0; 256]; 4]);
        zeroed.as_mut().init(&key).unwrap();
        assert_eq!(zeroed.as_ref().p(), boxed.as_ref().p());
        drop(key); // Neither context retained the key.
        assert_eq!(inline.as_ref().p(), boxed.as_ref().p());
        assert_eq!(inline.as_ref().s(), boxed.as_ref().s());
        let original = boxed.as_ref().p();
        let mut copy = boxed.clone();
        {
            let mut outer = boxed.as_mut();
            let mut inner = outer.as_mut();
            inner.init(b"replacement key").unwrap();
            let shared = inner.as_ref();
            assert_ne!(shared.p(), original);
            let twin = shared;
            assert_eq!(shared.p(), twin.p());
            outer.set_p([u32::MAX; 18]);
        }
        inline.as_mut().set_s([[u32::MAX; 256]; 4]);
        assert_eq!(inline.as_ref().s(), [[u32::MAX; 256]; 4]);
        assert_eq!(boxed.as_ref().p(), [u32::MAX; 18]);
        assert_eq!(copy.as_ref().p(), original);
        drop(boxed); // Deep copy has its own allocation.
        copy.as_mut().set_s([[0; 256]; 4]);
        assert_eq!(copy.as_ref().s(), [[0; 256]; 4]);
    }
}

#[test]
fn allocation_failure_is_reported_without_leaking() {
    use ffmpeg::libavutil::{blowfish, camellia, mem};
    const CHILD: &str = "CRUSTIFY_CRYPTO_BATCH1_ALLOC_FAILURE_CHILD";
    if std::env::var_os(CHILD).is_some() {
        // This child runs only this exact test: changing C's process-wide
        // allocation limit cannot affect other tests or constructors.
        mem::set_max_alloc(1);
        assert!(matches!(
            Blowfish::new_boxed(b"key"),
            Err(blowfish::InitError::AllocationFailed)
        ));
        assert!(matches!(
            camellia::Camellia::new(&[0; 16]),
            Err(camellia::InitError::AllocationFailed)
        ));
        // Inline construction requires no C allocation.
        assert!(Blowfish::new(b"key").is_ok());
        return;
    }
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "blowfish::allocation_failure_is_reported_without_leaking",
            "--nocapture",
        ])
        .env(CHILD, "1")
        .status()
        .unwrap();
    assert!(status.success());
}

use ffmpeg::libavutil::aes::Direction;

#[test]
fn processing_across_reborrows_and_unaligned_buffers() {
    for key_len in [1, 8, 56] {
        for direction in [Direction::Encrypt, Direction::Decrypt] {
            let key = vec![0xa7; key_len];
            let mut owner = Blowfish::new(&key).unwrap();
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
            let mut halves = [0x12345678, 0xabcdef01];
            owner.as_ref().crypt_ecb(&mut halves, direction);
            let boxed = Blowfish::new_boxed(&[0; 8]).unwrap();
            boxed.as_ref().crypt_ecb(&mut halves, direction);
            boxed
                .as_ref()
                .crypt_in_place(&mut source[1..9], None, direction)
                .unwrap();
        }
    }
}
