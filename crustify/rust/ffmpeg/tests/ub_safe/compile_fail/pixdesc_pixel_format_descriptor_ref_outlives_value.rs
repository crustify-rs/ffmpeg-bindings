// A shared handle cannot outlive the descriptor value it borrows.
use ffmpeg::libavutil::pixdesc::PixelFormatDescriptor;

fn main() {
    let r = {
        let d = PixelFormatDescriptor::new();
        d.as_ref()
    };
    println!("{}", r.component_count());
}
