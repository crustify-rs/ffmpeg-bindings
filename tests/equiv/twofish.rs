use ffmpeg::libavutil::{ffi, twofish};

#[test]
fn batch10_size_is_the_c_global() {
    for _ in 0..4 {
        // SAFETY: this global is an immutable const int with static lifetime.
        let raw = unsafe { &ffi::av_twofish_size };
        assert_eq!(twofish::context_size(), raw);
        assert!(std::ptr::eq(twofish::context_size(), raw));
    }
}
