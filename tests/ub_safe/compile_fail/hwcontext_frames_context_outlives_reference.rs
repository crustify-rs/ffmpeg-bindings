use ffmpeg::libavutil::hwcontext::FramesReferenceBox;

fn view(frames: FramesReferenceBox) {
    let ctx = frames.as_ref().context();
    drop(frames);
    let _ = ctx.format();
}

fn main() {}
