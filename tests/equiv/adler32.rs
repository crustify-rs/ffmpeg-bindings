#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "test inputs are chosen within range, and the casts mirror the C conversions under comparison"
)]
#![expect(
    clippy::unreadable_literal,
    reason = "constants as the C sources and specifications spell them"
)]
use ffmpeg_bindings::libavutil::libavutil::{adler32, ffi};

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn batch10_stream_sequences_match_c() {
    let data: Vec<_> = (0..4097).map(|i| (i * 41) as u8).collect();
    for initial in [0, 1, 0xABCDEF01, u32::MAX] {
        let (mut safe, mut raw) = (initial, initial);
        for len in [0, 1, 4, 7, 8, 9, 184, 185, 186, 1024, 4096] {
            let input = &data[1..=len];
            safe = adler32::update(safe, input);
            let owned = input.to_vec();
            // SAFETY: C reads only `len` bytes of this independent copy and
            // retains no pointer.
            raw = unsafe { ffi::av_adler32_update(raw, owned.as_ptr(), owned.len()) };
            assert_eq!(safe, raw);
        }
    }
}
