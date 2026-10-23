use ffmpeg_bindings::libavutil::libavutil::hwcontext::{DeviceReference, DeviceType};

fn main() {
    let mut device = DeviceReference::alloc(DeviceType::VAAPI).unwrap();
    device.context_mut().set_user_data(String::from("data")).unwrap();
    let data = device.context().user_data::<String>();
    device.context_mut().set_user_data(1u8).unwrap();
    let _ = data.map(|s| s.len());
}
