//! C observation probes compare independently owned contexts after construction
//! and each safe reinitialization. Hash update/final are not wrapped by this
//! type batch; they are used here only to observe the lifecycle's C state.
use ffmpeg::libavutil::{
    ffi,
    sha::{Algorithm, Sha, ShaBox},
};

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

// This probe mutates through the owner's raw FFI seam only while it is
// exclusively borrowed and has no outstanding shared or exclusive handles.
fn probe(owner: &mut ShaBox, reference: &mut Reference, chunks: &[&[u8]]) -> [u8; 32] {
    for chunk in chunks {
        // A real byte address is used for C memcpy even for an empty update.
        let data = if chunk.is_empty() {
            b"\0".as_ptr()
        } else {
            chunk.as_ptr()
        };
        // SAFETY: both allocations are independently initialized and uniquely
        // borrowed. C only reads chunk.len() bytes and retains no input pointer.
        unsafe {
            ffi::av_sha_update(owner.as_c_ptr(), data, chunk.len());
            ffi::av_sha_update(reference.0, data, chunk.len());
        }
    }
    let mut rust = [0xa5; 32];
    let mut c = [0xa5; 32];
    // SAFETY: both contexts are initialized and exclusively borrowed; each
    // separate output has the largest supported digest size, and is retained
    // by neither C context. Sentinel suffixes compare output cardinality too.
    unsafe {
        ffi::av_sha_final(owner.as_c_ptr(), rust.as_mut_ptr());
        ffi::av_sha_final(reference.0, c.as_mut_ptr());
    }
    assert_eq!(rust, c);
    rust
}

#[test]
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
                assert!(result[algorithm.digest_len()..].iter().all(|&b| b == 0xa5));
            }
        }
    }
}

#[test]
fn constructors_choose_the_expected_digest() {
    let mut reference = Reference::new();
    {
        let mut owner = Sha::new(Algorithm::Sha1).unwrap();
        assert_eq!(reference.init(Algorithm::Sha1), 0);
        let result = probe(&mut owner, &mut reference, &[b"a", b"b", b"c"]);
        assert_eq!(
            &result[..20],
            &[
                0xa9, 0x99, 0x3e, 0x36, 0x47, 0x06, 0x81, 0x6a, 0xba, 0x3e, 0x25, 0x71, 0x78, 0x50,
                0xc2, 0x6c, 0x9c, 0xd0, 0xd8, 0x9d
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
                0x23, 0x09, 0x7d, 0x22, 0x34, 0x05, 0xd8, 0x22, 0x86, 0x42, 0xa4, 0x77, 0xbd, 0xa2,
                0x55, 0xb3, 0x2a, 0xad, 0xbc, 0xe4, 0xbd, 0xa0, 0xb3, 0xf7, 0xe3, 0x6c, 0x9d, 0xa7
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
                0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea, 0x41, 0x41, 0x40, 0xde, 0x5d, 0xae,
                0x22, 0x23, 0xb0, 0x03, 0x61, 0xa3, 0x96, 0x17, 0x7a, 0x9c, 0xb4, 0x10, 0xff, 0x61,
                0xf2, 0x00, 0x15, 0xad
            ]
        );
    }
    let mut owner = ShaBox::new().unwrap();
    assert_eq!(reference.init(ALGORITHMS[2]), 0);
    probe(&mut owner, &mut reference, &[b"abc"]);
}
