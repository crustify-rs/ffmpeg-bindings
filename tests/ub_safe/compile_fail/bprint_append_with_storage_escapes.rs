// The storage `append_with` hands out is the buffer's own, which a later
// append may reallocate: it cannot leave the closure.
use ffmpeg::libavutil::bprint::BPrint;

fn main() {
    let mut b = BPrint::new(0, BPrint::SIZE_UNLIMITED).unwrap();
    let mut kept: &mut [u8] = &mut [];
    b.as_mut().append_with(4, |spare| {
        kept = spare;
        0
    });
    b.as_mut().append_repeated(b'x', 5000);
    kept[0] = 1;
}
