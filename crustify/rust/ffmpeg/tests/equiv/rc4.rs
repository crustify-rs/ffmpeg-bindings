use ffmpeg::libavutil::{ffi, rc4::Rc4};

fn check(raw: &ffi::AVRC4, wrapped: ffmpeg::libavutil::rc4::Rc4Ref<'_>) {
    assert_eq!(wrapped.x(), raw.x);
    assert_eq!(wrapped.y(), raw.y);
    assert_eq!(wrapped.state().elems().collect::<Vec<_>>(), raw.state);
}

#[test]
fn init_fields_reset_and_stream_checkpoints_match_c() {
    let mut wrapped = Rc4::new(b"key").unwrap();
    let mut raw = ffi::AVRC4::default();
    for key_len in [1, 2, 16, 255, 256, 257, 1024] {
        let key: Vec<_> = (0..key_len).map(|i| (i * 37) as u8).collect();
        // SAFETY: raw is writable full AVRC4 storage and key is nonempty,
        // readable for its declared bit count; C retains neither pointer.
        let bits = (key.len() * 8) as i32;
        // SAFETY: raw is complete writable storage and the nonempty key has
        // bits / 8 readable bytes. C retains neither pointer.
        let status = unsafe { ffi::av_rc4_init(&raw mut raw, key.as_ptr(), bits, 1) };
        assert_eq!(wrapped.as_mut().init(&key), Ok(()));
        assert_eq!(status, 0);
        check(&raw, wrapped.as_ref());
        for (x, y) in [(0, 255), (i32::MIN, i32::MAX), (-1, 256)] {
            raw.x = x;
            raw.y = y;
            wrapped.as_mut().set_x(x);
            wrapped.as_mut().set_y(y);
            let bytes = core::array::from_fn(|i| (i * 11) as u8);
            raw.state = bytes;
            wrapped.as_mut().set_state(&bytes);
            check(&raw, wrapped.as_ref());
            for count in [0, 1, 255, 256, 513] {
                let mut expected = vec![0; count];
                let mut actual = vec![0; count];
                // SAFETY: both contexts are full independent initialized
                // allocations, output buffers have count bytes, null src
                // requests keystream, and unused iv is null. C retains nothing.
                unsafe {
                    ffi::av_rc4_crypt(
                        &raw mut raw,
                        expected.as_mut_ptr(),
                        core::ptr::null(),
                        count as i32,
                        core::ptr::null_mut(),
                        0,
                    );
                    ffi::av_rc4_crypt(
                        wrapped.as_c_ptr(),
                        actual.as_mut_ptr(),
                        core::ptr::null(),
                        count as i32,
                        core::ptr::null_mut(),
                        0,
                    );
                }
                assert_eq!(actual, expected);
                check(&raw, wrapped.as_ref());
            }
        }
    }
}

#[test]
fn inline_and_c_allocated_construction_have_identical_fields() {
    let inline = Rc4::from_key(b"inline").unwrap();
    let boxed = Rc4::new(b"inline").unwrap();
    assert_eq!(inline.as_ref().x(), boxed.as_ref().x());
    assert_eq!(inline.as_ref().y(), boxed.as_ref().y());
    assert_eq!(
        inline.as_ref().state().elems().collect::<Vec<_>>(),
        boxed.as_ref().state().elems().collect::<Vec<_>>()
    );
}
