// A plane cannot be copied within one buffer: C's row copies (`memcpy`)
// must not overlap, so the destination's exclusive borrow excludes the
// source's.
use ffmpeg_bindings::libavutil::libavutil::imgutils::copy_plane;

fn main() {
    let mut buf = vec![0u8; 64];
    let _ = copy_plane(&mut buf[..32], 8, &buf[16..], 8, 8, 2);
}
