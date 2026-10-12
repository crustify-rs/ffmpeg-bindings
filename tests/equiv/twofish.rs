use ffmpeg::libavutil::twofish;

#[test]
#[cfg_attr(miri, ignore = "reads a C global")]
fn batch10_size_is_the_c_global() {
    for _ in 0..4 {
        // SAFETY: this global is an immutable const int with static lifetime.
        let raw = unsafe { &ffi::av_twofish_size };
        assert_eq!(twofish::context_size(), raw);
        assert!(std::ptr::eq(twofish::context_size(), raw));
    }
}

use ffmpeg::libavutil::aes::Direction;
use ffmpeg::libavutil::twofish::{InitError, InitStatus, Twofish};
use libavutil_sys as ffi;

/// The safe direction for C's `decrypt` flag.
fn direction(decrypt: bool) -> Direction {
    if decrypt {
        Direction::Decrypt
    } else {
        Direction::Encrypt
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn lifecycle_status_and_rekey_match_c() {
    let (mut owner, _) = Twofish::new(&[], 0).unwrap();
    // SAFETY: the allocator yields an independently owned full C context.
    let reference = unsafe { ffi::av_twofish_alloc() };
    assert!(!reference.is_null());
    for bits in [
        0,
        7,
        8,
        127,
        128,
        129,
        191,
        192,
        193,
        255,
        256,
        257,
        i32::MAX,
    ] {
        let length = (bits.min(256) / 8) as usize;
        let key = vec![bits as u8; length];
        let (mut constructed, constructed_status) = Twofish::new(&key, bits).unwrap();
        let status = owner.as_mut().init(&key, bits).unwrap();
        assert_eq!(constructed_status, status);
        // SAFETY: reference is live exclusive C storage and key has exactly
        // min(bits,256)/8 bytes, with a nonnull pointer even when empty.
        let expected_status = unsafe { ffi::av_twofish_init(reference, key.as_ptr(), bits) };
        assert_eq!(
            status,
            if expected_status == 0 {
                InitStatus::Exact
            } else {
                InitStatus::Adjusted
            }
        );
        let input = [0x5a; 48];
        let mut output = [0; 48];
        let mut expected = [0; 48];
        let mut constructed_output = [0; 48];
        for cbc in [false, true] {
            let mut iv = [7; 16];
            let mut expected_iv = iv;
            let mut constructed_iv = iv;
            for decrypt in [0, 1, 0] {
                // SAFETY: initialized contexts with no live borrowed handles;
                // independent arrays cover three blocks, and IVs one block.
                // C reads/writes only these arrays and retains no pointers.
                owner
                    .as_mut()
                    .as_ref()
                    .crypt(
                        &mut output,
                        &input,
                        if cbc { Some(&mut iv) } else { None },
                        direction(decrypt != 0),
                    )
                    .unwrap();
                // SAFETY: reference uniquely owns a live initialized context; buffers
                // cover three blocks and IV one block, with no overlap/retention.
                unsafe {
                    ffi::av_twofish_crypt(
                        reference,
                        expected.as_mut_ptr(),
                        input.as_ptr(),
                        3,
                        if cbc {
                            expected_iv.as_mut_ptr()
                        } else {
                            core::ptr::null_mut()
                        },
                        decrypt,
                    );
                }
                // SAFETY: constructed owns a separate initialized C allocation
                // built with the same key/bits; arrays cover three blocks and
                // its independent IV covers one. C retains no pointers.
                constructed
                    .as_mut()
                    .as_ref()
                    .crypt(
                        &mut constructed_output,
                        &input,
                        if cbc { Some(&mut constructed_iv) } else { None },
                        direction(decrypt != 0),
                    )
                    .unwrap();
                assert_eq!(constructed_output, expected);
                assert_eq!(constructed_iv, expected_iv);
                assert_eq!(output, expected);
                assert_eq!(iv, expected_iv);
            }
        }
        // Rejected Rust input does not modify a previously valid schedule.
        assert_eq!(
            owner.as_mut().init(&[], -1),
            Err(InitError::NegativeKeyBits)
        );
        assert!(matches!(
            owner.as_mut().init(&[], 256),
            Err(InitError::KeyTooShort { .. })
        ));
        // SAFETY: a negative bit count makes C return before reading the key or
        // writing reference. The nonnull empty slice pointer is never read.
        let rejected = unsafe { ffi::av_twofish_init(reference, [].as_ptr(), -1) };
        assert_eq!(rejected, -(ffi::EINVAL as i32));
        // SAFETY: both unchanged contexts remain initialized, and buffers cover
        // three blocks. C retains no pointers.
        owner
            .as_mut()
            .as_ref()
            .crypt(&mut output, &input, None, Direction::Encrypt)
            .unwrap();
        // SAFETY: reference uniquely owns a live initialized context; buffers
        // cover three blocks and IV one block, with no overlap/retention.
        unsafe {
            ffi::av_twofish_crypt(
                reference,
                expected.as_mut_ptr(),
                input.as_ptr(),
                3,
                core::ptr::null_mut(),
                0,
            );
        }
        assert_eq!(output, expected);
    }
    // SAFETY: sole live allocation from av_twofish_alloc, no suballocations or
    // remaining borrowed handles; av_free is its paired release primitive.
    unsafe { ffi::av_free(reference.cast()) };
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn batch2_in_place_multi_call_buffers_and_iv_match_c() {
    let mut owner = Twofish::new(&[0x5b; 16], 128).unwrap().0;
    let reference = Twofish::new(&[0x5b; 16], 128).unwrap().0;
    for cbc in [false, true] {
        let mut iv = [0x38; 16];
        let mut expected_iv = iv;
        for decrypt in [false, true, true, false] {
            for count in [0usize, 1, 2, 17] {
                let mut data: Vec<_> = (0..count * 16 + 1).map(|i| (i * 37) as u8).collect();
                let mut expected = data.clone();
                owner
                    .as_mut()
                    .as_ref()
                    .crypt_in_place(
                        &mut data[1..],
                        if cbc { Some(&mut iv) } else { None },
                        direction(decrypt),
                    )
                    .unwrap();
                // SAFETY: reference uniquely owns an initialized independent C
                // context with no handles. Buffer covers count blocks and the
                // separate IV covers one; C explicitly permits exact overlap.
                unsafe {
                    ffi::av_twofish_crypt(
                        reference.as_c_ptr(),
                        expected[1..].as_mut_ptr(),
                        expected[1..].as_ptr(),
                        count as i32,
                        if cbc {
                            expected_iv.as_mut_ptr()
                        } else {
                            core::ptr::null_mut()
                        },
                        i32::from(decrypt),
                    )
                };
                assert_eq!(data, expected);
                assert_eq!(iv, expected_iv);
                let mut output = vec![0xa5; data.len() + 16];
                let mut expected_output = output.clone();
                owner
                    .as_mut()
                    .as_ref()
                    .crypt(
                        &mut output[1..],
                        &data[1..],
                        if cbc { Some(&mut iv) } else { None },
                        direction(decrypt),
                    )
                    .unwrap();
                // SAFETY: independent initialized context and disjoint input,
                // output, IV; only count full blocks are read/written.
                unsafe {
                    ffi::av_twofish_crypt(
                        reference.as_c_ptr(),
                        expected_output[1..].as_mut_ptr(),
                        expected[1..].as_ptr(),
                        count as i32,
                        if cbc {
                            expected_iv.as_mut_ptr()
                        } else {
                            core::ptr::null_mut()
                        },
                        i32::from(decrypt),
                    )
                };
                assert_eq!(output, expected_output);
                assert_eq!(iv, expected_iv);
                assert!(output[data.len()..].iter().all(|&b| b == 0xa5));
            }
        }
    }
}
