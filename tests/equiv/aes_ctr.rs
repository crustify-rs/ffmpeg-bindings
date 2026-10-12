//! Observe the private lifecycle state through the unchanged C processing API.
use ffmpeg::libavutil::{aes_ctr::AesCtr, ffi};

struct Reference(*mut ffi::AVAESCTR);
impl Reference {
    fn new() -> Self {
        // SAFETY: allocation has no preconditions; the test releases it once.
        let ptr = unsafe { ffi::av_aes_ctr_alloc() };
        assert!(!ptr.is_null());
        Self(ptr)
    }
}
impl Drop for Reference {
    fn drop(&mut self) {
        // SAFETY: this fixture solely owns the C allocation.
        unsafe { ffi::av_aes_ctr_free(self.0) }
    }
}

/// # Safety
/// ptr must be an initialized full CTR context, exclusively accessed for this call.
unsafe fn observe(ptr: *mut ffi::AVAESCTR, len: usize) -> (Vec<u8>, [u8; 16]) {
    let input = vec![0x61; len.max(1)];
    let mut output = vec![0; len.max(1)];
    // SAFETY: caller supplies exclusive initialized context; buffers are valid
    // for len bytes, including a real address on the zero-length path.
    unsafe { ffi::av_aes_ctr_crypt(ptr, output.as_mut_ptr(), input.as_ptr(), len as i32) };
    // SAFETY: get_iv returns the inline, fully initialized counter[16]. Copy it
    // immediately; no reference to C memory survives a subsequent write or free.
    let iv = unsafe { ffi::av_aes_ctr_get_iv(ptr).cast::<[u8; 16]>().read() };
    output.truncate(len);
    (output, iv)
}

fn observe_safe(
    owner: &mut ffmpeg::libavutil::aes_ctr::AesCtrBox,
    len: usize,
) -> (Vec<u8>, [u8; 16]) {
    let input = vec![0x61; len];
    let mut output = vec![0; len];
    owner.as_mut().crypt(&mut output, &input).unwrap();
    (output, owner.as_ref().iv())
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn constructors_and_reset_partial_block_match_c() {
    let reference = Reference::new();
    let mut key = [0x42; 16];
    let mut owner = AesCtr::new(&key).unwrap();
    assert_eq!(
        // SAFETY: full reference storage and exactly 16 key bytes, copied by C.
        unsafe { ffi::av_aes_ctr_init(reference.0, key.as_ptr()) },
        0
    );
    key.fill(0); // initialization must not retain the key.
    for seed in 0..3 {
        for len in [0, 1, 15, 17, 31, 1] {
            // SAFETY: both contexts initialized and exclusively observed.
            let actual = observe_safe(&mut owner, len);
            // SAFETY: independently owned initialized reference context.
            let expected = unsafe { observe(reference.0, len) };
            assert_eq!(actual, expected);
        }
        // Reset after ending on a partial block: next crypt must not use old scratch.
        owner.as_mut().init(&[seed; 16]).unwrap();
        assert_eq!(
            // SAFETY: full reference allocation, exactly 16 key bytes, no retained alias.
            unsafe { ffi::av_aes_ctr_init(reference.0, [seed; 16].as_ptr()) },
            0
        );
        // SAFETY: both have just been reinitialized; counters must be zero.
        let actual = observe_safe(&mut owner, 0);
        // SAFETY: same initialized reference allocation.
        let expected = unsafe { observe(reference.0, 0) };
        assert_eq!(actual, (Vec::new(), [0; 16]));
        assert_eq!(actual, expected);
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn stream_and_iv_transitions_match_c() {
    let mut actual = AesCtr::new(&[0xa7; 16]).unwrap();
    let reference = AesCtr::new(&[0xa7; 16]).unwrap();
    for mode in 0..4 {
        match mode {
            0 => {
                actual.as_mut().set_iv(&[0x73; 8]);
                // SAFETY: independent initialized context and exactly 8 readable
                // nonce bytes. C copies them and retains nothing.
                unsafe { ffi::av_aes_ctr_set_iv(reference.as_c_ptr(), [0x73; 8].as_ptr()) };
            }
            1 => {
                actual.as_mut().set_full_iv(&[0xff; 16]);
                // SAFETY: initialized context and a readable full 16-byte counter.
                unsafe { ffi::av_aes_ctr_set_full_iv(reference.as_c_ptr(), [0xff; 16].as_ptr()) };
            }
            2 => {
                actual.as_mut().increment_iv();
                // SAFETY: this independently owned full initialized context is exclusive.
                unsafe { ffi::av_aes_ctr_increment_iv(reference.as_c_ptr()) };
            }
            _ => {
                actual.as_mut().set_random_iv();
                // Random bytes themselves are nondeterministic. Synchronize the
                // reference to the chosen nonce, then compare reset/stream state.
                let nonce: [u8; 8] = actual.as_ref().iv()[..8].try_into().unwrap();
                // SAFETY: full initialized context, exact copied nonce bytes.
                unsafe { ffi::av_aes_ctr_set_iv(reference.as_c_ptr(), nonce.as_ptr()) };
            }
        }
        for len in [0, 1, 14, 1, 16, 17, 31, 64, 129, 3] {
            let src = [0x5a; 133];
            let (mut a, mut r) = ([0x99; 133], [0x99; 133]);
            actual
                .as_mut()
                .crypt(&mut a[1..1 + len], &src[2..2 + len])
                .unwrap();
            // SAFETY: exclusive independent initialized context; disjoint
            // buffers provide len readable/writable bytes, int-sized count.
            unsafe {
                ffi::av_aes_ctr_crypt(
                    reference.as_c_ptr(),
                    r.as_mut_ptr().add(1),
                    src.as_ptr().add(2),
                    len as i32,
                )
            };
            assert_eq!(a, r);
            // SAFETY: the reference is live and initialized, get_iv returns its
            // inline counter[16]. Snapshot it without retaining a C reference.
            let iv = unsafe {
                ffi::av_aes_ctr_get_iv(reference.as_c_ptr())
                    .cast::<[u8; 16]>()
                    .read()
            };
            assert_eq!(actual.as_ref().iv(), iv);
            actual.as_mut().crypt_in_place(&mut a[1..1 + len]).unwrap();
            // SAFETY: initialized reference and exact-alias writable len-byte
            // buffer supported by C; no caller pointer is retained.
            unsafe {
                ffi::av_aes_ctr_crypt(
                    reference.as_c_ptr(),
                    r.as_mut_ptr().add(1),
                    r.as_mut_ptr().add(1),
                    len as i32,
                )
            };
            assert_eq!(a, r);
            // SAFETY: same live inline counter, copied before any next mutation.
            let iv = unsafe {
                ffi::av_aes_ctr_get_iv(reference.as_c_ptr())
                    .cast::<[u8; 16]>()
                    .read()
            };
            assert_eq!(actual.as_ref().iv(), iv);
        }
    }
}
