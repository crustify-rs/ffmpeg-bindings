use ffmpeg::libavutil::{base64, ffi};

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn batch10_encode_buffers_match_c_after_each_call() {
    let data: Vec<_> = (0..160).map(|i| (i * 83) as u8).collect();
    let mut safe = [0xAA; 220];
    let mut raw = safe;
    for len in 0..160 {
        let input = data[..len].to_vec();
        let text = base64::encode(&data[..len], &mut safe).unwrap();
        // SAFETY: the independent buffers have sufficient capacity and all
        // input/output lengths fit int; C reads input and writes only raw.
        let ptr = unsafe {
            ffi::av_base64_encode(
                raw.as_mut_ptr().cast(),
                raw.len() as i32,
                input.as_ptr(),
                input.len() as i32,
            )
        };
        assert!(!ptr.is_null());
        assert_eq!(
            text.to_bytes_with_nul(),
            &raw[..text.to_bytes_with_nul().len()]
        );
        assert_eq!(safe, raw);
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn batch10_decode_errors_lengths_and_all_buffer_bytes_match_c() {
    let texts: Vec<_> = [
        "",
        "A",
        "AA",
        "AAA",
        "AAAA",
        "YWJjZA==",
        "=ignored",
        "A=",
        "AA=",
        "AAA=",
        "YWJjZA!",
        " Zg==",
        "Zg==!ignored",
    ]
    .into_iter()
    .map(|s| std::ffi::CString::new(s).unwrap())
    .chain((1..=255).map(|b| std::ffi::CString::new(vec![b; 9]).unwrap()))
    .collect();
    for text in texts {
        let reference = text.clone();
        for capacity in 0..16 {
            let mut safe = vec![0xAA; capacity];
            let mut raw = safe.clone();
            let mut empty = 0;
            let dst = if capacity == 0 {
                &raw mut empty
            } else {
                raw.as_mut_ptr()
            };
            // SAFETY: input is NUL-terminated and dst permits capacity writes.
            // C's actual bounded implementation also accepts short outputs.
            let ret = unsafe { ffi::av_base64_decode(dst, reference.as_ptr(), capacity as i32) };
            let expected = if ret < 0 { Err(ret) } else { Ok(ret as usize) };
            assert_eq!(base64::decode(&text, &mut safe), expected);
            assert_eq!(safe, raw);
            let null = core::ptr::null_mut();
            // SAFETY: null output selects C's validity-only path; valid CStr.
            let valid = unsafe { ffi::av_base64_decode(null, reference.as_ptr(), 0) };
            assert_eq!(
                base64::validate(&text),
                if valid < 0 { Err(valid) } else { Ok(()) }
            );
        }
    }
}

/// Outputs longer than C's `int` size field reach C as `INT_MAX` bytes, as a
/// C caller holding such a buffer passes them, rather than failing.
mod beyond_int_max {
    use core::ffi::CStr;

    use ffmpeg::libavutil::{base64, ffi};

    const LEN: usize = i32::MAX as usize + 1;

    /// BSan cost grows with a 2 GiB allocation's history (see the campaign's
    /// test recipes); the plain and other sanitized runs cover these.
    fn under_bsan() -> bool {
        std::env::var("CRUSTIFY_FFMPEG_VARIANT").as_deref() == Ok("bsan")
    }

    /// A zeroed allocation, so only the pages written are ever mapped, with
    /// a recognisable head.
    fn buffer() -> Vec<u8> {
        let mut buffer = vec![0u8; LEN];
        buffer[..32].fill(0xAA);
        buffer
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn encodes_and_decodes_into_outputs_beyond_int_max_as_c() {
        if under_bsan() {
            return;
        }
        let input = b"crustify";
        let mut raw = buffer();
        let (out, size) = (raw.as_mut_ptr().cast(), i32::MAX);
        // SAFETY: `raw` is writable for `INT_MAX` bytes and `input` readable
        // for its length; C writes the encoded string and retains neither.
        let ptr = unsafe { ffi::av_base64_encode(out, size, input.as_ptr(), 8) };
        assert!(!ptr.is_null());
        let expected = raw[..32].to_vec();
        drop(raw);
        let mut safe = buffer();
        let text = base64::encode(input, &mut safe).unwrap();
        assert_eq!(text, c"Y3J1c3RpZnk=");
        assert_eq!(safe[..32], expected);
        drop(safe);

        for text in [c"Y3J1c3RpZnk=", c"Y3J1c3RpZnk", c"Y3J1!", c""] {
            let mut raw = buffer();
            // SAFETY: NUL-terminated input and an output writable for
            // `INT_MAX` bytes; C writes the decoded bytes only.
            let ret = unsafe { ffi::av_base64_decode(raw.as_mut_ptr(), text.as_ptr(), i32::MAX) };
            let expected = (
                if ret < 0 { Err(ret) } else { Ok(ret as usize) },
                raw[..32].to_vec(),
            );
            drop(raw);
            let mut safe = buffer();
            let actual = base64::decode(text, &mut safe);
            assert_eq!((actual, safe[..32].to_vec()), expected, "{text:?}");
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn exact_and_short_outputs_still_follow_c() {
        let text: &CStr = c"Y3J1c3RpZnk=";
        for capacity in [0, 1, 7, 8, 9] {
            let mut raw = vec![0xAA; capacity];
            let mut safe = raw.clone();
            let mut empty = 0;
            let dst = if capacity == 0 {
                &raw mut empty
            } else {
                raw.as_mut_ptr()
            };
            // SAFETY: NUL-terminated input; `dst` permits `capacity` writes.
            let ret = unsafe { ffi::av_base64_decode(dst, text.as_ptr(), capacity as i32) };
            assert_eq!(base64::decode(text, &mut safe), Ok(ret as usize));
            assert_eq!(safe, raw);
        }
    }
}
