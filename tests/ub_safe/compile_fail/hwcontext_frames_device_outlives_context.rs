use ffmpeg_bindings::libavutil::libavutil::hwcontext::FramesReferenceBox;

fn device(frames: FramesReferenceBox) {
    let device = frames.as_ref().context().device();
    drop(frames);
    let _ = device.alloc_frames();
}

fn main() {}
