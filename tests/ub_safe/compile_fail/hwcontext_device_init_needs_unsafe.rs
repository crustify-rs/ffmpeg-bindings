use ffmpeg_bindings::libavutil::libavutil::hwcontext::{DeviceReference, DeviceType};

fn main() {
    let device = DeviceReference::alloc(DeviceType::VAAPI).unwrap();
    let _ = device.init();
}
