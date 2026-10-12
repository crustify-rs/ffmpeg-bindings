//! Compare lifecycle, keys, buffers and CBC transitions on independently
//! owned contexts through the public safe API and the raw C implementation.

use core::ptr::addr_of_mut;

use ffmpeg::libavutil::aes::Direction;
use ffmpeg::libavutil::ffi;
use ffmpeg::libavutil::xtea::{Xtea, XteaBox, XteaRef};

const KEYS: [[u8; 16]; 4] = [
    [0; 16],
    [255; 16],
    [
        0, 1, 2, 3, 4, 5, 6, 7, 128, 255, 254, 253, 252, 251, 250, 249,
    ],
    [128, 0, 0, 1, 255, 0, 128, 0, 0, 128, 1, 0, 128, 0, 255, 0],
];

#[test]
#[cfg_attr(miri, ignore = "calls C")]
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
unsafe fn observe_c(ctx: *mut ffi::AVXTEA, le: bool, cbc: bool) -> ([u8; 24], [u8; 8]) {
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
#[cfg_attr(miri, ignore = "calls C")]
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
                    let src = *b"abcdefgh01234567ABCDEFGH";
                    let mut dst = [0; 24];
                    let mut iv = *b"initial!";
                    wrapped_call(
                        wrapped.as_ref(),
                        &mut dst,
                        Some(&src),
                        cbc.then_some(&mut iv),
                        Direction::Encrypt,
                        le,
                    );
                    let a = (dst, iv);
                    iv = *b"initial!";
                    wrapped_call(
                        wrapped.as_ref(),
                        &mut dst,
                        None,
                        cbc.then_some(&mut iv),
                        Direction::Decrypt,
                        le,
                    );
                    assert_eq!(dst, src);
                    // SAFETY: independent initialized C context, exclusively
                    // available; observe_c uses bounded buffers and retains nothing.
                    let b = unsafe { observe_c(&raw mut reference, le, cbc) };
                    assert_eq!(a, b);
                    assert_eq!(wrapped.as_ref().key(), reference.key);
                }
            }
        }
    }
}

fn wrapped_call(
    ctx: XteaRef<'_>,
    dst: &mut [u8],
    src: Option<&[u8]>,
    iv: Option<&mut [u8; 8]>,
    direction: Direction,
    le: bool,
) {
    match (le, src) {
        (false, Some(src)) => ctx.crypt(dst, src, iv, direction),
        (true, Some(src)) => ctx.crypt_le(dst, src, iv, direction),
        (false, None) => ctx.crypt_in_place(dst, iv, direction),
        (true, None) => ctx.crypt_le_in_place(dst, iv, direction),
    }
    .unwrap();
}

