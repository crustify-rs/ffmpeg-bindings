use ffmpeg_bindings::libavutil::libavutil::hwcontext::{DeviceReference, DeviceType};

fn main() {
    let mut device = DeviceReference::alloc(DeviceType::VAAPI).unwrap();
    device.context_mut().set_user_data(String::from("data")).unwrap();
    let data = device.context().user_data::<String>();
    drop(device);
    let _ = data.map(|s| s.len());
}
