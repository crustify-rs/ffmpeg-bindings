use ffmpeg::libavutil::hwcontext::UninitFrames;

fn view(frames: UninitFrames) {
    let ctx = frames.context();
    drop(frames);
    let _ = ctx.width();
}

fn main() {}
