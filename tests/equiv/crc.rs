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

use ffmpeg::libavutil::crc::{Table, TableSize};

#[test]
fn batch10_table_outputs_and_stream_sequences_match_c() {
    for (le, bits, poly) in [
        (false, 8, 7),
        (false, 16, 0x8005),
        (false, 24, 0x864cfb),
        (false, 32, 0x04c11db7),
        (true, 16, 0xa001),
        (true, 32, 0xedb88320),
        (false, 8, 0),
        (true, 31, 0x12345678),
    ] {
        for size in [TableSize::Compact, TableSize::Fast] {
            let safe = Table::new(le, bits, poly, size).unwrap();
            let mut raw = vec![0; safe.entries().len()];
            // SAFETY: C receives aligned exclusive u32 storage of the accepted
            // byte size and valid bits/poly, and retains no pointer.
            let ret = unsafe {
                ffi::av_crc_init(
                    raw.as_mut_ptr(),
                    i32::from(le),
                    i32::from(bits),
                    poly,
                    (raw.len() * 4) as i32,
                )
            };
            assert_eq!(ret, 0);
            assert_eq!(safe.entries(), raw);
            let (mut left, mut right) = (u32::MAX, u32::MAX);
            for len in [0, 1, 2, 3, 4, 15, 16, 17, 63, 64, 65, 256, 1024] {
                for offset in 0..4 {
                    let mut reference = vec![0; len + 8];
                    for (i, b) in reference.iter_mut().enumerate() {
                        *b = (i * 107) as u8;
                    }
                    let source = reference[3 + offset..3 + offset + len].to_vec();
                    left = safe.update(left, &source);
                    // SAFETY: initialized private raw table and independent
                    // readable input; prefix keeps C's end - 3 in allocation.
                    right = unsafe {
                        ffi::av_crc(raw.as_ptr(), right, reference.as_ptr().add(3 + offset), len)
                    };
                    assert_eq!(left, right);
                }
            }
        }
    }
}

#[test]
fn batch10_scalar_tables_match_c_in_isolated_process() {
    const CHILD: &str = "CRUSTIFY_B10_CRC_SCALAR_CHILD";
    if std::env::var_os(CHILD).is_some() {
        // SAFETY: this isolated process runs exactly this test, before any
        // CRC operation or worker starts. Zero disables CPU features rather
        // than asserting unavailable instructions. No concurrent C user
        // exists, and the process exits without changing another test's flags.
        unsafe { ffi::av_force_cpu_flags(0) };
        batch10_table_outputs_and_stream_sequences_match_c();
        return;
    }
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .env(CHILD, "1")
        .args([
            "--exact",
            "crc::batch10_scalar_tables_match_c_in_isolated_process",
            "--test-threads=1",
        ])
        .status()
        .unwrap();
    assert!(status.success());
}

#[test]
fn standard_table_sequences_match_c() {
    use ffmpeg::libavutil::crc::{CrcId, get_table};
    for id in [
        CrcId::ATM_8,
        CrcId::ANSI_16,
        CrcId::CCITT_16,
        CrcId::IEEE_32,
        CrcId::IEEE_32_LE,
        CrcId::ANSI_16_LE,
        CrcId::IEEE_24,
        CrcId::EBU_8,
    ] {
        let actual = get_table(id).unwrap();
        // SAFETY: C validates the ID and returns a read-only static table;
        // no allocation is transferred or later freed by this reference.
        let reference = unsafe { ffi::av_crc_get_table(id.into_raw()) };
        assert!(!reference.is_null());
        let input = [0x57; 259];
        let (mut a, mut r) = (0x12345678, 0x12345678);
        for len in [0, 1, 2, 3, 4, 7, 16, 255, 256] {
            a = actual.update(a, &input[3..3 + len]);
            // SAFETY: an initialized immutable C table; input has a 3-byte
            // prefix so generic C's end-3 pointer remains within the allocation.
            r = unsafe { ffi::av_crc(reference, r, input.as_ptr().add(3), len) };
            assert_eq!(a, r);
        }
    }
    for id in [8, 9, u32::MAX] {
        let id = CrcId::from_raw(ffi::AVCRCId(id));
        assert!(get_table(id).is_none());
        // SAFETY: every enum encoding is validated by C, which returns NULL
        // rather than indexing a table for an unsupported identifier.
        assert!(unsafe { ffi::av_crc_get_table(id.into_raw()) }.is_null());
    }
}
