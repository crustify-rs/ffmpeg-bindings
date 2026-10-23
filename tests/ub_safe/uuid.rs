use ffmpeg_bindings::libavutil::libavutil::uuid;

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn batch10_copy_nil_equality_parse_and_reborrow() {
    let mut output = [0; 37];
    for i in 0..=255 {
        let original = [i; 16];
        let mut copy = [0; 16];
        uuid::copy(&mut copy, &original);
        assert!(uuid::equal(&copy, &original));
        let text = uuid::unparse(&copy, &mut output);
        assert_eq!(uuid::parse(text).unwrap(), original);
        assert_eq!(uuid::parse_range(text.to_bytes()).unwrap(), original);
        let urn =
            std::ffi::CString::new([b"URN:UUID:".as_slice(), text.to_bytes()].concat()).unwrap();
        assert_eq!(uuid::urn_parse(&urn).unwrap(), original);
        uuid::nil(&mut copy);
        assert_eq!(copy, [0; 16]);
        output.fill(0);
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn batch10_parse_invalid_lengths_bytes_and_prefixes() {
    for len in 0..100 {
        if len != 36 {
            let input = std::ffi::CString::new(vec![b'0'; len]).unwrap();
            assert!(uuid::parse(&input).is_err());
            assert!(uuid::parse_range(input.to_bytes()).is_err());
        }
    }
    for b in 1..=255 {
        let mut text = *b"6021b21e-894e-43ff-8317-1ca891c1c49b";
        text[0] = b;
        let input = std::ffi::CString::new(text).unwrap();
        assert_eq!(uuid::parse(&input), uuid::parse_range(input.to_bytes()));
    }
    for input in [
        c"",
        c"urn",
        c"xurn:uuid:6021b21e-894e-43ff-8317-1ca891c1c49b",
        c"urn:uuid:",
    ] {
        assert!(uuid::urn_parse(input).is_err());
    }
}
