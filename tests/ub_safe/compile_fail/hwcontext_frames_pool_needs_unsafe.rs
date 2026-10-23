use ffmpeg_bindings::libavutil::libavutil::hwcontext::FramesReferenceBox;

fn surfaces_as_bytes(frames: FramesReferenceBox) {
    let _ = frames.as_ref().context().pool().map(|p| p.get());
}

fn main() {}
