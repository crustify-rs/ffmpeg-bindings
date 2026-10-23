fn main() {
    let mut uuid = [0; 16];
    ffmpeg_bindings::libavutil::libavutil::uuid::copy(&mut uuid, &uuid);
}
