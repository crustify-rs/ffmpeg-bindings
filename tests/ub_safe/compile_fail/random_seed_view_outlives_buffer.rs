// The initialised view of a filled buffer cannot outlive the buffer.
use core::mem::MaybeUninit;

use ffmpeg_bindings::libavutil::libavutil::random_seed::random_bytes_uninit;

fn main() {
    let bytes = {
        let mut buf = [MaybeUninit::<u8>::uninit(); 16];
        random_bytes_uninit(&mut buf).unwrap()
    };
    println!("{bytes:?}");
}
