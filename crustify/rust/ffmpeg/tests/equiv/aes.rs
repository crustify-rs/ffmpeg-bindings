//! Compare public lifecycle and processing APIs against independently owned C state.
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

fn observe_safe(owner: &mut aes::AesBox) -> ([u8; 64], [u8; 16]) {
    let input = [0x45; 64];
    let mut output = [0; 64];
    let mut iv = [0x31; 16];
    for offset in [0, 32] {
        owner
            .as_mut()
            .crypt(
                &mut output[offset..offset + 32],
                &input[offset..offset + 32],
                Some(&mut iv),
            )
            .unwrap();
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
            let actual = observe_safe(&mut owner);
            // SAFETY: independently owned reference has just been initialized.
            let expected = unsafe { observe(reference.0, direction) };
            assert_eq!(actual, expected);
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
                let actual = observe_safe(&mut owner);
                // SAFETY: same reference context validity and exclusive access.
                let expected = unsafe { observe(reference.0, direction) };
                assert_eq!(actual, expected);
            }
        }
    }
}

use ffmpeg::libavutil::aes;

#[test]
fn batch10_size_is_the_c_global() {
    for _ in 0..4 {
        // SAFETY: this global is an immutable const int with static lifetime.
        let raw = unsafe { &ffi::av_aes_size };
        assert_eq!(aes::context_size(), raw);
        assert!(std::ptr::eq(aes::context_size(), raw));
    }
}

#[test]
fn buffer_sequences_match_c() {
    for key_len in [16, 24, 32] {
        for direction in [Direction::Encrypt, Direction::Decrypt] {
            let key = vec![0xa7; key_len];
            let mut actual = Aes::new(&key, direction).unwrap();
            let reference = Aes::new(&key, direction).unwrap();
            let src: Vec<u8> = (0..16 * 8 + 3).map(|i| (i * 31) as u8).collect();
            for cbc in [false, true] {
                let (mut ia, mut ir) = ([0x73; 16], [0x73; 16]);
                for blocks in [0, 1, 2, 4, 8, 1] {
                    let len = blocks * 16;
                    let (mut a, mut r) = (vec![0xcc; len + 3], vec![0xcc; len + 3]);
                    actual
                        .as_mut()
                        .crypt(&mut a[1..1 + len], &src[2..2 + len], cbc.then_some(&mut ia))
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
                        ffi::av_aes_crypt(
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
                        .as_mut()
                        .crypt_in_place(&mut a[1..1 + len], cbc.then_some(&mut ia))
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
                        ffi::av_aes_crypt(
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
