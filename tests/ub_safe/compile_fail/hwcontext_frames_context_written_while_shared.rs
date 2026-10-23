use ffmpeg_bindings::libavutil::libavutil::hwcontext::UninitFrames;

fn write(mut frames: UninitFrames) {
    let shared = frames.context();
    frames.context_mut().set_width(16);
    let _ = shared.width();
}

fn main() {}
