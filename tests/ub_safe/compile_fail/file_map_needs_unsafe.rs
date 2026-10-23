// Mapping a file is `unsafe`: safe code cannot hold a borrow other writers
// could change.
fn main() {
    let _ = ffmpeg_bindings::libavutil::libavutil::file::map(c"/dev/null", 0, None);
}
