// A `FastBuffer`'s bytes start uninitialised: they read as `MaybeUninit<u8>`.
use ffmpeg_bindings::libavutil::libavutil::mem::FastBuffer;

fn main() {
    let mut b = FastBuffer::new();
    let _ = b.reserve(16);
    let s: &[u8] = b.as_slice();
    println!("{}", s[0]);
}
