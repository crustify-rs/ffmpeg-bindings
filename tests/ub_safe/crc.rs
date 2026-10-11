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
