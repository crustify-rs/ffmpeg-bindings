// Embed each linked FFmpeg library's directory as an rpath so the crate's tests and
// examples run against the prebuilt variant the `-sys` crates linked. DT_RPATH, not
// DT_RUNPATH: it also resolves the FFmpeg libraries' own NEEDED entries.
fn main() {
    println!("cargo:rustc-link-arg=-Wl,--disable-new-dtags");
    for lib in ["AVUTIL", "SWRESAMPLE", "SWSCALE", "AVCODEC", "AVFORMAT", "AVFILTER", "AVDEVICE"] {
        if let Ok(dir) = std::env::var(format!("DEP_{lib}_LIB_DIR")) {
            println!("cargo:rustc-link-arg=-Wl,-rpath,{dir}");
        }
    }
}
