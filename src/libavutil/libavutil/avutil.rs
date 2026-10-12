//! Safe wrappers over `libavutil/avutil.h`.

use core::ffi::{CStr, c_void};
use core::fmt;
use core::ptr;

use ffibox::CPlainElem;

use super::ffi;
use super::rational::Rational;

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

/// Wraps: AVPictureType
/// Handwritten
///
/// The coding type of a video picture: intra, predicted, bidirectionally
/// predicted, and the rarer MPEG-4 and H.264 kinds.
///
/// A newtype over the C enum rather than a Rust `enum`, as [`MediaType`] is:
/// C stores the value in public struct fields its users write directly
/// (`AVFrame.pict_type`), so it may hold any value, and a Rust view of that
/// memory must accept every one. Match on the associated constants, with a
/// `_` arm for the values C does not name:
///
/// ```
/// use ffmpeg::libavutil::avutil::PictureType;
/// fn is_intra(t: PictureType) -> bool {
///     matches!(t, PictureType::I | PictureType::SI | PictureType::BI)
/// }
/// assert!(is_intra(PictureType::I));
/// assert!(!is_intra(PictureType::NONE));
/// ```
///
/// No ffibox primitive covers a C enum: it owns nothing, has no handles, and
/// is `Send + Sync` as the integer it is.
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct PictureType(ffi::AVPictureType);

impl PictureType {
    /// Undefined; the value of a zero-filled field.
    pub const NONE: Self = Self(ffi::AVPictureType::AV_PICTURE_TYPE_NONE);
    /// Intra.
    pub const I: Self = Self(ffi::AVPictureType::AV_PICTURE_TYPE_I);
    /// Predicted.
    pub const P: Self = Self(ffi::AVPictureType::AV_PICTURE_TYPE_P);
    /// Bidirectionally predicted.
    pub const B: Self = Self(ffi::AVPictureType::AV_PICTURE_TYPE_B);
    /// S(GMC)-VOP, MPEG-4.
    pub const S: Self = Self(ffi::AVPictureType::AV_PICTURE_TYPE_S);
    /// Switching intra.
    pub const SI: Self = Self(ffi::AVPictureType::AV_PICTURE_TYPE_SI);
    /// Switching predicted.
    pub const SP: Self = Self(ffi::AVPictureType::AV_PICTURE_TYPE_SP);
    /// BI type.
    pub const BI: Self = Self(ffi::AVPictureType::AV_PICTURE_TYPE_BI);

    /// The picture type C encodes as `raw`, whatever its value.
    #[inline]
    pub const fn from_raw(raw: ffi::AVPictureType) -> Self {
        Self(raw)
    }

    /// The C encoding of this picture type.
    #[inline]
    pub const fn into_raw(self) -> ffi::AVPictureType {
        self.0
    }

    /// The name of the constant this value equals, if any.
    const fn name(self) -> Option<&'static str> {
        Some(match self {
            Self::NONE => "NONE",
            Self::I => "I",
            Self::P => "P",
            Self::B => "B",
            Self::S => "S",
            Self::SI => "SI",
            Self::SP => "SP",
            Self::BI => "BI",
            _ => return None,
        })
    }
}

impl fmt::Debug for PictureType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.name() {
            Some(name) => f.write_str(name),
            None => f.debug_tuple("PictureType").field(&self.0.0).finish(),
        }
    }
}

impl From<ffi::AVPictureType> for PictureType {
    #[inline]
    fn from(raw: ffi::AVPictureType) -> Self {
        Self::from_raw(raw)
    }
}

impl From<PictureType> for ffi::AVPictureType {
    #[inline]
    fn from(t: PictureType) -> Self {
        t.into_raw()
    }
}

// SAFETY: `PictureType` is `#[repr(transparent)]` over `ffi::AVPictureType`,
// itself `#[repr(transparent)]` over a `c_uint`, so every bit pattern is a
// valid value, and it is a value with no C-side identity: a copy read out of C
// memory is the whole of it.
unsafe impl CPlainElem for PictureType {}

/// Wraps: avutil_version
///
/// The `LIBAVUTIL_VERSION_INT` the library was built as:
/// `major << 16 | minor << 8 | micro`.
pub fn version() -> u32 {
    // SAFETY: `avutil_version` takes no arguments and returns a constant.
    unsafe { ffi::avutil_version() }
}

/// Wraps: av_version_info
///
/// An informative version string, usually the release number or a git
/// commit description. It has no fixed format and must not be parsed.
pub fn version_info() -> &'static CStr {
    // SAFETY: `av_version_info` returns the address of a string literal,
    // NUL-terminated and immutable for the life of the process.
    unsafe { CStr::from_ptr(ffi::av_version_info()) }
}

/// Wraps: avutil_configuration
///
/// The `configure` command line libavutil was built with.
pub fn configuration() -> &'static CStr {
    // SAFETY: `avutil_configuration` returns the address of a string literal,
    // NUL-terminated and immutable for the life of the process.
    unsafe { CStr::from_ptr(ffi::avutil_configuration()) }
}

