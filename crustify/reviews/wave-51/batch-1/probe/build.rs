fn main() {
    println!("cargo:rustc-link-arg=-Wl,--disable-new-dtags");
    if let Ok(dir) = std::env::var("DEP_AVUTIL_LIB_DIR") {
        println!("cargo:rustc-link-arg=-Wl,-rpath,{dir}");
    }
}
