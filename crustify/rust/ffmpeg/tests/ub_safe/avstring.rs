//! UB tests for `libavutil/avstring.h`'s [`EscapeMode`], through the safe API
//! only.

use std::thread;

use ffmpeg::libavutil::avstring::EscapeMode;
use ffmpeg::libavutil::ffi;

const ALL: [EscapeMode; 4] = [
    EscapeMode::Auto,
    EscapeMode::Backslash,
    EscapeMode::Quote,
    EscapeMode::Xml,
];

#[test]
fn every_mode_encodes_to_a_c_enumerator() {
    let known = [
        ffi::AVEscapeMode::AV_ESCAPE_MODE_AUTO,
        ffi::AVEscapeMode::AV_ESCAPE_MODE_BACKSLASH,
        ffi::AVEscapeMode::AV_ESCAPE_MODE_QUOTE,
        ffi::AVEscapeMode::AV_ESCAPE_MODE_XML,
    ];
    for m in ALL {
        assert!(known.contains(&m.into_raw()));
        assert!(!format!("{m:?}").is_empty());
    }
}

#[test]
fn modes_cross_threads() {
    let back = thread::spawn(|| ALL.map(ffi::AVEscapeMode::from))
        .join()
        .unwrap();
    assert_eq!(back, ALL.map(EscapeMode::into_raw));
}
