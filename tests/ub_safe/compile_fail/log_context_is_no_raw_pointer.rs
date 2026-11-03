// A logging context is a `LogContext`, which only a wrapped `AVClass`-enabled
// object or a log record hands out safely: a raw pointer is not one.
fn main() {
    let ctx = core::ptr::null_mut::<core::ffi::c_void>();
    ffmpeg_bindings::libavutil::libavutil::log::log_with_context(ctx, ffmpeg_bindings::libavutil::libavutil::log::Level::INFO, c"x\n");
}
