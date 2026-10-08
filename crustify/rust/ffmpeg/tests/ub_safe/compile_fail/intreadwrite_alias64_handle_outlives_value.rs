// A shared handle cannot outlive the `Alias64` it borrows.
use ffmpeg::libavutil::intreadwrite::Alias64;

fn main() {
    let r;
    {
        let a = Alias64::from_u64(1);
        r = a.as_ref();
    }
    println!("{}", r.u64());
}
