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
            // The non-lifecycle symbols are unwrapped C observers of the
            // initialization state. No context handles live across a call.
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
                    ffi::av_hmac_update(safe.as_c_ptr(), source, chunk.len() as u32);
                }
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
                    ffi::av_hmac_final(safe.as_c_ptr(), b.as_mut_ptr(), digest_len - 1),
                    -22
                );
                assert_eq!(
                    ffi::av_hmac_final(raw.0, a.as_mut_ptr(), 80),
                    digest_len as i32
                );
                assert_eq!(
                    ffi::av_hmac_final(safe.as_c_ptr(), b.as_mut_ptr(), 80),
                    digest_len as i32
                );
            }
            assert_eq!(a, b);
            assert_eq!(
                &b[digest_len as usize..],
                &[0xCC; 80][digest_len as usize..]
            );
        }
    }
}
