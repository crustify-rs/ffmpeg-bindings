// A descriptor's component run cannot outlive the box holding it.
use ffmpeg::libavutil::mem::HeapBox;
use ffmpeg::libavutil::pixdesc::PixelFormatDescriptor;

fn main() {
    let b = HeapBox::<PixelFormatDescriptor>::new_zeroed().unwrap();
    let run = b.as_ref().components();
    drop(b);
    println!("{}", run.get(0).unwrap().depth());
}
