#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "test inputs are chosen within range, and the casts mirror the C conversions under comparison"
)]
#![expect(
    clippy::items_after_statements,
    reason = "each test is one scenario, its helpers and steps kept where they are used"
)]
#![expect(
    clippy::unreadable_literal,
    reason = "constants as the C sources and specifications spell them"
)]
use ffmpeg_bindings::libavutil::libavutil::lzo::decode;
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
#[cfg_attr(miri, ignore = "calls C")]
fn batch10_hidden_padding_errors_and_reused_destinations() {
    for input in samples() {
        for capacity in [0, 1, 2, 3, 4, 8, 16, 64, 300, 512, 16388, 32772] {
            let mut output = vec![0xAA; capacity];
            for _ in 0..3 {
                let status = decode(&input, &mut output).unwrap();
                assert!(status.input_remaining <= input.len());
                assert!(status.output_remaining <= output.len());
                output.fill(0xAA);
            }
        }
    }
    let mut output = [0; 9];
    assert_eq!(
        decode(&[20, b'a', b'b', b'c', 168, 0, 17, 0, 0], &mut output)
            .unwrap()
            .error_flags,
        0
    );
    assert_eq!(&output, b"abcabcabc");
    assert_eq!(decode(&[], &mut []).unwrap().error_flags, 3);
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn batch10_malformed_bounded_corpus() {
    let mut seed = 0x12345678u32;
    for len in 1..65 {
        for _ in 0..16 {
            let input: Vec<_> = (0..len)
                .map(|_| {
                    seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                    (seed >> 24) as u8
                })
                .collect();
            let mut output = [0xAA; 96];
            let status = decode(&input, &mut output).unwrap();
            assert!(status.input_remaining <= len);
            assert!(status.output_remaining <= output.len());
        }
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn batch10_extended_length_overflow_guard() {
    // get_len's 255-per-byte accumulator must report an error before its
    // signed int counter overflows, even when supplied a large valid slice.
    let input = vec![0; ((i32::MAX - 1000) / 255) as usize + 3];
    let mut output = [0; 32];
    let status = decode(&input, &mut output).unwrap();
    assert_ne!(status.error_flags & 8, 0);
    assert!(status.input_remaining <= input.len());
    assert!(status.output_remaining <= output.len());
}
