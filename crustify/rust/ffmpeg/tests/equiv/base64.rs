use ffmpeg::libavutil::{base64, ffi};

#[test]
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
            // SAFETY: null output selects C's validity-only path; valid CStr.
            let valid =
                unsafe { ffi::av_base64_decode(core::ptr::null_mut(), reference.as_ptr(), 0) };
            assert_eq!(
                base64::validate(&text),
                if valid < 0 { Err(valid) } else { Ok(()) }
            );
        }
    }
}
