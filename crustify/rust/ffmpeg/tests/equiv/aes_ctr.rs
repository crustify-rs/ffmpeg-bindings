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

#[test]
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
            let actual = unsafe { observe(owner.as_c_ptr(), len) };
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
        let actual = unsafe { observe(owner.as_c_ptr(), 0) };
        // SAFETY: same initialized reference allocation.
        let expected = unsafe { observe(reference.0, 0) };
        assert_eq!(actual, (Vec::new(), [0; 16]));
        assert_eq!(actual, expected);
    }
}
