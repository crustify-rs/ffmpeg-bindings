fn main() {
    let mut bytes = [0; 24];
    let _ = ffmpeg_bindings::libavutil::libavutil::lzo::decode(&bytes, &mut bytes);
}
