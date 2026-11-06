// As the safe crate's `build.rs`: embed the linked libavutil's directory
// as an rpath so `cargo run` finds the prebuilt variant the `-sys` crate linked.
fn main() {
    println!("cargo:rustc-link-arg=-Wl,--disable-new-dtags");
    if let Ok(dir) = std::env::var("DEP_AVUTIL_LIB_DIR") {
        println!("cargo:rustc-link-arg=-Wl,-rpath,{dir}");
    }
}