/// Wraps: avutil_license
///
/// The license libavutil was built under, such as `LGPL version 2.1 or
/// later`.
pub fn license() -> &'static CStr {
    // SAFETY: `avutil_license` returns an address inside a string literal,
    // past its prefix and before its NUL, immutable for the life of the
    // process.
    unsafe { CStr::from_ptr(ffi::avutil_license()) }
}

/// Wraps: av_x_if_null
///
/// `p`, or `x` when `p` is `None`: C's `p ? p : x`, over shared borrows.
pub fn x_if_null<'a, T>(p: Option<&'a T>, x: Option<&'a T>) -> Option<&'a T> {
    let p: *const c_void = p.map_or(ptr::null(), |r| ptr::from_ref(r).cast());
    let x: *const c_void = x.map_or(ptr::null(), |r| ptr::from_ref(r).cast());
    // SAFETY: `av_x_if_null` compares `p` with NULL and returns `p` or `x`
    // unchanged, dereferencing neither.
    let chosen = unsafe { ffi::av_x_if_null(p, x) };
    // SAFETY: `chosen` is NULL or one of the two pointers above, each from a
    // `&'a T`, so it addresses a `T` shared for `'a`.
    unsafe { chosen.cast_const().cast::<T>().as_ref() }
}

/// Wraps: av_x_if_null
///
/// `p`, or `x` when `p` is `None`, as [`x_if_null`], over exclusive borrows:
/// the one not returned is given up with the call.
pub fn x_if_null_mut<'a, T>(p: Option<&'a mut T>, x: Option<&'a mut T>) -> Option<&'a mut T> {
    let p: *mut c_void = p.map_or(ptr::null_mut(), |r| ptr::from_mut(r).cast());
    let x: *mut c_void = x.map_or(ptr::null_mut(), |r| ptr::from_mut(r).cast());
    // SAFETY: as for `x_if_null`.
    let chosen = unsafe { ffi::av_x_if_null(p, x) };
    // SAFETY: `chosen` is NULL or one of the two pointers above, each from a
    // `&'a mut T` this call took by value, so it addresses a `T` exclusively
    // borrowed for `'a`, and no other reference to it remains.
    unsafe { chosen.cast::<T>().as_mut() }
}

/// `AV_FOURCC_MAX_STRING_SIZE`: the buffer [`fourcc_string`] fills.
const FOURCC_MAX_STRING_SIZE: usize = ffi::AV_FOURCC_MAX_STRING_SIZE as usize;

/// Wraps: av_fourcc_make_string
///
/// `fourcc` spelled out in `buf`, least significant byte first: a digit,
/// ASCII letter or one of `. -_` as itself, any other byte as its decimal
/// value in brackets (`[0]`). The result borrows `buf`.
///
/// ```
/// use ffmpeg::libavutil::avutil::fourcc_string;
/// let mut buf = [0; 32];
/// assert_eq!(fourcc_string(&mut buf, u32::from_le_bytes(*b"H264")), c"H264");
/// assert_eq!(fourcc_string(&mut buf, 0x0100_2e41), c"A.[0][1]");
/// ```
pub fn fourcc_string(buf: &mut [u8; FOURCC_MAX_STRING_SIZE], fourcc: u32) -> &CStr {
    // SAFETY: `buf` is `AV_FOURCC_MAX_STRING_SIZE` writable bytes, the size
    // `av_fourcc_make_string` bounds its `snprintf` calls by (at most 20
    // bytes and a NUL are written); it keeps no pointer to it.
    unsafe { ffi::av_fourcc_make_string(buf.as_mut_ptr().cast(), fourcc) };
    CStr::from_bytes_until_nul(buf).expect("snprintf NUL-terminates the buffer")
}

impl MediaType {
    /// Wraps: av_get_media_type_string
    ///
    /// The lowercase name C gives this media type (`"video"`, `"audio"`,
    /// `"data"`, `"subtitle"`, `"attachment"`), or `None` for
    /// [`UNKNOWN`](Self::UNKNOWN) and any value C does not name.
    pub fn as_str(self) -> Option<&'static str> {
        // SAFETY: `av_get_media_type_string` takes the enum by value, accepts
        // any value, and returns NULL or the address of a string literal.
        let name = unsafe { ffi::av_get_media_type_string(self.0) };
        // SAFETY: a non-NULL `name` is a NUL-terminated string literal,
        // immutable for the life of the process.
        let name = (!name.is_null()).then(|| unsafe { CStr::from_ptr(name) })?;
        // Every name C returns is ASCII.
        name.to_str().ok()
    }
}

impl PictureType {
    /// Wraps: av_get_picture_type_char
    ///
    /// The letter C prints for this picture type: `I`, `P`, `B` and `S` for
    /// those types, `i`, `p` and `b` for [`SI`](Self::SI),
    /// [`SP`](Self::SP) and [`BI`](Self::BI), and `?` for
    /// [`NONE`](Self::NONE) and any value C does not name.
    pub fn to_char(self) -> char {
        // SAFETY: `av_get_picture_type_char` takes the enum by value and maps
        // any value to an ASCII character.
        let c = unsafe { ffi::av_get_picture_type_char(self.0) };
        char::from(c as u8)
    }
}

