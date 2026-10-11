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
fn ephemeral_algorithm_name_is_not_retained() {
    let mut owner = {
        let name = std::ffi::CString::new("sHa256").unwrap();
        HashContext::new(&name).unwrap()
    };
    owner.as_mut().init();
    assert_eq!(owner.as_ref().name(), c"SHA256");
    assert_eq!(owner.as_ref().size(), 32);
}
