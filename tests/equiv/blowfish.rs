use core::ptr::{addr_of, addr_of_mut};
use ffmpeg::libavutil::{blowfish::Blowfish, ffi};

#[test]
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
                ffi::av_blowfish_crypt_ecb(safe.as_c_ptr(), &mut c, &mut d, decrypt);
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
