use ffmpeg::libavutil::{adler32, ffi};

#[test]
fn batch10_stream_sequences_match_c() {
    let data: Vec<_> = (0..4097).map(|i| (i * 41) as u8).collect();
    for initial in [0, 1, 0xabcdef01, u32::MAX] {
        let (mut safe, mut raw) = (initial, initial);
        for len in [0, 1, 4, 7, 8, 9, 184, 185, 186, 1024, 4096] {
            let input = &data[1..1 + len];
            safe = adler32::update(safe, input);
            // SAFETY: C reads only len bytes from this independent input copy.
            let owned = input.to_vec();
            // SAFETY: the input is valid for the stated length; no retention.
            raw = unsafe { ffi::av_adler32_update(raw, owned.as_ptr(), owned.len()) };
            assert_eq!(safe, raw);
        }
    }
}
