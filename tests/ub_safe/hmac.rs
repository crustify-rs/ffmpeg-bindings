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
