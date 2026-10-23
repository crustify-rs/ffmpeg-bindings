use ffmpeg_bindings::libavutil::libavutil::hwcontext::UninitFrames;

fn replace(mut frames: UninitFrames) {
    frames.context_mut().set_user_data(String::from("data")).unwrap();
    let data = frames.context().user_data::<String>();
    frames.context_mut().set_user_data(1u8).unwrap();
    let _ = data.map(|s| s.len());
}

fn main() {}
