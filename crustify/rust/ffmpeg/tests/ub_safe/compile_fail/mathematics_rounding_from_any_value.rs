// A `Rounding` holds only values `av_rescale_rnd` accepts: its field is
// private, so an arbitrary `AVRounding` cannot be wrapped without the check.
use ffmpeg::libavutil::ffi;
use ffmpeg::libavutil::mathematics::Rounding;

fn main() {
    let _ = Rounding(ffi::AVRounding(4));
}
