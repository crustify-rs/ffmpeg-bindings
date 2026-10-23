#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "test inputs are chosen within range, and the casts mirror the C conversions under comparison"
)]
#![expect(
    clippy::unreadable_literal,
    reason = "constants as the C sources and specifications spell them"
)]
use ffmpeg_bindings::libavutil::libavutil::adler32::update;

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn batch10_unaligned_empty_and_streaming() {
    let data: Vec<u8> = (0..8193).map(|i| (i * 73) as u8).collect();
    for start in 0..8 {
        let bytes = &data[start..];
        let mut sum = 1;
        for block in bytes.chunks(187) {
            sum = update(sum, block);
        }
        assert_eq!(sum, update(1, bytes));
        assert_eq!(update(sum, &[]), sum);
    }
    assert_eq!(update(1, b"Wikipedia"), 0x11E60398);
    assert_eq!(update(u32::MAX, &[]), u32::MAX);
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn batch10_stateless_threads_and_reborrows() {
    let workers: Vec<_> = (0..4)
        .map(|t| {
            std::thread::spawn(move || {
                use ffmpeg_bindings::libavutil::libavutil::crc::{Table, TableSize};
                use ffmpeg_bindings::libavutil::libavutil::{base64, hash, lzo, md5, uuid};
                let table = Table::new(true, 32, 0xEDB88320, TableSize::Fast).unwrap();
                let mut buffer = [0u8; 64];
                for i in 0..100 {
                    buffer.fill((t + i) as u8);
                    let data = buffer.to_vec();
                    let mut encoded = [0; 89];
                    let text = base64::encode(&data, &mut encoded).unwrap();
                    assert_eq!(base64::decode(text, &mut buffer), Ok(64));
                    assert_eq!(buffer.as_slice(), data);
                    assert_eq!(table.update(0, &buffer), table.clone().update(0, &data));
                    assert_eq!(md5::sum(&buffer), md5::sum(&data));
                    assert!(hash::name(0).is_some());
                    let mut formatted = [0; 37];
                    let id = [i as u8; 16];
                    assert_eq!(uuid::parse(uuid::unparse(&id, &mut formatted)).unwrap(), id);
                    let mut output = [0; 3];
                    assert_eq!(
                        lzo::decode(&[20, b'a', b'b', b'c', 17, 0, 0], &mut output)
                            .unwrap()
                            .error_flags,
                        0
                    );
                    assert_eq!(&output, b"abc");
                }
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
}
