use ffmpeg::libavutil::tea;

#[test]
fn batch10_size_is_the_c_global() {
    for _ in 0..4 {
        // SAFETY: this global is an immutable const int with static lifetime.
        let raw = unsafe { &ffi::av_tea_size };
        assert_eq!(tea::context_size(), raw);
        assert!(std::ptr::eq(tea::context_size(), raw));
    }
}

use ffmpeg::libavutil::aes::Direction;
use ffmpeg::libavutil::tea::Tea;
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
fn lifecycle_and_rekey_match_c() {
    let key = [3; 16];
    let mut owner = Tea::new(&key, 64).unwrap();
    // SAFETY: the allocator yields an independently owned full C context.
    let reference = unsafe { ffi::av_tea_alloc() };
    assert!(!reference.is_null());
    for rounds in [-1, 0, 1, 2, 16, 63, 64, 65] {
        let key = [rounds as u8; 16];
        let mut constructed = Tea::new(&key, rounds).unwrap();
        owner.as_mut().init(&key, rounds);
        // SAFETY: reference is live exclusive C storage; key has 16 bytes and
        // is copied during the call, never retained.
        unsafe { ffi::av_tea_init(reference, key.as_ptr(), rounds) };
        // Observe the Rust schedule through the safe cipher wrapper. Buffers/IVs are independent, and compared after each call.
        let input = [0x5a; 24];
        let mut output = [0; 24];
        let mut expected = [0; 24];
        let mut constructed_output = [0; 24];
        for cbc in [false, true] {
            let mut iv = [7; 8];
            let mut expected_iv = iv;
            let mut constructed_iv = iv;
            for decrypt in [0, 1, 0] {
                // SAFETY: both contexts are live and initialized, with no live
                // borrowed handles; all arrays cover three blocks and IVs one.
                // C retains no pointer, and input and output do not overlap.
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
                    ffi::av_tea_crypt(
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
                // built with the same key/rounds; arrays cover three blocks and
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
    }
    // SAFETY: reference is the sole live allocation returned by av_tea_alloc,
    // retains no nested resources and has no remaining borrowed handles.
    unsafe { ffi::av_free(reference.cast()) };
}

#[test]
fn batch2_in_place_multi_call_buffers_and_iv_match_c() {
    let mut owner = Tea::new(&[0x5b; 16], 64).unwrap();
    let reference = Tea::new(&[0x5b; 16], 64).unwrap();
    for cbc in [false, true] {
        let mut iv = [0x38; 8];
        let mut expected_iv = iv;
        for decrypt in [false, true, true, false] {
            for count in [0usize, 1, 2, 17] {
                let mut data: Vec<_> = (0..count * 8 + 1).map(|i| (i * 37) as u8).collect();
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
                    ffi::av_tea_crypt(
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
                let mut output = vec![0xa5; data.len() + 8];
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
                    ffi::av_tea_crypt(
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
