//! Compare lifecycle state through C observers; processing wrappers are scheduled
//! separately. Neither observer adopts the safe owner's allocation.
use ffmpeg::libavutil::{
    aes::{Aes, Direction},
    ffi,
};

struct Reference(*mut ffi::AVAES);
impl Reference {
    fn new() -> Self {
        // SAFETY: allocation has no preconditions; the test releases its result once.
        let ptr = unsafe { ffi::av_aes_alloc() };
        assert!(!ptr.is_null());
        Self(ptr)
    }
}
impl Drop for Reference {
    fn drop(&mut self) {
        // SAFETY: this reference fixture solely owns the live av_malloc allocation.
        unsafe { ffi::av_free(self.0.cast()) }
    }
}

/// Observe an initialized, exclusively accessed C context.
/// # Safety
/// ptr must address a full initialized AES context; no other handle is used here.
unsafe fn observe(ptr: *mut ffi::AVAES, direction: Direction) -> ([u8; 64], [u8; 16]) {
    let input = [0x45; 64];
    let mut output = [0; 64];
    let mut iv = [0x31; 16];
    let decrypt = i32::from(direction == Direction::Decrypt);
    for offset in [0, 32] {
        // SAFETY: caller provides a live initialized context. These disjoint
        // local buffers each have 32 bytes at offset; IV is writable for 16 bytes.
        unsafe {
            ffi::av_aes_crypt(
                ptr,
                output.as_mut_ptr().add(offset),
                input.as_ptr().add(offset),
                2,
                iv.as_mut_ptr(),
                decrypt,
            )
        };
    }
    (output, iv)
}

#[test]
fn constructors_and_reinitialization_match_c() {
    let reference = Reference::new();
    for direction in [Direction::Encrypt, Direction::Decrypt] {
        for len in [16, 24, 32] {
            let mut key = vec![0x91; len];
            let mut owner = Aes::new(&key, direction).unwrap();
            assert_eq!(
                // SAFETY: reference owns full C storage; key has exactly len valid bytes.
                unsafe {
                    ffi::av_aes_init(
                        reference.0,
                        key.as_ptr(),
                        (len * 8) as i32,
                        i32::from(direction == Direction::Decrypt),
                    )
                },
                0
            );
            key.fill(0); // both initializers copied the original key.
            // SAFETY: both independently owned contexts have been initialized;
            // no borrowed handle remains alive across their exclusive C observers.
            assert_eq!(unsafe { observe(owner.as_c_ptr(), direction) }, unsafe {
                observe(reference.0, direction)
            });
            for next_len in [32, 24, 16] {
                let key = vec![next_len as u8; next_len];
                owner.as_mut().init(&key, direction).unwrap();
                assert_eq!(
                    // SAFETY: full reference storage, valid key length; no retained alias.
                    unsafe {
                        ffi::av_aes_init(
                            reference.0,
                            key.as_ptr(),
                            (next_len * 8) as i32,
                            i32::from(direction == Direction::Decrypt),
                        )
                    },
                    0
                );
                // SAFETY: successful reinitialization restored both key schedules.
                let actual = unsafe { observe(owner.as_c_ptr(), direction) };
                // SAFETY: same reference context validity and exclusive access.
                let expected = unsafe { observe(reference.0, direction) };
                assert_eq!(actual, expected);
            }
        }
    }
}
