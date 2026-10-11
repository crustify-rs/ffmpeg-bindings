//! C allocation observes the enum's ABI values, including invalid encodings.

use ffmpeg::libavutil::{ffi, hmac::HmacType};

fn c_accepts(algorithm: ffi::AVHMACType) -> bool {
    // SAFETY: C accepts all enum integers and returns NULL for unknown ones.
    // A successful allocation is uniquely owned, and even an uninitialized
    // hashing state is valid for av_hmac_free. No pointer escapes this helper.
    unsafe {
        let context = ffi::av_hmac_alloc(algorithm);
        let success = !context.is_null();
        ffi::av_hmac_free(context);
        success
    }
}

#[test]
fn algorithm_values_match_c_consumption() {
    for (safe, raw) in [
        (HmacType::MD5, ffi::AVHMACType::AV_HMAC_MD5),
        (HmacType::SHA1, ffi::AVHMACType::AV_HMAC_SHA1),
        (HmacType::SHA224, ffi::AVHMACType::AV_HMAC_SHA224),
        (HmacType::SHA256, ffi::AVHMACType::AV_HMAC_SHA256),
        (HmacType::SHA384, ffi::AVHMACType::AV_HMAC_SHA384),
        (HmacType::SHA512, ffi::AVHMACType::AV_HMAC_SHA512),
    ] {
        assert_eq!(safe.into_raw(), raw);
        assert_eq!(c_accepts(safe.into_raw()), c_accepts(raw));
        assert!(c_accepts(safe.into_raw()));
    }
    for value in [6, 100, u32::MAX, 1 << 31] {
        let raw = ffi::AVHMACType(value);
        let safe = HmacType::from_raw(raw);
        assert_eq!(c_accepts(safe.into_raw()), c_accepts(raw));
        assert!(!c_accepts(safe.into_raw()));
    }
}

use ffmpeg::libavutil::hmac::Hmac;

struct Reference(*mut ffi::AVHMAC);
impl Reference {
    fn new(algorithm: ffi::AVHMACType) -> Self {
        // SAFETY: C validates algorithm and allocates a fresh context, owned
        // by this fixture and released through the paired av_hmac_free.
        let ptr = unsafe { ffi::av_hmac_alloc(algorithm) };
        assert!(!ptr.is_null());
        Self(ptr)
    }
}
impl Drop for Reference {
    fn drop(&mut self) {
        // SAFETY: the fixture owns this context and its private hash uniquely;
        // av_hmac_free releases both exactly once.
        unsafe { ffi::av_hmac_free(self.0) }
    }
}

#[test]
fn keyed_construction_and_reinitialization_agree() {
    for (algorithm, digest_len) in [
        (HmacType::MD5, 16),
        (HmacType::SHA1, 20),
        (HmacType::SHA224, 28),
        (HmacType::SHA256, 32),
        (HmacType::SHA384, 48),
        (HmacType::SHA512, 64),
    ] {
        let raw = Reference::new(algorithm.into_raw());
        let mut safe = Hmac::new(algorithm, b"").unwrap();
        for len in [0, 1, 63, 64, 65, 127, 128, 129, 1024] {
            let key = vec![0xA5; len];
            safe.as_mut().init(&key).unwrap();
            let empty = 0u8;
            let source = if key.is_empty() {
                &raw const empty
            } else {
                key.as_ptr()
            };
            // SAFETY: raw is a live context with valid callbacks/hash, and
            // source supplies exactly len readable bytes; C copies or hashes
            // them immediately and retains no input pointer.
            unsafe { ffi::av_hmac_init(raw.0, source, len as u32) };
            drop(key);
            // The independent C context receives the same update sequence.
            for chunk in [b"first".as_slice(), b"", b"second chunk"] {
                let empty = 0u8;
                let source = if chunk.is_empty() {
                    &raw const empty
                } else {
                    chunk.as_ptr()
                };
                // SAFETY: both independent contexts are keyed; source is
                // readable for chunk.len() bytes, and update keeps no pointer.
                unsafe {
                    ffi::av_hmac_update(raw.0, source, chunk.len() as u32);
                }
            }
            for chunk in [b"first".as_slice(), b"", b"second chunk"] {
                safe.as_mut().update(chunk).unwrap();
            }
            let mut a = [0xCC; 80];
            let mut b = [0xCC; 80];
            // SAFETY: both contexts are keyed and live, and both independent
            // output arrays hold 80 writable bytes. The short final is rejected
            // without touching state or output; the following full final fits.
            unsafe {
                assert_eq!(
                    ffi::av_hmac_final(raw.0, a.as_mut_ptr(), digest_len - 1),
                    -22
                );

                assert_eq!(
                    ffi::av_hmac_final(raw.0, a.as_mut_ptr(), 80),
                    digest_len as i32
                );
            }
            assert_eq!(
                safe.as_mut().finalize(&mut b[..digest_len as usize - 1]),
                Err(-22)
            );
            assert_eq!(safe.as_mut().finalize(&mut b), Ok(digest_len as usize));
            assert_eq!(a, b);
            assert_eq!(
                &b[digest_len as usize..],
                &[0xCC; 80][digest_len as usize..]
            );
        }
    }
}

#[test]
fn calculate_and_failed_output_preserve_c_state() {
    for algorithm in [
        HmacType::MD5,
        HmacType::SHA1,
        HmacType::SHA224,
        HmacType::SHA256,
        HmacType::SHA384,
        HmacType::SHA512,
    ] {
        let mut safe = Hmac::new(algorithm, b"initial").unwrap();
        let raw = Reference::new(algorithm.into_raw());
        for key_size in [0, 1, 64, 65, 128, 129, 256] {
            let key = vec![0xC3; key_size + 1];
            let data: Vec<_> = (0..258).map(|i| i as u8).collect();
            for len in [0, 1, 63, 64, 65, 257] {
                for capacity in [0, 1, 15, 16, 20, 28, 32, 48, 64, 80] {
                    let mut actual = vec![0x5A; capacity];
                    let mut expected = actual.clone();
                    let src = &data[1..1 + len];
                    let key = &key[1..];
                    // SAFETY: live valid context; source/key slices are backed by
                    // real allocations even when empty, checked small lengths;
                    // C validates output capacity before accessing its buffer.
                    let result = unsafe {
                        ffi::av_hmac_calc(
                            raw.0,
                            src.as_ptr(),
                            len as u32,
                            key.as_ptr(),
                            key.len() as u32,
                            expected.as_mut_ptr(),
                            capacity as u32,
                        )
                    };
                    let result = if result < 0 {
                        Err(result)
                    } else {
                        Ok(result as usize)
                    };
                    assert_eq!(safe.as_mut().calculate(src, key, &mut actual), result);
                    assert_eq!(actual, expected);
                    // A failing calc still keys and updates. A successful calc
                    // leaves the identical memory-safe post-final state.
                    let mut a = [0xA5; 80];
                    let mut b = a;
                    safe.as_mut().update(b"suffix").unwrap();
                    // SAFETY: live initialized/post-final context; six readable
                    // suffix bytes, 80 writable bytes, no retained pointers.
                    let written = unsafe {
                        ffi::av_hmac_update(raw.0, b"suffix".as_ptr(), 6);
                        ffi::av_hmac_final(raw.0, b.as_mut_ptr(), 80)
                    };
                    assert_eq!(safe.as_mut().finalize(&mut a), Ok(written as usize));
                    assert_eq!(a, b);
                }
            }
        }
    }
}
