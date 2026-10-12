use ffmpeg::libavutil::hwcontext::FramesConstraints;
use ffmpeg::libavutil::pixfmt::PixelFormat;

fn main() {
    let mut constraints = FramesConstraints::new().unwrap();
    let formats = constraints.as_ref().valid_sw_formats();
    constraints.as_mut().set_valid_sw_formats(Some(&[PixelFormat::NV12])).unwrap();
    let _ = formats.map(|l| l.len());
}
