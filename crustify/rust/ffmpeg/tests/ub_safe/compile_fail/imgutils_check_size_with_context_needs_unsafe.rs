// Checking a size with a raw parent log context is `unsafe`: safe code
// cannot vouch for what the pointer addresses.
fn main() {
    let _ = ffmpeg::libavutil::imgutils::check_size_with_context(1, 1, 0, core::ptr::null_mut());
}
