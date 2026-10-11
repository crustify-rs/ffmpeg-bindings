use ffmpeg::libavutil::{ffi, uuid};

#[test]
fn batch10_mutation_and_format_sequences_match_c() {
    let mut safe = [0; 16];
    let mut raw = [0; 16];
    for byte in 0..=255 {
        let source = [byte; 16];
        let independent = source;
        uuid::copy(&mut safe, &source);
        // SAFETY: distinct exact-size 16-byte source and exclusive destination.
        unsafe { ffi::av_uuid_copy(raw.as_mut_ptr(), independent.as_ptr()) };
        assert_eq!(safe, raw);
        // SAFETY: both arrays are readable for all sixteen compared bytes.
        let eq = unsafe { ffi::av_uuid_equal(raw.as_ptr(), independent.as_ptr()) };
        assert_eq!(uuid::equal(&safe, &source), eq != 0);
        let mut left = [0xAA; 37];
        let mut right = [0xAA; 37];
        uuid::unparse(&safe, &mut left);
        // SAFETY: readable 16-byte input and independent writable 37-byte output.
        unsafe { ffi::av_uuid_unparse(raw.as_ptr(), right.as_mut_ptr().cast()) };
        assert_eq!(left, right);
        uuid::nil(&mut safe);
        // SAFETY: raw is exclusive writable 16-byte storage.
        unsafe { ffi::av_uuid_nil(raw.as_mut_ptr()) };
        assert_eq!(safe, raw);
        // SAFETY: both arrays are readable for all sixteen compared bytes.
        let eq = unsafe { ffi::av_uuid_equal(raw.as_ptr(), independent.as_ptr()) };
        assert_eq!(uuid::equal(&safe, &source), eq != 0);
    }
}

#[test]
fn batch10_parsers_match_c_including_skipped_separators() {
    let mut inputs: Vec<_> = [
        "",
        "6021b21e-894e-43ff-8317-1ca891c1c49b",
        "6021B21E-894E-43FF-8317-1CA891C1C49B",
        "6021b21e_894e_43ff_8317_1ca891c1c49b",
        "6021g21e-894e-43ff-8317-1ca891c1c49b",
        "6021b21e-894e-43ff-8317-1ca891c1c49z",
    ]
    .into_iter()
    .map(|s| std::ffi::CString::new(s).unwrap())
    .collect();
    inputs.extend((1..=255).map(|b| {
        let mut s = *b"6021b21e-894e-43ff-8317-1ca891c1c49b";
        s[0] = b;
        std::ffi::CString::new(s).unwrap()
    }));
    for input in inputs {
        let mut raw = [0xAA; 16];
        let independent = input.clone();
        // SAFETY: CStr input and exclusive 16-byte output, no retention.
        let ret = unsafe { ffi::av_uuid_parse(independent.as_ptr(), raw.as_mut_ptr()) };
        assert_eq!(
            uuid::parse(&input),
            if ret < 0 { Err(ret) } else { Ok(raw) }
        );
        let mut range = vec![0; input.to_bytes().len() + 1];
        range[..input.to_bytes().len()].copy_from_slice(input.to_bytes());
        // SAFETY: both endpoints belong to the same live allocation, end is
        // one-past the logical range and C never reads it; exclusive output.
        let ret = unsafe {
            ffi::av_uuid_parse_range(
                range.as_ptr().cast(),
                range.as_ptr().add(input.to_bytes().len()).cast(),
                raw.as_mut_ptr(),
            )
        };
        assert_eq!(
            uuid::parse_range(input.to_bytes()),
            if ret < 0 { Err(ret) } else { Ok(raw) }
        );
        for prefix in [
            "urn:uuid:",
            "URN:UUID:",
            "UrN:uUiD:",
            "xurn:uuid:",
            "urn:uid:",
        ] {
            let urn =
                std::ffi::CString::new([prefix.as_bytes(), input.to_bytes()].concat()).unwrap();
            let independent = urn.clone();
            // SAFETY: CStr input and exclusive 16-byte output, no retention.
            let ret = unsafe { ffi::av_uuid_urn_parse(independent.as_ptr(), raw.as_mut_ptr()) };
            assert_eq!(
                uuid::urn_parse(&urn),
                if ret < 0 { Err(ret) } else { Ok(raw) }
            );
        }
    }
}