fn reference_call(
    ctx: &XteaBox,
    dst: &mut [u8],
    src: Option<&[u8]>,
    iv: Option<&mut [u8; 8]>,
    direction: Direction,
    le: bool,
) {
    let count = i32::try_from(dst.len() / 8).unwrap();
    assert!(dst.len().is_multiple_of(8));
    assert!(src.is_none_or(|src| src.len() == dst.len()));
    // One exclusive reborrow for both pointers when in place, as the wrapper.
    let dst_ptr = dst.as_mut_ptr();
    let src = src.map_or(dst_ptr.cast_const(), |src| src.as_ptr());
    let iv = iv.map_or(core::ptr::null_mut(), |iv| iv.as_mut_ptr());
    let crypt = if le {
        ffi::av_xtea_le_crypt
    } else {
        ffi::av_xtea_crypt
    };
    // SAFETY: C only reads this live independently owned context; all buffers
    // cover count*8 bytes, with disjoint or exact overlap and a disjoint eight-
    // byte IV (or NULL). Count is nonnegative and no pointer is retained.
    unsafe {
        crypt(
            ctx.as_c_ptr(),
            dst_ptr,
            src,
            count,
            iv,
            i32::from(direction == Direction::Decrypt),
        )
    };
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn cipher_matrix_and_multicall_cbc_state_match_c() {
    for key in KEYS {
        for key_le in [false, true] {
            // Initializer byte order is independent of block byte order.
            let wrapped = if key_le {
                Xtea::new_le(&key)
            } else {
                Xtea::new(&key)
            };
            let mut reference = XteaBox::new().unwrap();
            if key_le {
                // SAFETY: fresh exclusively owned context and sixteen readable
                // key bytes; C copies the key and retains neither pointer.
                unsafe { ffi::av_xtea_le_init(reference.as_c_ptr(), key.as_ptr()) };
            } else {
                // SAFETY: same initialized allocation and key extent as above.
                unsafe { ffi::av_xtea_init(reference.as_c_ptr(), key.as_ptr()) };
            }
            for le in [false, true] {
                for cbc in [false, true] {
                    for inplace in [false, true] {
                        for blocks in [0, 1, 2, 17] {
                            let len = blocks * 8;
                            let input: Vec<u8> = (0..len + 2)
                                .map(|i| (i as u8).wrapping_mul(173).wrapping_add(0x80))
                                .collect();
                            let src = &input[1..len + 1]; // byte-aligned, not word-aligned
                            let mut a = vec![0xa5; len + 2];
                            let mut b = a.clone();
                            if inplace {
                                a[1..len + 1].copy_from_slice(src);
                                b[1..len + 1].copy_from_slice(src);
                            }
                            let initial_iv = *b"initial!";
                            let mut iv_a = initial_iv;
                            let mut iv_b = initial_iv;
                            // Split calls deliberately include an empty call;
                            // compare output, key and IV after every transition.
                            let split = if blocks > 1 { 8 } else { 0 };
                            for range in [1..1, 1..1 + split, 1 + split..len + 1] {
                                let source = (!inplace).then_some(&input[range.clone()]);
                                wrapped_call(
                                    wrapped.as_ref(),
                                    &mut a[range.clone()],
                                    source,
                                    cbc.then_some(&mut iv_a),
                                    Direction::Encrypt,
                                    le,
                                );
                                reference_call(
                                    &reference,
                                    &mut b[range],
                                    source,
                                    cbc.then_some(&mut iv_b),
                                    Direction::Encrypt,
                                    le,
                                );
                                assert_eq!(a, b);
                                assert_eq!(iv_a, iv_b);
                                assert_eq!(wrapped.as_ref().key(), reference.as_ref().key());
                            }
                            let ciphertext = a.clone();
                            iv_a = initial_iv;
                            iv_b = initial_iv;
                            // Decrypt with the opposite aliasing form.
                            for range in [1..1 + split, 1 + split..len + 1, len + 1..len + 1] {
                                let source = inplace.then_some(&ciphertext[range.clone()]);
                                wrapped_call(
                                    wrapped.as_ref(),
                                    &mut a[range.clone()],
                                    source,
                                    cbc.then_some(&mut iv_a),
                                    Direction::Decrypt,
                                    le,
                                );
                                reference_call(
                                    &reference,
                                    &mut b[range],
                                    source,
                                    cbc.then_some(&mut iv_b),
                                    Direction::Decrypt,
                                    le,
                                );
                                assert_eq!(a, b);
                                assert_eq!(iv_a, iv_b);
                                assert_eq!(wrapped.as_ref().key(), reference.as_ref().key());
                            }
                            assert_eq!(&a[1..len + 1], src);
                            assert_eq!(a[0], 0xa5);
                            assert_eq!(a[len + 1], 0xa5);
                            assert_eq!(
                                iv_a,
                                if cbc && len > 0 {
                                    ciphertext[len - 7..len + 1].try_into().unwrap()
                                } else {
                                    initial_iv
                                }
                            );
                        }
                    }
                }
            }
            // Reinitialization still preserves the public unused words.
            reference.as_mut().set_key([u32::MAX; 16]);
            reference.as_mut().init(&key);
            assert_eq!(reference.as_ref().key()[4..], [u32::MAX; 12]);
        }
    }
}
