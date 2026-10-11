use ffmpeg::libavutil::{md5::Md5Box, murmur3::Murmur3Box};
fn main() {
    // Opaque bindgen types have no concrete body. Policies must not offer
    // Rust-sized generic allocation, which would underallocate C's context.
    let _ = Md5Box::new_uninit();
    let _ = Murmur3Box::new_uninit();
}
