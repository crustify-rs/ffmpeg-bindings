use ffmpeg_bindings::libavutil::libavutil::base64::{decode, encode, validate};

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn batch10_roundtrips_truncation_and_reused_buffers() {
    let input: Vec<_> = (0..2048).map(|i| (i * 53) as u8).collect();
    for len in 0..80 {
        let bytes = &input[1..1 + len];
        let mut encoded = vec![0xAA; len.div_ceil(3) * 4 + 1];
        let text = encode(bytes, &mut encoded).unwrap();
        validate(text).unwrap();
        for capacity in [0, 1, 2, 3, len, len + 3] {
            let mut output = vec![0xAA; capacity];
            let n = decode(text, &mut output).unwrap();
            assert_eq!(n, len.min(capacity));
            assert_eq!(&output[..n], &bytes[..n]);
            output.fill(0);
            assert_eq!(decode(text, &mut output), Ok(n));
        }
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn batch10_malformed_nul_padding_and_high_bytes() {
    for byte in 1..=255 {
        let text = std::ffi::CString::new(vec![byte; 17]).unwrap();
        for capacity in 0..8 {
            let mut buffer = vec![0xAA; capacity];
            let result = decode(&text, &mut buffer);
            assert_eq!(result.is_ok(), validate(&text).is_ok());
        }
    }
    for text in [
        c"",
        c"A",
        c"AA",
        c"AAA",
        c"AAAA",
        c"=ignored!",
        c"AA=ignored!",
    ] {
        assert!(validate(text).is_ok());
    }
    assert!(validate(c"Zg==").is_ok());
    assert!(validate(c"Z g").is_err());
}
