//! C observation probes compare independently owned contexts after construction
//! and each safe reinitialization, update sequence and finalization.
#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "test inputs are chosen within range, and the casts mirror the C conversions under comparison"
)]
use ffmpeg_bindings::libavutil::libavutil::ffi;
use ffmpeg_bindings::libavutil::libavutil::sha::{Algorithm, Sha, ShaBox};

const ALGORITHMS: [Algorithm; 3] = [Algorithm::Sha1, Algorithm::Sha224, Algorithm::Sha256];

struct Reference(*mut ffi::AVSHA);

impl Reference {
    fn new() -> Self {
        // SAFETY: the no-argument allocator returns a fresh C allocation.
        let ptr = unsafe { ffi::av_sha_alloc() };
        assert!(!ptr.is_null());
        Self(ptr)
    }

    fn init(&mut self, algorithm: Algorithm) -> i32 {
        // SAFETY: this test uniquely owns full-sized storage; the enum is a
        // supported size, and init retains no pointer.
        unsafe { ffi::av_sha_init(self.0, algorithm as i32) }
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
fn probe(owner: &mut ShaBox, reference: &mut Reference, chunks: &[&[u8]]) -> [u8; 32] {
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
            ffi::av_sha_update(reference.0, data, chunk.len());
        }
    }
    let rust = owner.as_mut().finalize();
    let mut c = [0; 32];
    // SAFETY: both contexts are initialized and exclusively borrowed; each
    // separate output has the largest supported digest size, and is retained
    // by neither C context. Zero suffixes compare output cardinality too.
    unsafe {
        ffi::av_sha_final(reference.0, c.as_mut_ptr());
    }
    assert_eq!(rust, c);
    rust
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn initialized_and_reinitialized_state_matches_c() {
    let data: Vec<u8> = (0..4097).map(|i| (i * 17) as u8).collect();
    let mut reference = Reference::new();
    let mut owner = Sha::new(ALGORITHMS[0]).unwrap();
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
        let mut owner = Sha::new(Algorithm::Sha1).unwrap();
        assert_eq!(reference.init(Algorithm::Sha1), 0);
        let result = probe(&mut owner, &mut reference, &[b"a", b"b", b"c"]);
        assert_eq!(
            &result[..20],
            &[
                0xA9, 0x99, 0x3E, 0x36, 0x47, 0x06, 0x81, 0x6A, 0xBA, 0x3E, 0x25, 0x71, 0x78, 0x50,
                0xC2, 0x6C, 0x9C, 0xD0, 0xD8, 0x9D
            ]
        );
    }
    {
        let mut owner = Sha::new(Algorithm::Sha224).unwrap();
        assert_eq!(reference.init(Algorithm::Sha224), 0);
        let result = probe(&mut owner, &mut reference, &[b"a", b"b", b"c"]);
        assert_eq!(
            &result[..28],
            &[
                0x23, 0x09, 0x7D, 0x22, 0x34, 0x05, 0xD8, 0x22, 0x86, 0x42, 0xA4, 0x77, 0xBD, 0xA2,
                0x55, 0xB3, 0x2A, 0xAD, 0xBC, 0xE4, 0xBD, 0xA0, 0xB3, 0xF7, 0xE3, 0x6C, 0x9D, 0xA7
            ]
        );
    }
    {
        let mut owner = Sha::new(Algorithm::Sha256).unwrap();
        assert_eq!(reference.init(Algorithm::Sha256), 0);
        let result = probe(&mut owner, &mut reference, &[b"a", b"b", b"c"]);
        assert_eq!(
            &result[..32],
            &[
                0xBA, 0x78, 0x16, 0xBF, 0x8F, 0x01, 0xCF, 0xEA, 0x41, 0x41, 0x40, 0xDE, 0x5D, 0xAE,
                0x22, 0x23, 0xB0, 0x03, 0x61, 0xA3, 0x96, 0x17, 0x7A, 0x9C, 0xB4, 0x10, 0xFF, 0x61,
                0xF2, 0x00, 0x15, 0xAD
            ]
        );
    }
    let mut owner = ShaBox::new().unwrap();
    assert_eq!(reference.init(ALGORITHMS[2]), 0);
    probe(&mut owner, &mut reference, &[b"abc"]);
}

use ffmpeg_bindings::libavutil::libavutil::sha;

#[test]
#[cfg_attr(miri, ignore = "reads a C global")]
fn batch10_size_is_the_c_global() {
    for _ in 0..4 {
        // SAFETY: this global is an immutable const int with static lifetime.
        let raw = unsafe { &ffi::av_sha_size };
        assert_eq!(sha::context_size(), raw);
        assert!(std::ptr::eq(sha::context_size(), raw));
    }
}
