fn main() {
    let mut output = [0; 5];
    let view = ffmpeg::libavutil::base64::encode(b"abc", &mut output).unwrap();
    output.fill(0);
    println!("{view:?}");
}
