use ffmpeg::libavutil::{
    camellia::{self, Camellia, CamelliaBox},
    ffi,
};

#[test]
fn lifecycle_sequences_match_c() {
    let _serial = TEST_LOCK.lock().unwrap();
    // SAFETY: C returns a fresh zero-filled full-sized context, or NULL.
    // CamelliaBox's CamelliaFree policy frees it once with av_free; no C pointer is kept.
    let reference = unsafe { CamelliaBox::from_c(ffi::av_camellia_alloc()) }.unwrap();
    let mut safe = Camellia::new(&[0; 16]).unwrap();
    for len in [16, 24, 32, 16, 32, 24] {
        let key: Vec<u8> = (0..len).map(|i| (i * 37) as u8).collect();
        // SAFETY: full writable C context, exact readable key size and valid
        // key bit count. TEST_LOCK serializes Camellia reference tests, so raw SP
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
                    safe.as_ref()
                        .crypt(
                            &mut b,
                            &source,
                            cbc.then_some(&mut iv_b),
                            if decrypt != 0 {
                                Direction::Decrypt
                            } else {
                                Direction::Encrypt
                            },
                        )
                        .unwrap();
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
            safe.as_ref()
                .crypt(&mut b, &[0x72; 16], None, Direction::Encrypt)
                .unwrap();
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

static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

use ffmpeg::libavutil::aes::Direction;

#[test]
fn buffer_sequences_match_c() {
    let _serial = TEST_LOCK.lock().unwrap();
    for key_len in [16, 24, 32] {
        for direction in [Direction::Encrypt, Direction::Decrypt] {
            let key = vec![0xa7; key_len];
            let actual = Camellia::new(&key).unwrap();
            let reference = Camellia::new(&key).unwrap();
            let src: Vec<u8> = (0..16 * 8 + 3).map(|i| (i * 31) as u8).collect();
            for cbc in [false, true] {
                let (mut ia, mut ir) = ([0x73; 16], [0x73; 16]);
                for blocks in [0, 1, 2, 4, 8, 1] {
                    let len = blocks * 16;
                    let (mut a, mut r) = (vec![0xcc; len + 3], vec![0xcc; len + 3]);
                    actual
                        .as_ref()
                        .crypt(
                            &mut a[1..1 + len],
                            &src[2..2 + len],
                            cbc.then_some(&mut ia),
                            direction,
                        )
                        .unwrap();
                    let iv = if cbc {
                        ir.as_mut_ptr()
                    } else {
                        core::ptr::null_mut()
                    };
                    // SAFETY: independent initialized context, len whole readable/
                    // writable blocks in disjoint allocations; IV is a separate
                    // full block or NULL. This synchronous call retains nothing.
                    unsafe {
                        ffi::av_camellia_crypt(
                            reference.as_c_ptr(),
                            r.as_mut_ptr().add(1),
                            src.as_ptr().add(2),
                            blocks as i32,
                            iv,
                            i32::from(direction == Direction::Decrypt),
                        )
                    };
                    assert_eq!(a, r);
                    assert_eq!(ia, ir);
                    // Compare subsequent in-place processing and IV transitions.
                    actual
                        .as_ref()
                        .crypt_in_place(&mut a[1..1 + len], cbc.then_some(&mut ia), direction)
                        .unwrap();
                    // The assertions above reborrowed `r` and `ir`, retiring the
                    // pointers of the first call: take fresh ones, and a single
                    // pointer for both exactly aliased buffers.
                    let iv = if cbc {
                        ir.as_mut_ptr()
                    } else {
                        core::ptr::null_mut()
                    };
                    let place = r.as_mut_ptr();
                    // SAFETY: same initialized reference, exact-alias buffers
                    // explicitly supported by C, with len full blocks and separate IV.
                    unsafe {
                        ffi::av_camellia_crypt(
                            reference.as_c_ptr(),
                            place.add(1),
                            place.add(1),
                            blocks as i32,
                            iv,
                            i32::from(direction == Direction::Decrypt),
                        )
                    };
                    assert_eq!(a, r);
                    assert_eq!(ia, ir);
                }
            }
        }
    }
}
