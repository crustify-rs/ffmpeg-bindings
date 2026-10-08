//! Safe wrappers over `libavutil/avstring.h`.

use super::ffi;

/// Wraps: AVEscapeMode
/// Handwritten
///
/// How `av_escape` escapes special characters.
///
/// A Rust `enum`: the mode only ever flows from the caller into C as an
/// argument, and C never stores or returns one, so the safe surface names the
/// documented modes only. (C treats any other value as
/// [`Backslash`](Self::Backslash).)
///
/// Not every bit pattern is a mode, so C memory never reads as one: the
/// plain-element arrays reject it.
///
/// ```compile_fail,E0277
/// use ffmpeg::libavutil::{avstring::EscapeMode, mem::alloc_array_zeroed};
/// let _ = alloc_array_zeroed::<EscapeMode>(1);
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum EscapeMode {
    /// Let the library choose; currently [`Backslash`](Self::Backslash), which
    /// a later version may change.
    #[default]
    Auto,
    /// Backslash escaping.
    Backslash,
    /// Single-quote escaping.
    Quote,
    /// XML non-markup character data escaping.
    Xml,
}

impl EscapeMode {
    /// The C encoding of this mode.
    #[inline]
    pub const fn into_raw(self) -> ffi::AVEscapeMode {
        match self {
            Self::Auto => ffi::AVEscapeMode::AV_ESCAPE_MODE_AUTO,
            Self::Backslash => ffi::AVEscapeMode::AV_ESCAPE_MODE_BACKSLASH,
            Self::Quote => ffi::AVEscapeMode::AV_ESCAPE_MODE_QUOTE,
            Self::Xml => ffi::AVEscapeMode::AV_ESCAPE_MODE_XML,
        }
    }
}

impl From<EscapeMode> for ffi::AVEscapeMode {
    #[inline]
    fn from(mode: EscapeMode) -> Self {
        mode.into_raw()
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    const ALL: [EscapeMode; 4] = [
        EscapeMode::Auto,
        EscapeMode::Backslash,
        EscapeMode::Quote,
        EscapeMode::Xml,
    ];

    #[test]
    fn default_is_auto() {
        assert_eq!(EscapeMode::default(), EscapeMode::Auto);
        assert_eq!(
            ffi::AVEscapeMode::from(EscapeMode::default()),
            ffi::AVEscapeMode::AV_ESCAPE_MODE_AUTO
        );
    }

    #[test]
    fn modes_encode_to_distinct_values() {
        for (i, a) in ALL.iter().enumerate() {
            assert_eq!(ffi::AVEscapeMode::from(*a), a.into_raw());
            for b in &ALL[i + 1..] {
                assert_ne!(a.into_raw(), b.into_raw());
            }
        }
    }

    #[test]
    fn debug_names_the_variants() {
        let names: Vec<_> = ALL.iter().map(|m| format!("{m:?}")).collect();
        assert_eq!(names, ["Auto", "Backslash", "Quote", "Xml"]);
    }
}
