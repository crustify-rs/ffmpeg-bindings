include!("../sys-build.rs");

fn main() {
    build("libavformat", &["libavcodec", "libavutil"]);
}
