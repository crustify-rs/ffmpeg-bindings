//! Equivalence of `libavutil/avstring.h`'s [`EscapeMode`] with the C enum, as
//! `av_escape` observes it.

use core::ffi::{CStr, c_char, c_int, c_void};

use ffmpeg::libavutil::avstring::EscapeMode;
use ffmpeg::libavutil::ffi;

/// Every mode beside its C enumerator.
const MODES: [(EscapeMode, ffi::AVEscapeMode); 4] = [
    (EscapeMode::Auto, ffi::AVEscapeMode::AV_ESCAPE_MODE_AUTO),
    (
        EscapeMode::Backslash,
        ffi::AVEscapeMode::AV_ESCAPE_MODE_BACKSLASH,
    ),
    (EscapeMode::Quote, ffi::AVEscapeMode::AV_ESCAPE_MODE_QUOTE),
    (EscapeMode::Xml, ffi::AVEscapeMode::AV_ESCAPE_MODE_XML),
];

/// `AV_ESCAPE_FLAG_WHITESPACE`, `_STRICT`, `_XML_SINGLE_QUOTES` and
/// `_XML_DOUBLE_QUOTES`, alone and combined.
const FLAGS: [c_int; 6] = [0, 1 << 0, 1 << 1, 1 << 2, 1 << 3, 0b1111];

const INPUTS: [&CStr; 7] = [
    c"",
    c"plain",
    c" leading and trailing ",
    c"it's a \\ back'slash",
    c"<tag attr=\"v\" alt='w'>&amp;</tag>",
    c"\t\n\r\x01\x7f",
    c"ünïcödé ' \" < >",
];

/// `av_escape(src, NULL, mode, flags)`: the escaped bytes, or the error code.
fn escape(
    src: &CStr,
    special: Option<&CStr>,
    mode: ffi::AVEscapeMode,
    flags: c_int,
) -> Result<Vec<u8>, c_int> {
    let mut dst: *mut c_char = core::ptr::null_mut();
    let special = special.map_or(core::ptr::null(), CStr::as_ptr);
    // SAFETY: `dst` is a writable out-slot; `src` and `special` are NULL or
    // live NUL-terminated strings, which `av_escape` only reads; it takes the
    // mode and flags by value and accepts any.
    let ret = unsafe { ffi::av_escape(&mut dst, src.as_ptr(), special, mode, flags) };
    if ret < 0 {
        assert!(dst.is_null());
        return Err(ret);
    }
    // SAFETY: on success `dst` is a fresh NUL-terminated `av_malloc`-family
    // string of `ret` bytes, which we own.
    let out = unsafe { CStr::from_ptr(dst) }.to_bytes().to_vec();
    // SAFETY: as above; freed once.
    unsafe { ffi::av_free(dst.cast::<c_void>()) };
    assert_eq!(out.len(), ret as usize);
    Ok(out)
}

#[test]
fn modes_encode_as_the_c_enumerators() {
    for (mode, c) in MODES {
        assert_eq!(mode.into_raw(), c);
        assert_eq!(ffi::AVEscapeMode::from(mode), c);
    }
    assert_eq!(
        EscapeMode::default().into_raw(),
        ffi::AVEscapeMode::AV_ESCAPE_MODE_AUTO
    );
}

#[test]
fn av_escape_behaves_alike_for_each_encoding() {
    for (mode, c) in MODES {
        for flags in FLAGS {
            for special in [None, Some(c"lt"), Some(c" ")] {
                for src in INPUTS {
                    assert_eq!(
                        escape(src, special, mode.into(), flags),
                        escape(src, special, c, flags),
                        "{mode:?} flags {flags:#x} {special:?} {src:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn chained_escapes_agree_after_every_step() {
    for (mode, c) in MODES {
        let mut rust = c"a 'b' <c> \\d".to_owned();
        let mut raw = rust.clone();
        for step in 0..4 {
            let r = escape(&rust, None, mode.into(), 0).unwrap();
            let k = escape(&raw, None, c, 0).unwrap();
            assert_eq!(r, k, "{mode:?} step {step}");
            rust = std::ffi::CString::new(r).unwrap();
            raw = std::ffi::CString::new(k).unwrap();
        }
    }
}

#[test]
fn default_mode_escapes_as_backslash_today() {
    // `bprint.c` maps `AV_ESCAPE_MODE_AUTO` to backslash escaping, as
    // `EscapeMode::Auto` documents.
    for src in INPUTS {
        assert_eq!(
            escape(src, None, EscapeMode::default().into(), 0),
            escape(src, None, ffi::AVEscapeMode::AV_ESCAPE_MODE_BACKSLASH, 0)
        );
    }
}

#[test]
fn the_modes_are_observably_distinct_in_c() {
    let src = c"it's <x> \\ y";
    let outs: Vec<_> = [EscapeMode::Backslash, EscapeMode::Quote, EscapeMode::Xml]
        .map(|m| escape(src, None, m.into(), 0).unwrap())
        .into();
    assert_ne!(outs[0], outs[1]);
    assert_ne!(outs[0], outs[2]);
    assert_ne!(outs[1], outs[2]);
}
