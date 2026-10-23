//! C observation probes compare independently owned contexts after construction
//! and each safe reinitialization, update sequence and finalization.
use ffmpeg_bindings::libavutil::libavutil::{
    ffi,
    sha512::{Algorithm, Sha512, Sha512Box},
};

const ALGORITHMS: [Algorithm; 4] = [
    Algorithm::Sha512224,
    Algorithm::Sha512256,
    Algorithm::Sha384,
    Algorithm::Sha512,
];

struct Reference(*mut ffi::AVSHA512);

impl Reference {
    fn new() -> Self {
        // SAFETY: the no-argument allocator returns a fresh C allocation.
        let ptr = unsafe { ffi::av_sha512_alloc() };
        assert!(!ptr.is_null());
        Self(ptr)
    }

    fn init(&mut self, algorithm: Algorithm) -> i32 {
        // SAFETY: this test uniquely owns full-sized storage; the enum is a
        // supported size, and init retains no pointer.
        unsafe { ffi::av_sha512_init(self.0, algorithm as i32) }
    }
}

impl Drop for Reference {
    fn drop(&mut self) {
        // SAFETY: this guard holds the unique av_mallocz block and frees it
        // once, without reading it; no Rust owner also adopted the block.
        unsafe { ffi::av_free(self.0.cast()) };
    }
}

