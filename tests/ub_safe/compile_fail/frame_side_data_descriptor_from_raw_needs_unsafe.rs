// Adopting a raw descriptor asserts its name lives for the whole program, so
// it needs `unsafe`.
use ffmpeg::libavutil::ffi;
use ffmpeg::libavutil::frame::SideDataDescriptor;

fn main() {
    let raw = ffi::AVSideDataDescriptor {
        name: std::ptr::null(),
        props: 0,
    };
    let d = SideDataDescriptor::from_raw(raw);
    println!("{d:?}");
}
