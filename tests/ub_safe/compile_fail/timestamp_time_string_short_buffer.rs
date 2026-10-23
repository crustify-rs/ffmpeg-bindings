// `make_time_string` writes up to `AV_TS_MAX_STRING_SIZE` bytes: a shorter
// buffer, or a slice of unknown length, is rejected.
use ffmpeg_bindings::libavutil::libavutil::{rational::Rational, timestamp::make_time_string};

fn main() {
    let tb = Rational::new(1, 1);
    let mut short = [0u8; 31];
    make_time_string(&mut short, 1, tb);
    let mut long = [0u8; 64];
    make_time_string(&mut long[..], 1, tb);
}
