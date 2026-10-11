use ffmpeg::libavutil::{crc::CrcId, ffi};

#[test]
fn constants_and_lookup_selection_match_c() {
    for (id, raw) in [
        (CrcId::ATM_8, ffi::AVCRCId::AV_CRC_8_ATM),
        (CrcId::ANSI_16, ffi::AVCRCId::AV_CRC_16_ANSI),
        (CrcId::CCITT_16, ffi::AVCRCId::AV_CRC_16_CCITT),
        (CrcId::IEEE_32, ffi::AVCRCId::AV_CRC_32_IEEE),
        (CrcId::IEEE_32_LE, ffi::AVCRCId::AV_CRC_32_IEEE_LE),
        (CrcId::ANSI_16_LE, ffi::AVCRCId::AV_CRC_16_ANSI_LE),
        (CrcId::IEEE_24, ffi::AVCRCId::AV_CRC_24_IEEE),
        (CrcId::EBU_8, ffi::AVCRCId::AV_CRC_8_EBU),
    ] {
        assert_eq!(id.into_raw(), raw);
        // SAFETY: both arguments are valid value encodings; C bounds-checks
        // them, initializes tables with thread_once, and returns static
        // storage. No table representation or length is assumed here.
        let (actual, reference) = unsafe {
            (
                ffi::av_crc_get_table(id.into_raw()),
                ffi::av_crc_get_table(raw),
            )
        };
        assert!(!actual.is_null());
        assert_eq!(actual, reference);
    }
    for raw in [8, 9, u32::MAX] {
        let id = CrcId::from_raw(ffi::AVCRCId(raw));
        // SAFETY: C rejects out-of-range values before table indexing.
        let table = unsafe { ffi::av_crc_get_table(id.into_raw()) };
        assert!(table.is_null());
    }
}

#[test]
fn identifiers_select_the_same_tables_without_cpu_extensions() {
    const ISOLATED: &str = "CRUSTIFY_CRC_FALLBACK_CHILD";
    if std::env::var_os(ISOLATED).is_none() {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "crc::identifiers_select_the_same_tables_without_cpu_extensions",
                "--test-threads=1",
            ])
            .env(ISOLATED, "1")
            .status()
            .unwrap();
        assert!(status.success());
        return;
    }
    // SAFETY: zero enables no unsupported instructions. This isolated child
    // runs only CRC table lookups, which choose their implementation afresh;
    // no earlier context or buffer was allocated under different CPU flags.
    unsafe { ffmpeg::libavutil::cpu::force_cpu_flags(0) };
    constants_and_lookup_selection_match_c();
}
