use ffmpeg::libavutil::{ffi, tea};

#[test]
fn batch10_size_is_the_c_global() {
    for _ in 0..4 {
        // SAFETY: this global is an immutable const int with static lifetime.
        let raw = unsafe { &ffi::av_tea_size };
        assert_eq!(tea::context_size(), raw);
        assert!(std::ptr::eq(tea::context_size(), raw));
    }
}
