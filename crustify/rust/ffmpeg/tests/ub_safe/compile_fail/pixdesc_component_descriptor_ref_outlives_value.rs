// A shared handle cannot outlive the `ComponentDescriptor` value it borrows.
use ffmpeg::libavutil::pixdesc::{ComponentDescriptor, ComponentDescriptorRef};

fn dangling() -> ComponentDescriptorRef<'static> {
    let d = ComponentDescriptor::new();
    d.as_ref()
}

fn main() {
    println!("{}", dangling().depth());
}
