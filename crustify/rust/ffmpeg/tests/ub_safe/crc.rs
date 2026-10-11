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

#[test]
fn standard_tables_are_static_shared_and_thread_safe() {
    use ffmpeg::libavutil::crc::{CrcId, get_table};
    let threads: Vec<_> = [
        CrcId::ATM_8,
        CrcId::ANSI_16,
        CrcId::CCITT_16,
        CrcId::IEEE_32,
        CrcId::IEEE_32_LE,
        CrcId::ANSI_16_LE,
        CrcId::IEEE_24,
        CrcId::EBU_8,
    ]
    .into_iter()
    .map(|id| {
        // C's once-control permits concurrent first lookup for each polynomial.
        std::thread::spawn(move || {
            let table = get_table(id).unwrap();
            let copy = table;
            let input = [0x57; 259];
            for len in [0, 1, 2, 3, 4, 7, 16, 255, 256] {
                let _crc = copy.update(table.update(0, &input[1..1 + len]), &input[2..2 + len]);
            }
            table
        })
    })
    .collect();
    for thread in threads {
        let table = thread.join().unwrap();
        let _crc = table.update(0, b"after originating thread exited");
    }
}
