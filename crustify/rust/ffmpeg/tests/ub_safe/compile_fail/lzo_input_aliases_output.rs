fn main() {
    let mut bytes = [0; 24];
    let _ = ffmpeg::libavutil::lzo::decode(&bytes, &mut bytes);
}
