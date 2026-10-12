use ffmpeg::libavutil::hwcontext::{DeviceReference, DeviceType};

fn main() {
    let device = DeviceReference::alloc(DeviceType::VAAPI).unwrap();
    let ctx = device.context();
    drop(device);
    let _ = ctx.device_type();
}
