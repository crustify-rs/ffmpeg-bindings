include!("../sys-build.rs");

fn main() {
    build("libswresample", &["libavutil"]);
}
