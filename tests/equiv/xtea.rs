//! Compare lifecycle and field transitions against independently owned C
//! contexts. Raw cipher calls only observe the type's key storage; the cipher
//! symbols themselves are not part of this type batch's safe surface.

use core::ptr::addr_of_mut;

use ffmpeg::libavutil::ffi;
use ffmpeg::libavutil::xtea::{Xtea, XteaBox};

const KEYS: [[u8; 16]; 4] = [
    [0; 16],
    [255; 16],
    [
        0, 1, 2, 3, 4, 5, 6, 7, 128, 255, 254, 253, 252, 251, 250, 249,
    ],
    [128, 0, 0, 1, 255, 0, 128, 0, 0, 128, 1, 0, 128, 0, 255, 0],
];

#[test]
fn allocation_and_reinitialization_preserve_the_full_public_array() {
    let reference = XteaBox::new().unwrap();
    let mut wrapped = XteaBox::new().unwrap();
    assert_eq!(wrapped.as_ref().key(), reference.as_ref().key());
    assert_eq!(wrapped.as_ref().key(), [0; 16]);
    let words = core::array::from_fn(|i| (i as u32).wrapping_mul(0x12345678));
    wrapped.as_mut().set_key(words);
    // SAFETY: reference is sole owner of live C storage; projection writes
    // its entire plain-data array without forming a reference to C memory.
    unsafe { addr_of_mut!((*reference.as_c_ptr()).key).write(words) };
    assert_eq!(wrapped.as_ref().key(), reference.as_ref().key());
    for key in KEYS {
        // SAFETY: independent live, exclusively accessed context and exactly
        // 16 readable key bytes; av_xtea_init retains neither pointer.
        unsafe { ffi::av_xtea_init(reference.as_c_ptr(), key.as_ptr()) };
        wrapped.as_mut().init(&key);
        assert_eq!(wrapped.as_ref().key(), reference.as_ref().key());
        assert_eq!(wrapped.as_ref().key()[4..], words[4..]);
        assert_eq!(
            Xtea::new(&key).as_ref().key()[..4],
            reference.as_ref().key()[..4]
        );
        // SAFETY: the same valid context and input contracts as above.
        unsafe { ffi::av_xtea_le_init(reference.as_c_ptr(), key.as_ptr()) };
        wrapped.as_mut().init_le(&key);
        assert_eq!(wrapped.as_ref().key(), reference.as_ref().key());
        assert_eq!(wrapped.as_ref().key()[4..], words[4..]);
        assert_eq!(
            Xtea::new_le(&key).as_ref().key()[..4],
            reference.as_ref().key()[..4]
        );
    }
    let cloned = wrapped.clone();
    drop(wrapped);
    assert_eq!(cloned.as_ref().key(), reference.as_ref().key());
}

/// # Safety
/// `ctx` must be a live initialized context, exclusively available during
/// this call. Each cipher only reads its first four words and retains nothing.
unsafe fn observe(ctx: *mut ffi::AVXTEA, le: bool, cbc: bool) -> ([u8; 24], [u8; 8]) {
    let crypt = if le {
        ffi::av_xtea_le_crypt
    } else {
        ffi::av_xtea_crypt
    };
    let src = *b"abcdefgh01234567ABCDEFGH";
    let mut dst = [0; 24];
    let mut iv = *b"initial!";
    let iv_ptr = if cbc {
        iv.as_mut_ptr()
    } else {
        core::ptr::null_mut()
    };
    // SAFETY: caller guarantees ctx; src/dst contain three nonoverlapping
    // 8-byte blocks, iv is NULL or eight writable bytes, count is nonnegative.
    unsafe { crypt(ctx, dst.as_mut_ptr(), src.as_ptr(), 3, iv_ptr, 0) };
    let encrypted = dst;
    let final_iv = iv;
    iv = *b"initial!";
    let iv_ptr = if cbc {
        iv.as_mut_ptr()
    } else {
        core::ptr::null_mut()
    };
    // SAFETY: same lengths as above; exact in-place overlap is supported by
    // C, including CBC decryption, which saves the ciphertext before writing.
    unsafe { crypt(ctx, dst.as_mut_ptr(), dst.as_ptr(), 3, iv_ptr, 1) };
    assert_eq!(dst, src);
    // SAFETY: count zero touches neither buffers nor iv; all are still valid.
    unsafe { crypt(ctx, dst.as_mut_ptr(), src.as_ptr(), 0, iv_ptr, 0) };
    assert_eq!(dst, src);
    (encrypted, final_iv)
}

#[test]
fn c_cipher_observes_setters_and_initialization_identically() {
    for key in KEYS {
        for le in [false, true] {
            let mut wrapped = XteaBox::new().unwrap();
            let mut reference = ffi::AVXTEA { key: [0; 16] };
            if le {
                wrapped.as_mut().init_le(&key);
                // SAFETY: initialized local context and 16-byte readable key;
                // neither pointer is retained by C.
                unsafe { ffi::av_xtea_le_init(&raw mut reference, key.as_ptr()) };
            } else {
                wrapped.as_mut().init(&key);
                // SAFETY: same context and input extent as above.
                unsafe { ffi::av_xtea_init(&raw mut reference, key.as_ptr()) };
            }
            for words in [reference.key, [u32::MAX; 16], [0x80000000; 16]] {
                wrapped.as_mut().set_key(words);
                reference.key = words;
                assert_eq!(wrapped.as_ref().key(), reference.key);
                for cbc in [false, true] {
                    // SAFETY: these independently owned initialized contexts
                    // are exclusively available; observe retains no pointers.
                    let (a, b) = unsafe {
                        (
                            observe(wrapped.as_c_ptr(), le, cbc),
                            observe(&raw mut reference, le, cbc),
                        )
                    };
                    assert_eq!(a, b);
                    assert_eq!(wrapped.as_ref().key(), reference.key);
                }
            }
        }
    }
}
