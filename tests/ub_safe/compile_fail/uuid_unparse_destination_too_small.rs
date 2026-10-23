fn main() {
    let mut output = [0; 36];
    let _ = ffmpeg_bindings::libavutil::libavutil::uuid::unparse(&[0; 16], &mut output);
}