/// Wraps: av_get_time_base_q
///
/// FFmpeg's internal time base, `1/AV_TIME_BASE`: microseconds.
pub fn time_base() -> Rational {
    // SAFETY: `av_get_time_base_q` takes no arguments and returns a constant
    // struct by value.
    Rational::from_raw(unsafe { ffi::av_get_time_base_q() })
}

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

    const PICTURE_NAMED: [(PictureType, &str); 8] = [
        (PictureType::NONE, "NONE"),
        (PictureType::I, "I"),
        (PictureType::P, "P"),
        (PictureType::B, "B"),
        (PictureType::S, "S"),
        (PictureType::SI, "SI"),
        (PictureType::SP, "SP"),
        (PictureType::BI, "BI"),
    ];

    #[test]
    fn picture_type_layout_matches_the_c_enum() {
        assert_eq!(size_of::<PictureType>(), size_of::<ffi::AVPictureType>());
        assert_eq!(align_of::<PictureType>(), align_of::<ffi::AVPictureType>());
        assert_eq!(size_of::<PictureType>(), size_of::<c_int>());
    }

    #[test]
    fn picture_type_debug_names_the_constants() {
        for (t, name) in PICTURE_NAMED {
            assert_eq!(format!("{t:?}"), name);
        }
    }

    #[test]
    fn picture_type_debug_shows_unnamed_values_raw() {
        for raw in [8, 9, 1 << 20, u32::MAX - 1, u32::MAX] {
            let t = PictureType::from_raw(ffi::AVPictureType(raw));
            assert_eq!(format!("{t:?}"), format!("PictureType({raw})"));
        }
    }

    #[test]
    fn picture_type_constants_are_distinct_and_ordered() {
        for (i, (t, _)) in PICTURE_NAMED.iter().enumerate() {
            assert_eq!(t.into_raw().0 as usize, i);
            for (u, _) in &PICTURE_NAMED[i + 1..] {
                assert_ne!(t, u);
            }
        }
    }

    #[test]
    fn picture_type_raw_conversions_round_trip_every_value() {
        for raw in [0, 1, 7, 8, 1 << 31, u32::MAX] {
            let c = ffi::AVPictureType(raw);
            assert_eq!(PictureType::from(c).into_raw(), c);
            assert_eq!(ffi::AVPictureType::from(PictureType::from_raw(c)), c);
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn x_if_null_picks_the_first_present_borrow() {
        let (a, b) = (1u64, 2u64);
        assert_eq!(x_if_null(Some(&a), Some(&b)), Some(&1));
        assert_eq!(x_if_null(None, Some(&b)), Some(&2));
        assert_eq!(x_if_null(Some(&a), None), Some(&1));
        assert_eq!(x_if_null::<u64>(None, None), None);
        assert!(core::ptr::eq(x_if_null(None, Some(&b)).unwrap(), &b));
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn x_if_null_mut_hands_back_a_writable_borrow() {
        let (mut a, mut b) = ([0u8; 3], [0u8; 3]);
        x_if_null_mut(Some(&mut a), Some(&mut b)).unwrap()[0] = 7;
        x_if_null_mut(None, Some(&mut b)).unwrap()[2] = 9;
        assert_eq!((a, b), ([7, 0, 0], [0, 0, 9]));
        assert!(x_if_null_mut::<()>(None, None).is_none());
        let mut unit = ();
        assert!(x_if_null_mut(Some(&mut unit), None).is_some());
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn version_packs_the_built_parts() {
        let v = version();
        assert_eq!(v >> 16, ffi::LIBAVUTIL_VERSION_MAJOR);
        assert_eq!(v >> 8 & 0xff, ffi::LIBAVUTIL_VERSION_MINOR);
        assert_eq!(v & 0xff, ffi::LIBAVUTIL_VERSION_MICRO);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn media_type_names_are_cs() {
        assert_eq!(MediaType::VIDEO.as_str(), Some("video"));
        assert_eq!(MediaType::ATTACHMENT.as_str(), Some("attachment"));
        assert_eq!(MediaType::UNKNOWN.as_str(), None);
        assert_eq!(MediaType::from_raw(ffi::AVMediaType(99)).as_str(), None);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn picture_type_letters_are_cs() {
        assert_eq!(PictureType::I.to_char(), 'I');
        assert_eq!(PictureType::SI.to_char(), 'i');
        assert_eq!(PictureType::BI.to_char(), 'b');
        assert_eq!(PictureType::NONE.to_char(), '?');
        assert_eq!(PictureType::from_raw(ffi::AVPictureType(99)).to_char(), '?');
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn time_base_is_one_microsecond() {
        let tb = time_base();
        assert_eq!((tb.as_ref().num(), tb.as_ref().den()), (1, 1_000_000));
        assert_eq!(tb.as_ref().den(), ffi::AV_TIME_BASE as i32);
    }
}
