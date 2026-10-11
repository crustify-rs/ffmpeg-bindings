use ffmpeg::libavutil::twofish;

#[test]
fn batch10_size_is_the_c_global() {
    for _ in 0..4 {
        // SAFETY: this global is an immutable const int with static lifetime.
        let raw = unsafe { &ffi::av_twofish_size };
        assert_eq!(twofish::context_size(), raw);
        assert!(std::ptr::eq(twofish::context_size(), raw));
    }
}

use ffmpeg::libavutil::twofish::{InitError, InitStatus, Twofish};
use libavutil_sys as ffi;

#[test]
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
        let (constructed, constructed_status) = Twofish::new(&key, bits).unwrap();
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
                unsafe {
                    ffi::av_twofish_crypt(
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
                unsafe {
                    ffi::av_twofish_crypt(
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
        unsafe {
            ffi::av_twofish_crypt(
                owner.as_c_ptr(),
                output.as_mut_ptr(),
                input.as_ptr(),
                3,
                core::ptr::null_mut(),
                0,
            );
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
