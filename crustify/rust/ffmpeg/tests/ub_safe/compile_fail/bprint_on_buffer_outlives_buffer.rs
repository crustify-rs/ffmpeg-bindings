// A buffer printing into caller storage cannot outlive that storage.
use ffmpeg::libavutil::bprint::BPrint;

fn main() {
    let buf;
    {
        let mut out = [0u8; 16];
        buf = BPrint::for_buffer(&mut out).unwrap();
    }
    println!("{}", buf.as_ref().len());
}
