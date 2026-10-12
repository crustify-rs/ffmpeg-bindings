use ffmpeg::libavutil::{
    cast5::{Cast5, Cast5Box, InitError},
    ffi,
};

// Raw C calls below are independent reference construction or observations
// of the safe Rust lifecycle, not tests of an unsafe public Rust API.
fn c_context(key: &[u8]) -> Cast5Box {
    // SAFETY: C returns a fresh av_mallocz allocation; Cast5Free releases it
    // once. The reference key's length is 5..=16 in every caller below.
    let owner = unsafe { Cast5Box::from_c(ffi::av_cast5_alloc()) }.unwrap();
    // SAFETY: the context has its full C allocation and key.len() readable
    // bytes; init copies them and retains no pointers.
    let bits = (key.len() * 8) as i32;
    // SAFETY: key has exactly bits/8 bytes and owner is a full C allocation.
    let result = unsafe { ffi::av_cast5_init(owner.as_c_ptr(), key.as_ptr(), bits) };
    assert_eq!(result, 0);
    owner
}

fn observe(owner: &mut Cast5Box, input: &[u8; 16], decrypt: bool) -> [u8; 16] {
    let mut output = [0; 16];
    // SAFETY: the owner is initialized and exclusively borrowed; both arrays
    // contain two complete 8-byte blocks and do not overlap. C keeps no
    // pointers and writes exactly the output array.
    unsafe {
        ffi::av_cast5_crypt(
            owner.as_c_ptr(),
            output.as_mut_ptr(),
            input.as_ptr(),
            2,
            i32::from(decrypt),
        )
    };
    output
}

fn observe_safe(owner: &Cast5Box, input: &[u8; 16], decrypt: bool) -> [u8; 16] {
    let mut output = [0; 16];
    owner
        .as_ref()
        .crypt(
            &mut output,
            input,
            if decrypt {
                Direction::Decrypt
            } else {
                Direction::Encrypt
            },
        )
        .unwrap();
    output
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn constructors_and_reinitialization_match_independent_c_contexts() {
    let input = *b"0123456789abcdef";
    for initial_len in 5..=16 {
        let key = vec![initial_len as u8; initial_len];
        let mut rust = Cast5::new(&key).unwrap();
        let mut c = c_context(&key);
        for len in 5..=16 {
            let encrypted = observe_safe(&rust, &input, false);
            assert_eq!(encrypted, observe(&mut c, &input, false));
            assert_eq!(observe_safe(&rust, &encrypted, true), input);
            assert_eq!(observe(&mut c, &encrypted, true), input);
            let key = vec![(len * 7) as u8; len];
            rust.as_mut().init(&key).unwrap();
            // SAFETY: c owns a full context, key has len initialized bytes;
            // C only replaces round keys, retaining no pointer.
            let bits = (len * 8) as i32;
            // SAFETY: key has bits/8 bytes and c is exclusively owned.
            let result = unsafe { ffi::av_cast5_init(c.as_c_ptr(), key.as_ptr(), bits) };
            assert_eq!(result, 0);
            assert_eq!(
                observe_safe(&rust, &input, false),
                observe(&mut c, &input, false)
            );
        }
        for len in [0, 4, 17] {
            let invalid_key = vec![0; len];
            assert_eq!(
                rust.as_mut().init(&invalid_key),
                Err(InitError::InvalidKeyLength)
            );
            // SAFETY: invalid bit lengths are rejected before C reads the
            // key or writes the full, live context, even for an empty slice.
            let ptr = invalid_key.as_ptr();
            let bits = (len * 8) as i32;
            // SAFETY: invalid bits are rejected before any pointer access.
            let result = unsafe { ffi::av_cast5_init(c.as_c_ptr(), ptr, bits) };
            assert!(result < 0);
            assert_eq!(
                observe_safe(&rust, &input, false),
                observe(&mut c, &input, false)
            );
        }
    }
}

use ffmpeg::libavutil::cast5;

#[test]
#[cfg_attr(miri, ignore = "reads a C global")]
fn batch10_size_is_the_c_global() {
    for _ in 0..4 {
        // SAFETY: this global is an immutable const int with static lifetime.
        let raw = unsafe { &ffi::av_cast5_size };
        assert_eq!(cast5::context_size(), raw);
        assert!(std::ptr::eq(cast5::context_size(), raw));
    }
}

use ffmpeg::libavutil::aes::Direction;

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn buffer_sequences_match_c() {
    for key_len in [5, 10, 11, 16] {
        for direction in [Direction::Encrypt, Direction::Decrypt] {
            let key = vec![0xa7; key_len];
            let actual = Cast5::new(&key).unwrap();
            let reference = Cast5::new(&key).unwrap();
            let src: Vec<u8> = (0..8 * 8 + 3).map(|i| (i * 31) as u8).collect();
            for cbc in [false, true] {
                let (mut ia, mut ir) = ([0x73; 8], [0x73; 8]);
                for blocks in [0, 1, 2, 4, 8, 1] {
                    let len = blocks * 8;
                    let (mut a, mut r) = (vec![0xcc; len + 3], vec![0xcc; len + 3]);
                    actual
                        .as_ref()
                        .crypt_cbc(
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
                        ffi::av_cast5_crypt2(
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
                        .crypt_cbc_in_place(&mut a[1..1 + len], cbc.then_some(&mut ia), direction)
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
                        ffi::av_cast5_crypt2(
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
            for blocks in [0, 1, 8] {
                let len = blocks * 8;
                let (mut a, mut r) = (vec![1; len], vec![1; len]);
                actual
                    .as_ref()
                    .crypt(&mut a, &src[..len], direction)
                    .unwrap();
                // SAFETY: independent initialized context, disjoint buffers of
                // blocks complete blocks, supported direction, no retained pointer.
                unsafe {
                    ffi::av_cast5_crypt(
                        reference.as_c_ptr(),
                        r.as_mut_ptr(),
                        src.as_ptr(),
                        blocks as i32,
                        i32::from(direction == Direction::Decrypt),
                    )
                };
                assert_eq!(a, r);
                actual.as_ref().crypt_in_place(&mut a, direction).unwrap();
                // SAFETY: independent initialized context, exact-alias buffer of
                // blocks complete blocks; C explicitly supports in-place processing.
                unsafe {
                    ffi::av_cast5_crypt(
                        reference.as_c_ptr(),
                        r.as_mut_ptr(),
                        r.as_mut_ptr(),
                        blocks as i32,
                        i32::from(direction == Direction::Decrypt),
                    )
                };
                assert_eq!(a, r);
            }
        }
    }
}
