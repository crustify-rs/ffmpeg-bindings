use ffmpeg_bindings::libavutil::libavutil::refstruct::RefStructOpaque;
fn main() {
    let mut value = RefStructOpaque::default();
    let shared = value.as_ref();
    value.as_mut().set_mut_ptr(std::ptr::null_mut());
    let _ = shared.mut_ptr();
}
