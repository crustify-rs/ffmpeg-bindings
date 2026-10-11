use ffmpeg::libavutil::{ffi, ripemd};

#[test]
fn batch10_size_is_the_c_global() {
    for _ in 0..4 {
        // SAFETY: this global is an immutable const int with static lifetime.
        let raw = unsafe { &ffi::av_ripemd_size };
        assert_eq!(ripemd::context_size(), raw);
        assert!(std::ptr::eq(ripemd::context_size(), raw));
    }
}
