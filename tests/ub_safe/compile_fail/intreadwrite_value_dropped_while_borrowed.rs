// A handle cannot outlive the `Unaligned64` value it borrows.
use ffmpeg_bindings::libavutil::libavutil::intreadwrite::{Unaligned64, Unaligned64Mut};

fn main() {
    let m: Unaligned64Mut<'_>;
    {
        let mut v = Unaligned64::new(1);
        m = v.as_mut();
    }
    let _ = m;
}
