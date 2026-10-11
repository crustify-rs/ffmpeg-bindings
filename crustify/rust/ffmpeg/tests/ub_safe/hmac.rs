use ffmpeg::libavutil::{ffi, hmac::HmacType};

#[test]
fn algorithms_and_unknown_values_are_plain_values() {
    for algorithm in [
        HmacType::MD5,
        HmacType::SHA1,
        HmacType::SHA224,
        HmacType::SHA256,
        HmacType::SHA384,
        HmacType::SHA512,
    ] {
        assert_eq!(HmacType::from_raw(algorithm.into_raw()), algorithm);
    }
    for value in [6, 7, u32::MAX, 1 << 31] {
        assert_eq!(
            HmacType::from_raw(ffi::AVHMACType(value)).into_raw().0,
            value
        );
    }
}

use ffmpeg::libavutil::hmac::Hmac;

#[test]
fn keyed_owners_and_reborrows() {
    for algorithm in [
        HmacType::MD5,
        HmacType::SHA1,
        HmacType::SHA224,
        HmacType::SHA256,
        HmacType::SHA384,
        HmacType::SHA512,
    ] {
        let mut owner = Hmac::new(algorithm, b"").unwrap();
        let shared = owner.as_ref();
        let _copied = shared;
        let mut exclusive = owner.as_mut();
        let _shared = exclusive.as_ref();
        for len in [0, 1, 63, 64, 65, 127, 128, 129, 1024] {
            let key = vec![0xA5; len];
            exclusive.as_mut().init(&key).unwrap();
            drop(key); // C retains no pointer to the key.
        }
    }
}

#[test]
fn repeated_construction_and_destruction() {
    for _ in 0..128 {
        let mut owner = Hmac::new(HmacType::SHA256, &[7; 256]).unwrap();
        owner.as_mut().init(b"").unwrap();
    }
}

#[test]
fn independent_contexts_on_threads() {
    let threads: Vec<_> = (0..4)
        .map(|_| {
            std::thread::spawn(|| {
                // No Send/Sync assertions: each thread constructs and releases its
                // own context, exercising the C allocators without sharing objects.
                for _ in 0..16 {
                    let mut owner = Hmac::new(HmacType::SHA1, b"key").unwrap();
                    owner.as_mut().init(&[0; 129]).unwrap();
                }
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
}

#[test]
fn allocation_failures_clean_up() {
    const CHILD: &str = "CRUSTIFY_CRYPTO_B3_ALLOC_FAILURE_CHILD";
    if std::env::var_os(CHILD).is_some() {
        // Isolate the process-wide (atomic) allocation cap so parallel tests
        // never inherit it. Every object still comes from its safe constructor.
        ffmpeg::libavutil::mem::set_max_alloc(1);
        assert!(matches!(Hmac::new(HmacType::SHA512, b"key"), Err(-12)));
        assert!(matches!(
            ffmpeg::libavutil::des::Des::new_boxed(&[0; 24], false),
            Err(-12)
        ));
        // On the campaign ABI, AVHMAC is 176 bytes and AVSHA512 is 208.
        // This admits the parent but rejects its child hash allocation,
        // exercising C's cleanup before a Rust owner can be formed.
        ffmpeg::libavutil::mem::set_max_alloc(200);
        assert!(matches!(Hmac::new(HmacType::SHA512, b"key"), Err(-12)));
        ffmpeg::libavutil::mem::set_max_alloc(usize::MAX);
        let _restored = Hmac::new(HmacType::SHA512, b"key").unwrap();
        return;
    }
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "hmac::allocation_failures_clean_up",
            "--test-threads=1",
        ])
        .env(CHILD, "1")
        .status()
        .unwrap();
    assert!(status.success());
}
