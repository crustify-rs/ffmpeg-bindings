include!("../sys-build.rs");

fn main() {
    build("libswscale", &["libavutil"]);
}
