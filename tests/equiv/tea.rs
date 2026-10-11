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

use ffmpeg::libavutil::tea::Tea;
use libavutil_sys as ffi;

#[test]
fn lifecycle_and_rekey_match_c() {
    let key = [3; 16];
    let mut owner = Tea::new(&key, 64).unwrap();
    // SAFETY: the allocator yields an independently owned full C context.
    let reference = unsafe { ffi::av_tea_alloc() };
    assert!(!reference.is_null());
    for rounds in [-1, 0, 1, 2, 16, 63, 64, 65] {
        let key = [rounds as u8; 16];
        let constructed = Tea::new(&key, rounds).unwrap();
        owner.as_mut().init(&key, rounds);
        // SAFETY: reference is live exclusive C storage; key has 16 bytes and
        // is copied during the call, never retained.
        unsafe { ffi::av_tea_init(reference, key.as_ptr(), rounds) };
        // Observe both schedules with C: cipher wrapping is a later symbol
        // batch. Buffers/IVs are independent, and compared after each call.
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
                unsafe {
                    ffi::av_tea_crypt(
                        owner.as_c_ptr(),
                        output.as_mut_ptr(),
                        input.as_ptr(),
                        3,
                        if cbc {
                            iv.as_mut_ptr()
                        } else {
                            core::ptr::null_mut()
                        },
                        decrypt,
                    );
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
                unsafe {
                    ffi::av_tea_crypt(
                        constructed.as_c_ptr(),
                        constructed_output.as_mut_ptr(),
                        input.as_ptr(),
                        3,
                        if cbc {
                            constructed_iv.as_mut_ptr()
                        } else {
                            core::ptr::null_mut()
                        },
                        decrypt,
                    );
                }
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
