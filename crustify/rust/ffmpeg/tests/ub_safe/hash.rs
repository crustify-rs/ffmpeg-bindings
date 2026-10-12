//! Public ownership and borrow paths; no foreign memory is referenced in Rust.

use ffmpeg::libavutil::hash::HashContext;

const NAMES: [&std::ffi::CStr; 15] = [
    c"MD5",
    c"murmur3",
    c"RIPEMD128",
    c"RIPEMD160",
    c"RIPEMD256",
    c"RIPEMD320",
    c"SHA160",
    c"SHA224",
    c"SHA256",
    c"SHA512/224",
    c"SHA512/256",
    c"SHA384",
    c"SHA512",
    c"CRC32",
    c"adler32",
];

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn owners_shared_views_and_exclusive_reborrows() {
    for name in NAMES {
        for _ in 0..8 {
            let mut owner = HashContext::new(name).unwrap();
            let shared = owner.as_ref();
            let copy = shared;
            assert_eq!(shared.name(), copy.name());
            assert!(shared.size() > 0);
            let static_name = shared.name();
            let mut mutable = owner.as_mut();
            assert_eq!(mutable.as_ref().name(), name);
            mutable.as_mut().init();
            mutable.init();
            assert_eq!(mutable.as_ref().name(), static_name);
            drop(owner);
            // The name is static, unlike the borrowed context handle.
            assert_eq!(static_name, name);
        }
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn ephemeral_algorithm_name_is_not_retained() {
    let mut owner = {
        let name = std::ffi::CString::new("sHa256").unwrap();
        HashContext::new(&name).unwrap()
    };
    owner.as_mut().init();
    assert_eq!(owner.as_ref().name(), c"SHA256");
    assert_eq!(owner.as_ref().size(), 32);
}

use ffmpeg::libavutil::hash::name;

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn batch10_static_names_and_invalid_indices() {
    assert!(name(i32::MIN).is_none());
    assert!(name(-1).is_none());
    assert!(name(i32::MAX).is_none());
    let first: &'static std::ffi::CStr = name(0).unwrap();
    assert_eq!(first, c"MD5");
    for i in 0..64 {
        if let Some(s) = name(i) {
            assert!(!s.to_bytes().is_empty());
        }
    }
    assert_eq!(name(0), Some(first));
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn hashing_foreign_writes_and_reborrows() {
    for name in NAMES {
        let mut owner = HashContext::new(name).unwrap();
        let mut outer = owner.as_mut();
        for len in [0, 1, 15, 16, 55, 56, 63, 64, 65, 127, 128, 129, 513] {
            let mut input = vec![0xA3; len + 1];
            outer.as_mut().update(&input[1..]);
            input.fill(0);
            drop(input);
            let mut out = [0x99; 143];
            let mut inner = outer.as_mut();
            inner.finalize_bin(&mut out[1..]);
            out.fill(0);
            inner.update(&[]);
            let _ = inner.finalize(); // current C post-final states are memory-safe
            inner.update(b"after final");
            inner.finalize_hex(&mut out[1..]).unwrap();
            inner.finalize_b64(&mut out[1..]).unwrap();
            inner.init();
        }
        for size in 0..=140 {
            let mut out = vec![0xA5; size + 1];
            outer.finalize_bin(&mut out[1..]);
            outer.init();
            if size >= 2 {
                outer.finalize_hex(&mut out[1..]).unwrap();
            }
            outer.init();
            if size >= 1 {
                outer.finalize_b64(&mut out[1..]).unwrap();
            }
            outer.init();
        }
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn independent_hash_contexts_on_threads() {
    let workers: Vec<_> = (0..4)
        .map(|_| {
            std::thread::spawn(|| {
                for name in NAMES {
                    let mut owner = HashContext::new(name).unwrap();
                    owner.as_mut().update(b"thread-local message");
                    let _ = owner.as_mut().finalize();
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
fn owned_digest_outlives_its_context() {
    let digest = {
        let mut owner = HashContext::new(c"SHA256").unwrap();
        owner.as_mut().update(b"message");
        owner.as_mut().finalize()
    };
    assert_eq!(digest.len(), 32);
    assert_ne!(digest, vec![0; 32]);
}
