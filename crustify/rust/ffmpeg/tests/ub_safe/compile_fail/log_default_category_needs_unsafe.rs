// `default_category` reads through a raw context pointer the caller vouches
// for, so calling it takes `unsafe`.
fn main() {
    let ctx = core::ptr::null::<core::ffi::c_void>();
    let _ = ffmpeg::libavutil::log::default_category(ctx);
}
