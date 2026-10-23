include!("../sys-build.rs");

fn main() {
    build(
        "libavfilter",
        &[
            "libswscale",
            "libavformat",
            "libavcodec",
            "libswresample",
            "libavutil",
        ],
    );
}
