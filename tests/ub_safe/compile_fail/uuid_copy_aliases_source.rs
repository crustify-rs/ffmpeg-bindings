fn main() {
    let mut uuid = [0; 16];
    ffmpeg::libavutil::uuid::copy(&mut uuid, &uuid);
}
