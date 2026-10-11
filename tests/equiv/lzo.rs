use ffmpeg::libavutil::{ffi, lzo};
fn samples() -> Vec<Vec<u8>> {
    let mut long = vec![0, 0, 27];
    long.extend((0..300).map(|i| i as u8));
    long.extend([17, 0, 0]);
    fn long_back_reference(len: usize, token: u8) -> Vec<u8> {
        let count = len - 18;
        let (zeros, last) = if count.is_multiple_of(255) {
            (count / 255 - 1, 255)
        } else {
            (count / 255, count % 255)
        };
        let mut input = vec![0];
        input.extend(std::iter::repeat_n(0, zeros));
        input.push(last as u8);
        input.extend((0..len).map(|i| i as u8));
        input.extend([token, 4, 0, 17, 0, 0]);
        input
    }
    vec![
        vec![],
        vec![17, 0, 0],
        vec![20, b'a', b'b', b'c', 17, 0, 0],
        vec![20, b'a', b'b', b'c', 168, 0, 17, 0, 0],
        vec![20, b'a', b'b', b'c', 169, 0, b'd', 0, 0, 17, 0, 0],
        vec![20, b'a', b'b', b'c', 32, 1, 8, 0, 17, 0, 0],
        vec![16, 0, 0],
        vec![255],
        vec![1, b'a', b'b', b'c', b'd', 0, 0],
        vec![64, 0],
        vec![0, 0, 0],
        vec![20, b'a', b'b', b'c', 0],
        long,
        long_back_reference(16385, 17),
        long_back_reference(32769, 25),
    ]
}

#[test]
fn batch10_errors_remaining_lengths_and_output_match_c() {
    for input in samples() {
        for capacity in [0, 1, 2, 3, 4, 8, 16, 64, 300, 512, 16388, 32772] {
            let mut safe = vec![0xAA; capacity];
            let mut raw = vec![0; capacity + ffi::AV_LZO_OUTPUT_PADDING as usize];
            raw[..capacity].fill(0xAA);
            let mut reference = input.clone();
            reference.resize(input.len() + ffi::AV_LZO_INPUT_PADDING as usize, 0);
            for _ in 0..3 {
                let (mut out_left, mut in_left) = (capacity as i32, input.len() as i32);
                // SAFETY: independent initialized inputs have C's required
                // 8/12 byte padding and int-bounded lengths; scalar slots
                // are exclusive. No pointer is retained or freed by C.
                let flags = unsafe {
                    ffi::av_lzo1x_decode(
                        raw.as_mut_ptr().cast(),
                        &raw mut out_left,
                        reference.as_ptr().cast(),
                        &raw mut in_left,
                    )
                };
                let status = lzo::decode(&input, &mut safe).unwrap();
                assert_eq!(status.error_flags, flags);
                assert_eq!(status.input_remaining, in_left as usize);
                assert_eq!(status.output_remaining, out_left as usize);
                assert_eq!(safe, raw[..capacity]);
            }
        }
    }
}
