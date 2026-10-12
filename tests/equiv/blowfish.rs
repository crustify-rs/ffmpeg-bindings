use core::ptr::{addr_of, addr_of_mut};
use ffmpeg::libavutil::{blowfish::Blowfish, ffi};

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn lifecycle_and_field_sequences_match_c() {
    let mut safe = Blowfish::new_boxed(b"initial").unwrap();
    let mut reference = ffi::AVBlowfish::default();
    for len in [1, 8, 24, 56, 72, 128] {
        let key: Vec<u8> = (0..len).map(|i| (i * 37) as u8).collect();
        // SAFETY: full writable inline context and len readable key bytes;
        // the key is nonempty and its length fits int. Neither is retained.
        unsafe { ffi::av_blowfish_init(&mut reference, key.as_ptr(), len) };
        safe.as_mut().init(&key).unwrap();
        let inline = Blowfish::new(&key).unwrap();
        assert_eq!(safe.as_ref().p(), reference.p);
        assert_eq!(safe.as_ref().s(), reference.s);
        assert_eq!(inline.as_ref().p(), reference.p);
        assert_eq!(inline.as_ref().s(), reference.s);
        for decrypt in [0, 1] {
            let (mut a, mut b) = (0x12345678, 0x90abcdef);
            let (mut c, mut d) = (a, b);
            // SAFETY: each context is fully initialized and each word is a
            // separate writable local; crypt retains no pointers.
            unsafe {
                ffi::av_blowfish_crypt_ecb(&mut reference, &mut a, &mut b, decrypt);
                let mut halves = [c, d];
                safe.as_ref().crypt_ecb(
                    &mut halves,
                    if decrypt != 0 {
                        Direction::Decrypt
                    } else {
                        Direction::Encrypt
                    },
                );
                [c, d] = halves;
            }
            assert_eq!((a, b), (c, d));
        }
        let p = core::array::from_fn(|i| (i as u32).wrapping_mul(0x12345678));
        let s = core::array::from_fn(|i| core::array::from_fn(|j| (i * 256 + j) as u32));
        safe.as_mut().set_p(p);
        safe.as_mut().set_s(s);
        // SAFETY: the inline reference context is exclusively owned; raw
        // projection writes the entire valid arrays without forming references.
        unsafe {
            addr_of_mut!(reference.p).write(p);
            addr_of_mut!(reference.s).write(s);
        }
        assert_eq!(safe.as_ref().p(), reference.p);
        assert_eq!(safe.as_ref().s(), reference.s);
        // SAFETY: the box's pointer addresses a live initialized AVBlowfish;
        // no mutable handle or foreign writer is live during these reads.
        unsafe {
            assert_eq!(addr_of!((*safe.as_c_ptr()).p).read(), p);
            assert_eq!(addr_of!((*safe.as_c_ptr()).s).read(), s);
        }
    }
}

use ffmpeg::libavutil::aes::Direction;

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn buffer_sequences_match_c() {
    for key_len in [1, 8, 56] {
        for direction in [Direction::Encrypt, Direction::Decrypt] {
            let key = vec![0xa7; key_len];
            let actual = Blowfish::new(&key).unwrap();
            let reference = Blowfish::new_boxed(&key).unwrap();
            let src: Vec<u8> = (0..8 * 8 + 3).map(|i| (i * 31) as u8).collect();
            for cbc in [false, true] {
                let (mut ia, mut ir) = ([0x73; 8], [0x73; 8]);
                for blocks in [0, 1, 2, 4, 8, 1] {
                    let len = blocks * 8;
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
                        ffi::av_blowfish_crypt(
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
                        ffi::av_blowfish_crypt(
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
            let (mut a, mut r) = ([0x12345678, 0xabcdef01], [0x12345678, 0xabcdef01]);
            for _ in 0..3 {
                actual.as_ref().crypt_ecb(&mut a, direction);
                let ptr = r.as_mut_ptr();
                // SAFETY: independent initialized context and two distinct
                // aligned writable u32 words; the routine retains no pointer.
                unsafe {
                    ffi::av_blowfish_crypt_ecb(
                        reference.as_c_ptr(),
                        ptr,
                        ptr.add(1),
                        i32::from(direction == Direction::Decrypt),
                    )
                };
                assert_eq!(a, r);
            }
        }
    }
}
