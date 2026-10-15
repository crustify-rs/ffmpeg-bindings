// A logging context is an `ObjectRef`, which only a wrapped `AVClass`-enabled
// object hands out safely: a raw pointer is not one.
fn main() {
    let ctx = core::ptr::null_mut::<core::ffi::c_void>();
    ffmpeg::libavutil::log::log_with_context(ctx, ffmpeg::libavutil::log::Level::INFO, c"x\n");
}
