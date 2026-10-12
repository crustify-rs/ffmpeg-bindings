use ffmpeg::libavutil::frame::FrameBox;

fn frames_context(frame: FrameBox) {
    let frames = frame.as_ref().hw_frames_context();
    drop(frame);
    let _ = frames.map(|f| f.get_buffer());
}

fn main() {}
