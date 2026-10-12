use std::cell::RefCell;

use ffmpeg::libavutil::hwcontext::{DeviceContextMut, DeviceReference, DeviceType};

thread_local! {
    static KEPT: RefCell<Option<DeviceContextMut<'static>>> = const { RefCell::new(None) };
}

fn main() {
    let mut device = DeviceReference::alloc(DeviceType::VAAPI).unwrap();
    let _ = device
        .context_mut()
        .set_free_callback(|ctx: DeviceContextMut<'_>| KEPT.with(|k| *k.borrow_mut() = Some(ctx)));
}
