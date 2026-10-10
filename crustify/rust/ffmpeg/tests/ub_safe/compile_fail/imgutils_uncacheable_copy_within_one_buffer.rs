// As `copy_plane`, the uncacheable copy cannot read and write one buffer.
use ffmpeg::libavutil::imgutils::copy_plane_from_uncacheable;

fn main() {
    let mut buf = vec![0u8; 256];
    let _ = copy_plane_from_uncacheable(&mut buf, 64, &buf, 64, 64, 2);
}
