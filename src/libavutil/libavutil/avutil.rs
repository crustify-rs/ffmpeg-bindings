//! Safe wrappers over `libavutil/avutil.h`.

use core::fmt;

use ffibox::CPlainElem;

use super::ffi;

/// Wraps: AVMediaType
/// Handwritten
///
/// The kind of data a stream, codec or filter pad carries.
///
/// A newtype over the C enum rather than a Rust `enum`: C stores the value in
/// public struct fields and arrays its users write directly, so it may hold
/// any `int`, and a Rust view of that memory must accept every one. Match on
/// the associated constants, with a `_` arm for the values C does not name:
///
/// ```
/// use ffmpeg::libavutil::avutil::MediaType;
/// fn is_av(t: MediaType) -> bool {
///     match t {
///         MediaType::VIDEO | MediaType::AUDIO => true,
///         _ => false,
///     }
/// }
/// assert!(is_av(MediaType::AUDIO));
/// assert!(!is_av(MediaType::UNKNOWN));
/// ```
///
/// No ffibox primitive covers a C enum: it owns nothing, has no handles, and
/// is `Send + Sync` as the integer it is.
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct MediaType(ffi::AVMediaType);

impl MediaType {
    /// Unknown media, usually treated as [`DATA`](Self::DATA).
    pub const UNKNOWN: Self = Self(ffi::AVMediaType::AVMEDIA_TYPE_UNKNOWN);
    /// Video.
    pub const VIDEO: Self = Self(ffi::AVMediaType::AVMEDIA_TYPE_VIDEO);
    /// Audio.
    pub const AUDIO: Self = Self(ffi::AVMediaType::AVMEDIA_TYPE_AUDIO);
    /// Opaque data, usually continuous.
    pub const DATA: Self = Self(ffi::AVMediaType::AVMEDIA_TYPE_DATA);
    /// Subtitles.
    pub const SUBTITLE: Self = Self(ffi::AVMediaType::AVMEDIA_TYPE_SUBTITLE);
    /// Opaque data, usually sparse.
    pub const ATTACHMENT: Self = Self(ffi::AVMediaType::AVMEDIA_TYPE_ATTACHMENT);

    /// The number of media types from [`VIDEO`](Self::VIDEO) to
    /// [`ATTACHMENT`](Self::ATTACHMENT) (`AVMEDIA_TYPE_NB`): the length of an
    /// array indexed by [`index`](Self::index).
    pub const COUNT: usize = ffi::AVMediaType::AVMEDIA_TYPE_NB.0 as usize;

    /// The media type C encodes as `raw`, whatever its value.
    #[inline]
    pub const fn from_raw(raw: ffi::AVMediaType) -> Self {
        Self(raw)
    }

    /// The C encoding of this media type.
    #[inline]
    pub const fn into_raw(self) -> ffi::AVMediaType {
        self.0
    }

    /// This media type's position in `0..COUNT`, or `None` for
    /// [`UNKNOWN`](Self::UNKNOWN) and any value C does not name.
    #[inline]
    pub const fn index(self) -> Option<usize> {
        let raw = self.0.0;
        if raw >= 0 && (raw as usize) < Self::COUNT {
            Some(raw as usize)
        } else {
            None
        }
    }

    /// The name of the constant this value equals, if any.
    const fn name(self) -> Option<&'static str> {
        Some(match self {
            Self::UNKNOWN => "UNKNOWN",
            Self::VIDEO => "VIDEO",
            Self::AUDIO => "AUDIO",
            Self::DATA => "DATA",
            Self::SUBTITLE => "SUBTITLE",
            Self::ATTACHMENT => "ATTACHMENT",
            _ => return None,
        })
    }
}

impl fmt::Debug for MediaType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.name() {
            Some(name) => f.write_str(name),
            None => f.debug_tuple("MediaType").field(&self.0.0).finish(),
        }
    }
}

impl From<ffi::AVMediaType> for MediaType {
    #[inline]
    fn from(raw: ffi::AVMediaType) -> Self {
        Self::from_raw(raw)
    }
}

impl From<MediaType> for ffi::AVMediaType {
    #[inline]
    fn from(t: MediaType) -> Self {
        t.into_raw()
    }
}

// SAFETY: `MediaType` is `#[repr(transparent)]` over `ffi::AVMediaType`, itself
// `#[repr(transparent)]` over a `c_int`, so every bit pattern is a valid value,
// and it is a value with no C-side identity: a copy read out of C memory is the
// whole of it.
unsafe impl CPlainElem for MediaType {}

#[cfg(test)]
mod unit_tests {
    use super::*;
    use core::ffi::c_int;
    use core::mem::{align_of, size_of};

    const NAMED: [(MediaType, &str); 6] = [
        (MediaType::UNKNOWN, "UNKNOWN"),
        (MediaType::VIDEO, "VIDEO"),
        (MediaType::AUDIO, "AUDIO"),
        (MediaType::DATA, "DATA"),
        (MediaType::SUBTITLE, "SUBTITLE"),
        (MediaType::ATTACHMENT, "ATTACHMENT"),
    ];

    #[test]
    fn layout_matches_the_c_enum() {
        assert_eq!(size_of::<MediaType>(), size_of::<ffi::AVMediaType>());
        assert_eq!(align_of::<MediaType>(), align_of::<ffi::AVMediaType>());
        assert_eq!(size_of::<MediaType>(), size_of::<c_int>());
    }

    #[test]
    fn debug_names_the_constants() {
        for (t, name) in NAMED {
            assert_eq!(format!("{t:?}"), name);
        }
    }

    #[test]
    fn debug_shows_unnamed_values_raw() {
        let nb = MediaType::from_raw(ffi::AVMediaType::AVMEDIA_TYPE_NB);
        assert_eq!(format!("{nb:?}"), "MediaType(5)");
        for raw in [c_int::MIN, -2, 6, c_int::MAX] {
            let t = MediaType::from_raw(ffi::AVMediaType(raw));
            assert_eq!(format!("{t:?}"), format!("MediaType({raw})"));
        }
    }

    #[test]
    fn index_covers_exactly_the_counted_types() {
        let counted = [
            MediaType::VIDEO,
            MediaType::AUDIO,
            MediaType::DATA,
            MediaType::SUBTITLE,
            MediaType::ATTACHMENT,
        ];
        assert_eq!(counted.len(), MediaType::COUNT);
        for (i, t) in counted.into_iter().enumerate() {
            assert_eq!(t.index(), Some(i));
        }
        assert_eq!(MediaType::UNKNOWN.index(), None);
        for raw in [c_int::MIN, -2, 5, 6, c_int::MAX] {
            assert_eq!(MediaType::from_raw(ffi::AVMediaType(raw)).index(), None);
        }
    }

    #[test]
    fn raw_conversions_round_trip_every_value() {
        for raw in [c_int::MIN, -2, -1, 0, 4, 5, 6, c_int::MAX] {
            let c = ffi::AVMediaType(raw);
            assert_eq!(MediaType::from(c).into_raw(), c);
            assert_eq!(ffi::AVMediaType::from(MediaType::from_raw(c)), c);
        }
    }

    #[test]
    fn constants_are_distinct() {
        for (i, (a, _)) in NAMED.iter().enumerate() {
            for (b, _) in &NAMED[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }
}
