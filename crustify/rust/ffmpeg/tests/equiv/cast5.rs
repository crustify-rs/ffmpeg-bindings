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

#[test]
fn constructors_and_reinitialization_match_independent_c_contexts() {
    let input = *b"0123456789abcdef";
    for initial_len in 5..=16 {
        let key = vec![initial_len as u8; initial_len];
        let mut rust = Cast5::new(&key).unwrap();
        let mut c = c_context(&key);
        for len in 5..=16 {
            let encrypted = observe(&mut rust, &input, false);
            assert_eq!(encrypted, observe(&mut c, &input, false));
            assert_eq!(observe(&mut rust, &encrypted, true), input);
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
                observe(&mut rust, &input, false),
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
                observe(&mut rust, &input, false),
                observe(&mut c, &input, false)
            );
        }
    }
}

use ffmpeg::libavutil::cast5;

#[test]
fn batch10_size_is_the_c_global() {
    for _ in 0..4 {
        // SAFETY: this global is an immutable const int with static lifetime.
        let raw = unsafe { &ffi::av_cast5_size };
        assert_eq!(cast5::context_size(), raw);
        assert!(std::ptr::eq(cast5::context_size(), raw));
    }
}
