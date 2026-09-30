include!("../sys-build.rs");

fn main() {
    build("libavcodec", &["libswresample", "libavutil"]);
}
