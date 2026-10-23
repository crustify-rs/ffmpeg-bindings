// A shared handle cannot outlive the `Alias64` it borrows.
use ffmpeg_bindings::libavutil::libavutil::intreadwrite_h::Alias64;

fn main() {
    let r;
    {
        let a = Alias64::from_u64(1);
        r = a.as_ref();
    }
    println!("{}", r.u64());
}
