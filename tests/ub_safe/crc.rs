use ffmpeg::libavutil::{crc::CrcId, ffi};

#[test]
fn arbitrary_identifiers_remain_valid_values() {
    for raw in [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, u32::MAX] {
        let id = CrcId::from_raw(ffi::AVCRCId(raw));
        let copy = id;
        assert_eq!(id, copy);
        assert_eq!(copy.into_raw().0, raw);
        assert!(!format!("{id:?}").is_empty());
    }
}

#[test]
fn identifiers_cross_threads_without_shared_mutable_storage() {
    let ids = [CrcId::ATM_8, CrcId::EBU_8, CrcId::IEEE_32_LE];
    let thread = std::thread::spawn(move || ids);
    assert_eq!(thread.join().unwrap(), ids);
}

use ffmpeg::libavutil::crc::{Table, TableSize};

#[test]
fn batch10_tables_clone_share_and_stream_unaligned_data() {
    let input: Vec<_> = (0..4097).map(|i| (i * 137) as u8).collect();
    for (le, bits, poly) in [
        (false, 8, 7),
        (false, 16, 0x1021),
        (false, 24, 0x864cfb),
        (false, 32, 0x04c11db7),
        (true, 16, 0xa001),
        (true, 32, 0xedb88320),
    ] {
        for size in [TableSize::Compact, TableSize::Fast] {
            let table = Table::new(le, bits, poly, size).unwrap();
            let clone = table.clone();
            for offset in 0..4 {
                let bytes = &input[offset..];
                let mut crc = 0;
                for chunk in bytes.chunks(13) {
                    crc = table.update(crc, chunk);
                }
                assert_eq!(crc, clone.update(0, bytes));
                for n in 0..3 {
                    let _ = clone.update(crc, &bytes[..n]);
                }
            }
            let shared = std::sync::Arc::new(table);
            let worker = std::thread::spawn(move || shared.update(0, b"123456789"));
            assert_eq!(worker.join().unwrap(), clone.update(0, b"123456789"));
        }
    }
}