// Exercise the public Rust API and the independently owned raw C reference.
fn probe(owner: &mut Sha512Box, reference: &mut Reference, chunks: &[&[u8]]) -> [u8; 64] {
    for chunk in chunks {
        owner.as_mut().update(chunk);
        // A real byte address is used for C memcpy even for an empty update.
        let data = if chunk.is_empty() {
            c"".as_ptr().cast()
        } else {
            chunk.as_ptr()
        };
        // SAFETY: both allocations are independently initialized and uniquely
        // borrowed. C only reads chunk.len() bytes and retains no input pointer.
        unsafe {
            ffi::av_sha512_update(reference.0, data, chunk.len());
        }
    }
    let rust = owner.as_mut().finalize();
    let mut c = [0; 64];
    // SAFETY: both contexts are initialized and exclusively borrowed; each
    // separate output has the largest supported digest size, and is retained
    // by neither C context. Zero suffixes compare output cardinality too.
    unsafe {
        ffi::av_sha512_final(reference.0, c.as_mut_ptr());
    }
    assert_eq!(rust, c);
    rust
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn initialized_and_reinitialized_state_matches_c() {
    let data: Vec<u8> = (0..4097).map(|i| (i * 17) as u8).collect();
    let mut reference = Reference::new();
    let mut owner = Sha512::new(ALGORITHMS[0]).unwrap();
    assert_eq!(reference.init(ALGORITHMS[0]), 0);
    probe(&mut owner, &mut reference, &[b"", b"a", b"bc"]);

    for _ in 0..2 {
        for algorithm in ALGORITHMS {
            for len in [0, 1, 55, 56, 63, 64, 65, 111, 112, 127, 128, 129, 4097] {
                owner.as_mut().init(algorithm);
                assert_eq!(reference.init(algorithm), 0);
                let (first, rest) = data[..len].split_at(len.min(7));
                let result = probe(&mut owner, &mut reference, &[b"", first, rest, b""]);
                assert!(result[algorithm.digest_len()..].iter().all(|&b| b == 0));
            }
        }
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn constructors_choose_the_expected_digest() {
    let mut reference = Reference::new();
    {
        let mut owner = Sha512::new(Algorithm::Sha512224).unwrap();
        assert_eq!(reference.init(Algorithm::Sha512224), 0);
        let result = probe(&mut owner, &mut reference, &[b"a", b"b", b"c"]);
        assert_eq!(
            &result[..28],
            &[
                0x46, 0x34, 0x27, 0x0f, 0x70, 0x7b, 0x6a, 0x54, 0xda, 0xae, 0x75, 0x30, 0x46, 0x08,
                0x42, 0xe2, 0x0e, 0x37, 0xed, 0x26, 0x5c, 0xee, 0xe9, 0xa4, 0x3e, 0x89, 0x24, 0xaa
            ]
        );
    }
    {
        let mut owner = Sha512::new(Algorithm::Sha512256).unwrap();
        assert_eq!(reference.init(Algorithm::Sha512256), 0);
        let result = probe(&mut owner, &mut reference, &[b"a", b"b", b"c"]);
        assert_eq!(
            &result[..32],
            &[
                0x53, 0x04, 0x8e, 0x26, 0x81, 0x94, 0x1e, 0xf9, 0x9b, 0x2e, 0x29, 0xb7, 0x6b, 0x4c,
                0x7d, 0xab, 0xe4, 0xc2, 0xd0, 0xc6, 0x34, 0xfc, 0x6d, 0x46, 0xe0, 0xe2, 0xf1, 0x31,
                0x07, 0xe7, 0xaf, 0x23
            ]
        );
    }
    {
        let mut owner = Sha512::new(Algorithm::Sha384).unwrap();
        assert_eq!(reference.init(Algorithm::Sha384), 0);
        let result = probe(&mut owner, &mut reference, &[b"a", b"b", b"c"]);
        assert_eq!(
            &result[..48],
            &[
                0xcb, 0x00, 0x75, 0x3f, 0x45, 0xa3, 0x5e, 0x8b, 0xb5, 0xa0, 0x3d, 0x69, 0x9a, 0xc6,
                0x50, 0x07, 0x27, 0x2c, 0x32, 0xab, 0x0e, 0xde, 0xd1, 0x63, 0x1a, 0x8b, 0x60, 0x5a,
                0x43, 0xff, 0x5b, 0xed, 0x80, 0x86, 0x07, 0x2b, 0xa1, 0xe7, 0xcc, 0x23, 0x58, 0xba,
                0xec, 0xa1, 0x34, 0xc8, 0x25, 0xa7
            ]
        );
    }
    {
        let mut owner = Sha512::new(Algorithm::Sha512).unwrap();
        assert_eq!(reference.init(Algorithm::Sha512), 0);
        let result = probe(&mut owner, &mut reference, &[b"a", b"b", b"c"]);
        assert_eq!(
            &result[..64],
            &[
                0xdd, 0xaf, 0x35, 0xa1, 0x93, 0x61, 0x7a, 0xba, 0xcc, 0x41, 0x73, 0x49, 0xae, 0x20,
                0x41, 0x31, 0x12, 0xe6, 0xfa, 0x4e, 0x89, 0xa9, 0x7e, 0xa2, 0x0a, 0x9e, 0xee, 0xe6,
                0x4b, 0x55, 0xd3, 0x9a, 0x21, 0x92, 0x99, 0x2a, 0x27, 0x4f, 0xc1, 0xa8, 0x36, 0xba,
                0x3c, 0x23, 0xa3, 0xfe, 0xeb, 0xbd, 0x45, 0x4d, 0x44, 0x23, 0x64, 0x3c, 0xe8, 0x0e,
                0x2a, 0x9a, 0xc9, 0x4f, 0xa5, 0x4c, 0xa4, 0x9f
            ]
        );
    }
    let mut owner = Sha512Box::new().unwrap();
    assert_eq!(reference.init(ALGORITHMS[3]), 0);
    probe(&mut owner, &mut reference, &[b"abc"]);
}

use ffmpeg_bindings::libavutil::libavutil::sha512;

#[test]
#[cfg_attr(miri, ignore = "reads a C global")]
fn batch10_size_is_the_c_global() {
    for _ in 0..4 {
        // SAFETY: this global is an immutable const int with static lifetime.
        let raw = unsafe { &ffi::av_sha512_size };
        assert_eq!(sha512::context_size(), raw);
        assert!(std::ptr::eq(sha512::context_size(), raw));
    }
}
