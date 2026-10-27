#![expect(
    clippy::no_effect_underscore_binding,
    reason = "assertions keep the form of the C behaviour they check"
)]
use ffmpeg_bindings::libavutil::libavutil::hmac::HmacType;

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
        assert_eq!(HmacType::from_raw(value).into_raw(), value);
    }
}

use ffmpeg_bindings::libavutil::libavutil::hmac::Hmac;

#[test]
#[cfg_attr(miri, ignore = "calls C")]
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
#[cfg_attr(miri, ignore = "calls C")]
fn repeated_construction_and_destruction() {
    for _ in 0..128 {
        let mut owner = Hmac::new(HmacType::SHA256, &[7; 256]).unwrap();
        owner.as_mut().init(b"").unwrap();
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn independent_contexts_on_threads() {
    let threads: Vec<_> = (0..4)
        .map(|_| {
            std::thread::spawn(|| {
                // No Send/Sync assertions: each thread constructs and releases its
                // own context, exercising the C allocators without sharing objects.
                for _ in 0..16 {
                    let mut owner = Hmac::new(HmacType::SHA1, b"key").unwrap();
                    owner.as_mut().init(&[0; 129]).unwrap();
                    owner.as_mut().update(b"thread-local data").unwrap();
                    let mut output = [0; 20];
                    owner.as_mut().finalize(&mut output).unwrap();
                    owner
                        .as_mut()
                        .calculate(b"new", b"key", &mut output)
                        .unwrap();
                }
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn allocation_failures_clean_up() {
    const CHILD: &str = "CRUSTIFY_CRYPTO_B3_ALLOC_FAILURE_CHILD";
    if std::env::var_os(CHILD).is_some() {
        // Isolate the process-wide (atomic) allocation cap so parallel tests
        // never inherit it. Every object still comes from its safe constructor.
        ffmpeg_bindings::libavutil::libavutil::mem::set_max_alloc(1);
        assert!(matches!(Hmac::new(HmacType::SHA512, b"key"), Err(-12)));
        assert!(matches!(
            ffmpeg_bindings::libavutil::libavutil::des::Des::new_boxed(&[0; 24], false),
            Err(-12)
        ));
        // On the campaign ABI, AVHMAC is 176 bytes and AVSHA512 is 208.
        // This admits the parent but rejects its child hash allocation,
        // exercising C's cleanup before a Rust owner can be formed.
        ffmpeg_bindings::libavutil::libavutil::mem::set_max_alloc(200);
        assert!(matches!(Hmac::new(HmacType::SHA512, b"key"), Err(-12)));
        ffmpeg_bindings::libavutil::libavutil::mem::set_max_alloc(usize::MAX);
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

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn hashing_foreign_writes_and_reborrows() {
    for algorithm in [
        HmacType::MD5,
        HmacType::SHA1,
        HmacType::SHA224,
        HmacType::SHA256,
        HmacType::SHA384,
        HmacType::SHA512,
    ] {
        let mut owner = Hmac::new(algorithm, b"temporary key").unwrap();
        let mut outer = owner.as_mut();
        for len in [0, 1, 63, 64, 65, 127, 128, 129, 257] {
            let mut input = vec![0xAB; len + 1];
            outer.as_mut().update(&input[1..]).unwrap();
            input.fill(0);
            drop(input);
            let mut output = [0xAA; 81];
            outer.as_mut().finalize(&mut output[1..]).unwrap();
            output.fill(0);
            let mut key = vec![7; len + 1];
            outer
                .as_mut()
                .calculate(b"", &key[1..], &mut output[1..])
                .unwrap();
            key.fill(0);
            drop(key);
            outer.update(b"post-final").unwrap();
            outer.finalize(&mut output[1..]).unwrap();
            outer.init(b"").unwrap();
        }
    }
}
