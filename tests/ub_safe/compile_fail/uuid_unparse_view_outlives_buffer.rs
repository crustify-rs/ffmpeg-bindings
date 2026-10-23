fn main() {
    let view;
    {
        let mut output = [0; 37];
        view = ffmpeg_bindings::libavutil::libavutil::uuid::unparse(&[0; 16], &mut output);
    }
    println!("{view:?}");
}
