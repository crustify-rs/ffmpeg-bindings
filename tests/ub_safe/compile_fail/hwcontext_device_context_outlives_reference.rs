use ffmpeg_bindings::libavutil::libavutil::hwcontext::DeviceReferenceBox;

fn view(device: DeviceReferenceBox) {
    let ctx = device.as_ref().context();
    drop(device);
    let _ = ctx.device_type();
}

fn main() {}
