// A shared handle cannot outlive the side data descriptor value it borrows.
use ffmpeg::libavutil::frame::SideDataDescriptor;

fn main() {
    let r = {
        let d = SideDataDescriptor::new();
        d.as_ref()
    };
    println!("{:?}", r.props());
}
