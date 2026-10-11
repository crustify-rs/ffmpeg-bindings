//! C allocation observes the enum's ABI values, including invalid encodings.

use ffmpeg::libavutil::{ffi, hmac::HmacType};

fn c_accepts(algorithm: ffi::AVHMACType) -> bool {
    // SAFETY: C accepts all enum integers and returns NULL for unknown ones.
    // A successful allocation is uniquely owned, and even an uninitialized
    // hashing state is valid for av_hmac_free. No pointer escapes this helper.
    unsafe {
        let context = ffi::av_hmac_alloc(algorithm);
        let success = !context.is_null();
        ffi::av_hmac_free(context);
        success
    }
}

#[test]
fn algorithm_values_match_c_consumption() {
    for (safe, raw) in [
        (HmacType::MD5, ffi::AVHMACType::AV_HMAC_MD5),
        (HmacType::SHA1, ffi::AVHMACType::AV_HMAC_SHA1),
        (HmacType::SHA224, ffi::AVHMACType::AV_HMAC_SHA224),
        (HmacType::SHA256, ffi::AVHMACType::AV_HMAC_SHA256),
        (HmacType::SHA384, ffi::AVHMACType::AV_HMAC_SHA384),
        (HmacType::SHA512, ffi::AVHMACType::AV_HMAC_SHA512),
    ] {
        assert_eq!(safe.into_raw(), raw);
        assert_eq!(c_accepts(safe.into_raw()), c_accepts(raw));
        assert!(c_accepts(safe.into_raw()));
    }
    for value in [6, 100, u32::MAX, 1 << 31] {
        let raw = ffi::AVHMACType(value);
        let safe = HmacType::from_raw(raw);
        assert_eq!(c_accepts(safe.into_raw()), c_accepts(raw));
        assert!(!c_accepts(safe.into_raw()));
    }
}
