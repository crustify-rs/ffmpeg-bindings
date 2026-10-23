use ffmpeg_bindings::libavutil::libavutil::hwcontext::FramesConstraints;

fn main() {
    let constraints = FramesConstraints::new().unwrap();
    let formats = constraints.as_ref().valid_hw_formats();
    drop(constraints);
    let _ = formats.map(|l| l.len());
}
