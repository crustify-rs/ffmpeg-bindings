use ffmpeg::libavutil::{
    camellia::{self, Camellia, CamelliaBox},
    ffi,
};

#[test]
fn lifecycle_sequences_match_c() {
    // SAFETY: C returns a fresh zero-filled full-sized context, or NULL.
    // CamelliaBox's Heap policy frees it once with av_free; no C pointer is kept.
    let reference = unsafe { CamelliaBox::from_c(ffi::av_camellia_alloc()) }.unwrap();
    let mut safe = Camellia::new(&[0; 16]).unwrap();
    for len in [16, 24, 32, 16, 32, 24] {
        let key: Vec<u8> = (0..len).map(|i| (i * 37) as u8).collect();
        // SAFETY: full writable C context, exact readable key size and valid
        // key bit count. This is the sole Camellia equivalence test, so raw SP
        // accesses do not run concurrently with any init or crypt call.
        let status = unsafe { ffi::av_camellia_init(reference.as_c_ptr(), key.as_ptr(), len * 8) };
        assert_eq!(status, 0);
        safe.as_mut().init(&key).unwrap();
        drop(key);
        for decrypt in [0, 1] {
            let source = [0x57; 32];
            let (mut a, mut b) = ([0; 32], [0; 32]);
            let (mut iv_a, mut iv_b) = ([0x93; 16], [0x93; 16]);
            for cbc in [false, true] {
                let pa = if cbc {
                    iv_a.as_mut_ptr()
                } else {
                    core::ptr::null_mut()
                };
                let pb = if cbc {
                    iv_b.as_mut_ptr()
                } else {
                    core::ptr::null_mut()
                };
                // SAFETY: initialized full-sized independent contexts; both
                // destinations and source hold two blocks; IVs are writable
                // 16-byte locals or NULL. These sequential calls retain nothing.
                unsafe {
                    ffi::av_camellia_crypt(
                        reference.as_c_ptr(),
                        a.as_mut_ptr(),
                        source.as_ptr(),
                        2,
                        pa,
                        decrypt,
                    );
                    ffi::av_camellia_crypt(
                        safe.as_c_ptr(),
                        b.as_mut_ptr(),
                        source.as_ptr(),
                        2,
                        pb,
                        decrypt,
                    );
                }
                assert_eq!(a, b);
                assert_eq!(iv_a, iv_b);
            }
        }
        // A rejected reinitialization must not disturb the usable schedule.
        assert!(safe.as_mut().init(&[0; 15]).is_err());
        let (mut a, mut b) = ([0; 16], [0; 16]);
        // SAFETY: initialized contexts, one readable/writable block, NULL IV;
        // exclusive sequential raw access, with no retained pointers.
        unsafe {
            ffi::av_camellia_crypt(
                reference.as_c_ptr(),
                a.as_mut_ptr(),
                [0x72; 16].as_ptr(),
                1,
                core::ptr::null_mut(),
                0,
            );
            ffi::av_camellia_crypt(
                safe.as_c_ptr(),
                b.as_mut_ptr(),
                [0x72; 16].as_ptr(),
                1,
                core::ptr::null_mut(),
                0,
            );
        }
        assert_eq!(a, b);
    }
}

#[test]
fn batch10_size_is_the_c_global() {
    for _ in 0..4 {
        // SAFETY: this global is an immutable const int with static lifetime.
        let raw = unsafe { &ffi::av_camellia_size };
        assert_eq!(camellia::context_size(), raw);
        assert!(std::ptr::eq(camellia::context_size(), raw));
    }
}
