// A `Rounding` holds only values `av_rescale_rnd` accepts: its field is
// private, so an arbitrary `AVRounding` cannot be wrapped without the check.
use ffmpeg_bindings::libavutil::libavutil::ffi;
use ffmpeg_bindings::libavutil::libavutil::mathematics::Rounding;

fn main() {
    let _ = Rounding(ffi::AV_ROUND_ZERO | 4);
}
