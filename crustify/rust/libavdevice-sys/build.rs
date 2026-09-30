include!("../sys-build.rs");

fn main() {
    build("libavdevice", &["libavfilter", "libavformat", "libavcodec", "libavutil"]);
}
