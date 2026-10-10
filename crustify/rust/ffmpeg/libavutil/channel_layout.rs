//! Safe wrappers over `libavutil/channel_layout.h`.

use core::{cmp, fmt};

use ffibox::CPlainElem;

use super::ffi;

/// Wraps: AVChannel
/// Handwritten
///
/// The identity of one audio channel: a speaker position, an Ambisonic
/// component, or a user-defined id.
///
/// A newtype over the C enum rather than a Rust `enum`: C stores the value in
/// a public struct field its users write directly (`AVChannelCustom.id`), and
/// `av_channel_from_string` returns any non-negative `int` for `"USR<n>"`, so
/// it may hold any value, and a Rust view of that memory must accept every
/// one. Match on the associated constants, with a `_` arm for the values C
/// does not name:
///
/// ```
/// use ffmpeg::libavutil::channel_layout::Channel;
/// fn is_lfe(ch: Channel) -> bool {
///     matches!(ch, Channel::LOW_FREQUENCY | Channel::LOW_FREQUENCY_2)
/// }
/// assert!(is_lfe(Channel::LOW_FREQUENCY));
/// assert!(!is_lfe(Channel::FRONT_LEFT));
/// ```
///
/// The ids from [`AMBISONIC_BASE`](Self::AMBISONIC_BASE) to
/// [`AMBISONIC_END`](Self::AMBISONIC_END) are Ambisonic components by ACN
/// index; [`ambisonic`](Self::ambisonic) and
/// [`ambisonic_index`](Self::ambisonic_index) convert between the two.
///
/// Values order as C compares them: the native channel order is the order of
/// the ids.
///
/// No ffibox primitive covers a C enum: it owns nothing, has no handles, and
/// is `Send + Sync` as the integer it is.
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Channel(ffi::AVChannel);

impl Channel {
    /// Invalid channel index.
    pub const NONE: Self = Self(ffi::AVChannel::AV_CHAN_NONE);
    /// Front left (`FL`).
    pub const FRONT_LEFT: Self = Self(ffi::AVChannel::AV_CHAN_FRONT_LEFT);
    /// Front right (`FR`).
    pub const FRONT_RIGHT: Self = Self(ffi::AVChannel::AV_CHAN_FRONT_RIGHT);
    /// Front center (`FC`).
    pub const FRONT_CENTER: Self = Self(ffi::AVChannel::AV_CHAN_FRONT_CENTER);
    /// Low frequency (`LFE`).
    pub const LOW_FREQUENCY: Self = Self(ffi::AVChannel::AV_CHAN_LOW_FREQUENCY);
    /// Back left (`BL`).
    pub const BACK_LEFT: Self = Self(ffi::AVChannel::AV_CHAN_BACK_LEFT);
    /// Back right (`BR`).
    pub const BACK_RIGHT: Self = Self(ffi::AVChannel::AV_CHAN_BACK_RIGHT);
    /// Front left-of-center (`FLC`).
    pub const FRONT_LEFT_OF_CENTER: Self = Self(ffi::AVChannel::AV_CHAN_FRONT_LEFT_OF_CENTER);
    /// Front right-of-center (`FRC`).
    pub const FRONT_RIGHT_OF_CENTER: Self = Self(ffi::AVChannel::AV_CHAN_FRONT_RIGHT_OF_CENTER);
    /// Back center (`BC`).
    pub const BACK_CENTER: Self = Self(ffi::AVChannel::AV_CHAN_BACK_CENTER);
    /// Side left (`SL`).
    pub const SIDE_LEFT: Self = Self(ffi::AVChannel::AV_CHAN_SIDE_LEFT);
    /// Side right (`SR`).
    pub const SIDE_RIGHT: Self = Self(ffi::AVChannel::AV_CHAN_SIDE_RIGHT);
    /// Top center (`TC`).
    pub const TOP_CENTER: Self = Self(ffi::AVChannel::AV_CHAN_TOP_CENTER);
    /// Top front left (`TFL`).
    pub const TOP_FRONT_LEFT: Self = Self(ffi::AVChannel::AV_CHAN_TOP_FRONT_LEFT);
    /// Top front center (`TFC`).
    pub const TOP_FRONT_CENTER: Self = Self(ffi::AVChannel::AV_CHAN_TOP_FRONT_CENTER);
    /// Top front right (`TFR`).
    pub const TOP_FRONT_RIGHT: Self = Self(ffi::AVChannel::AV_CHAN_TOP_FRONT_RIGHT);
    /// Top back left (`TBL`).
    pub const TOP_BACK_LEFT: Self = Self(ffi::AVChannel::AV_CHAN_TOP_BACK_LEFT);
    /// Top back center (`TBC`).
    pub const TOP_BACK_CENTER: Self = Self(ffi::AVChannel::AV_CHAN_TOP_BACK_CENTER);
    /// Top back right (`TBR`).
    pub const TOP_BACK_RIGHT: Self = Self(ffi::AVChannel::AV_CHAN_TOP_BACK_RIGHT);
    /// Stereo downmix left (`DL`).
    pub const STEREO_LEFT: Self = Self(ffi::AVChannel::AV_CHAN_STEREO_LEFT);
    /// Stereo downmix right (`DR`).
    pub const STEREO_RIGHT: Self = Self(ffi::AVChannel::AV_CHAN_STEREO_RIGHT);
    /// Wide left (`WL`).
    pub const WIDE_LEFT: Self = Self(ffi::AVChannel::AV_CHAN_WIDE_LEFT);
    /// Wide right (`WR`).
    pub const WIDE_RIGHT: Self = Self(ffi::AVChannel::AV_CHAN_WIDE_RIGHT);
    /// Surround direct left (`SDL`).
    pub const SURROUND_DIRECT_LEFT: Self = Self(ffi::AVChannel::AV_CHAN_SURROUND_DIRECT_LEFT);
    /// Surround direct right (`SDR`).
    pub const SURROUND_DIRECT_RIGHT: Self = Self(ffi::AVChannel::AV_CHAN_SURROUND_DIRECT_RIGHT);
    /// Second low frequency (`LFE2`).
    pub const LOW_FREQUENCY_2: Self = Self(ffi::AVChannel::AV_CHAN_LOW_FREQUENCY_2);
    /// Top side left (`TSL`).
    pub const TOP_SIDE_LEFT: Self = Self(ffi::AVChannel::AV_CHAN_TOP_SIDE_LEFT);
    /// Top side right (`TSR`).
    pub const TOP_SIDE_RIGHT: Self = Self(ffi::AVChannel::AV_CHAN_TOP_SIDE_RIGHT);
    /// Bottom front center (`BFC`).
    pub const BOTTOM_FRONT_CENTER: Self = Self(ffi::AVChannel::AV_CHAN_BOTTOM_FRONT_CENTER);
    /// Bottom front left (`BFL`).
    pub const BOTTOM_FRONT_LEFT: Self = Self(ffi::AVChannel::AV_CHAN_BOTTOM_FRONT_LEFT);
    /// Bottom front right (`BFR`).
    pub const BOTTOM_FRONT_RIGHT: Self = Self(ffi::AVChannel::AV_CHAN_BOTTOM_FRONT_RIGHT);
    /// Side surround left, +90 degrees (`SSL`).
    pub const SIDE_SURROUND_LEFT: Self = Self(ffi::AVChannel::AV_CHAN_SIDE_SURROUND_LEFT);
    /// Side surround right, -90 degrees (`SSR`).
    pub const SIDE_SURROUND_RIGHT: Self = Self(ffi::AVChannel::AV_CHAN_SIDE_SURROUND_RIGHT);
    /// Top surround left, +110 degrees (`TTL`).
    pub const TOP_SURROUND_LEFT: Self = Self(ffi::AVChannel::AV_CHAN_TOP_SURROUND_LEFT);
    /// Top surround right, -110 degrees (`TTR`).
    pub const TOP_SURROUND_RIGHT: Self = Self(ffi::AVChannel::AV_CHAN_TOP_SURROUND_RIGHT);
    /// Binaural left (`BIL`).
    pub const BINAURAL_LEFT: Self = Self(ffi::AVChannel::AV_CHAN_BINAURAL_LEFT);
    /// Binaural right (`BIR`).
    pub const BINAURAL_RIGHT: Self = Self(ffi::AVChannel::AV_CHAN_BINAURAL_RIGHT);
    /// An empty channel that can be safely skipped.
    pub const UNUSED: Self = Self(ffi::AVChannel::AV_CHAN_UNUSED);
    /// A channel with data at an unknown position.
    pub const UNKNOWN: Self = Self(ffi::AVChannel::AV_CHAN_UNKNOWN);
    /// The Ambisonic component with ACN index 0.
    pub const AMBISONIC_BASE: Self = Self(ffi::AVChannel::AV_CHAN_AMBISONIC_BASE);
    /// The last Ambisonic component, ACN index 1023 (order-32 harmonics).
    pub const AMBISONIC_END: Self = Self(ffi::AVChannel::AV_CHAN_AMBISONIC_END);

    /// The channel C encodes as `raw`, whatever its value.
    #[inline]
    pub const fn from_raw(raw: ffi::AVChannel) -> Self {
        Self(raw)
    }

    /// The C encoding of this channel.
    #[inline]
    pub const fn into_raw(self) -> ffi::AVChannel {
        self.0
    }

    /// The Ambisonic component with ACN index `acn`, or `None` past
    /// [`AMBISONIC_END`](Self::AMBISONIC_END).
    #[inline]
    pub const fn ambisonic(acn: u16) -> Option<Self> {
        let base = Self::AMBISONIC_BASE.0.0;
        if acn as i32 <= Self::AMBISONIC_END.0.0 - base {
            Some(Self(ffi::AVChannel(base + acn as i32)))
        } else {
            None
        }
    }

    /// This channel's ACN index if it is an Ambisonic component, from
    /// [`AMBISONIC_BASE`](Self::AMBISONIC_BASE) to
    /// [`AMBISONIC_END`](Self::AMBISONIC_END) inclusive.
    #[inline]
    pub const fn ambisonic_index(self) -> Option<u16> {
        let (raw, base) = (self.0.0, Self::AMBISONIC_BASE.0.0);
        if raw >= base && raw <= Self::AMBISONIC_END.0.0 {
            Some((raw - base) as u16)
        } else {
            None
        }
    }

    /// The name of the constant this value equals, if any, the Ambisonic
    /// bounds aside.
    const fn name(self) -> Option<&'static str> {
        Some(match self {
            Self::NONE => "NONE",
            Self::FRONT_LEFT => "FRONT_LEFT",
            Self::FRONT_RIGHT => "FRONT_RIGHT",
            Self::FRONT_CENTER => "FRONT_CENTER",
            Self::LOW_FREQUENCY => "LOW_FREQUENCY",
            Self::BACK_LEFT => "BACK_LEFT",
            Self::BACK_RIGHT => "BACK_RIGHT",
            Self::FRONT_LEFT_OF_CENTER => "FRONT_LEFT_OF_CENTER",
            Self::FRONT_RIGHT_OF_CENTER => "FRONT_RIGHT_OF_CENTER",
            Self::BACK_CENTER => "BACK_CENTER",
            Self::SIDE_LEFT => "SIDE_LEFT",
            Self::SIDE_RIGHT => "SIDE_RIGHT",
            Self::TOP_CENTER => "TOP_CENTER",
            Self::TOP_FRONT_LEFT => "TOP_FRONT_LEFT",
            Self::TOP_FRONT_CENTER => "TOP_FRONT_CENTER",
            Self::TOP_FRONT_RIGHT => "TOP_FRONT_RIGHT",
            Self::TOP_BACK_LEFT => "TOP_BACK_LEFT",
            Self::TOP_BACK_CENTER => "TOP_BACK_CENTER",
            Self::TOP_BACK_RIGHT => "TOP_BACK_RIGHT",
            Self::STEREO_LEFT => "STEREO_LEFT",
            Self::STEREO_RIGHT => "STEREO_RIGHT",
            Self::WIDE_LEFT => "WIDE_LEFT",
            Self::WIDE_RIGHT => "WIDE_RIGHT",
            Self::SURROUND_DIRECT_LEFT => "SURROUND_DIRECT_LEFT",
            Self::SURROUND_DIRECT_RIGHT => "SURROUND_DIRECT_RIGHT",
            Self::LOW_FREQUENCY_2 => "LOW_FREQUENCY_2",
            Self::TOP_SIDE_LEFT => "TOP_SIDE_LEFT",
            Self::TOP_SIDE_RIGHT => "TOP_SIDE_RIGHT",
            Self::BOTTOM_FRONT_CENTER => "BOTTOM_FRONT_CENTER",
            Self::BOTTOM_FRONT_LEFT => "BOTTOM_FRONT_LEFT",
            Self::BOTTOM_FRONT_RIGHT => "BOTTOM_FRONT_RIGHT",
            Self::SIDE_SURROUND_LEFT => "SIDE_SURROUND_LEFT",
            Self::SIDE_SURROUND_RIGHT => "SIDE_SURROUND_RIGHT",
            Self::TOP_SURROUND_LEFT => "TOP_SURROUND_LEFT",
            Self::TOP_SURROUND_RIGHT => "TOP_SURROUND_RIGHT",
            Self::BINAURAL_LEFT => "BINAURAL_LEFT",
            Self::BINAURAL_RIGHT => "BINAURAL_RIGHT",
            Self::UNUSED => "UNUSED",
            Self::UNKNOWN => "UNKNOWN",
            _ => return None,
        })
    }
}

/// Named constants print their name, Ambisonic components
/// `AMBISONIC(<acn>)`, and every other value `Channel(<id>)`.
impl fmt::Debug for Channel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(name) = self.name() {
            f.write_str(name)
        } else if let Some(acn) = self.ambisonic_index() {
            f.debug_tuple("AMBISONIC").field(&acn).finish()
        } else {
            f.debug_tuple("Channel").field(&self.0.0).finish()
        }
    }
}

impl PartialOrd for Channel {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Channel {
    #[inline]
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        self.0.0.cmp(&other.0.0)
    }
}

impl From<ffi::AVChannel> for Channel {
    #[inline]
    fn from(raw: ffi::AVChannel) -> Self {
        Self::from_raw(raw)
    }
}

impl From<Channel> for ffi::AVChannel {
    #[inline]
    fn from(ch: Channel) -> Self {
        ch.into_raw()
    }
}

// SAFETY: `Channel` is `#[repr(transparent)]` over `ffi::AVChannel`, itself
// `#[repr(transparent)]` over a `c_int`, so every bit pattern is a valid
// value, and it is a value with no C-side identity: a copy read out of C
// memory is the whole of it.
unsafe impl CPlainElem for Channel {}

/// Wraps: AVMatrixEncoding
/// Handwritten
///
/// The matrixed-stereo encoding of a downmix: none, or one of the Dolby
/// surround variants that fold extra channels into a stereo pair.
///
/// A newtype over the C enum rather than a Rust `enum`: C stores the value as
/// a raw `int` (the payload of `AV_FRAME_DATA_MATRIXENCODING` frame side
/// data, libswresample's `matrix_encoding` option), so a Rust view of that
/// memory may meet any `int` and must accept every one. Match on the
/// associated constants, with a `_` arm for the values C does not name:
///
/// ```
/// use ffmpeg::libavutil::channel_layout::MatrixEncoding;
/// fn is_dolby(e: MatrixEncoding) -> bool {
///     match e {
///         MatrixEncoding::NONE => false,
///         MatrixEncoding::DOLBY
///         | MatrixEncoding::DOLBY_EX
///         | MatrixEncoding::DOLBY_HEADPHONE
///         | MatrixEncoding::DPLII
///         | MatrixEncoding::DPLIIX
///         | MatrixEncoding::DPLIIZ => true,
///         _ => false,
///     }
/// }
/// assert!(is_dolby(MatrixEncoding::DPLII));
/// assert!(!is_dolby(MatrixEncoding::NONE));
/// ```
///
/// No ffibox primitive covers a C enum: it owns nothing, has no handles, and
/// is `Send + Sync` as the integer it is.
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct MatrixEncoding(ffi::AVMatrixEncoding);

impl MatrixEncoding {
    /// No matrixed encoding.
    pub const NONE: Self = Self(ffi::AVMatrixEncoding::AV_MATRIX_ENCODING_NONE);
    /// Dolby Surround.
    pub const DOLBY: Self = Self(ffi::AVMatrixEncoding::AV_MATRIX_ENCODING_DOLBY);
    /// Dolby Pro Logic II.
    pub const DPLII: Self = Self(ffi::AVMatrixEncoding::AV_MATRIX_ENCODING_DPLII);
    /// Dolby Pro Logic IIx.
    pub const DPLIIX: Self = Self(ffi::AVMatrixEncoding::AV_MATRIX_ENCODING_DPLIIX);
    /// Dolby Pro Logic IIz.
    pub const DPLIIZ: Self = Self(ffi::AVMatrixEncoding::AV_MATRIX_ENCODING_DPLIIZ);
    /// Dolby Digital EX.
    pub const DOLBY_EX: Self = Self(ffi::AVMatrixEncoding::AV_MATRIX_ENCODING_DOLBYEX);
    /// Dolby Headphone.
    pub const DOLBY_HEADPHONE: Self =
        Self(ffi::AVMatrixEncoding::AV_MATRIX_ENCODING_DOLBYHEADPHONE);

    /// The number of encodings from [`NONE`](Self::NONE) to
    /// [`DOLBY_HEADPHONE`](Self::DOLBY_HEADPHONE) (`AV_MATRIX_ENCODING_NB`):
    /// the length of an array indexed by [`index`](Self::index).
    pub const COUNT: usize = ffi::AVMatrixEncoding::AV_MATRIX_ENCODING_NB.0 as usize;

    /// The encoding C encodes as `raw`, whatever its value.
    #[inline]
    pub const fn from_raw(raw: ffi::AVMatrixEncoding) -> Self {
        Self(raw)
    }

    /// The C encoding of this value.
    #[inline]
    pub const fn into_raw(self) -> ffi::AVMatrixEncoding {
        self.0
    }

    /// This encoding's position in `0..COUNT`, or `None` for any value C
    /// does not name.
    #[inline]
    pub const fn index(self) -> Option<usize> {
        let raw = self.0.0 as usize;
        if raw < Self::COUNT { Some(raw) } else { None }
    }

    /// The name of the constant this value equals, if any.
    const fn name(self) -> Option<&'static str> {
        Some(match self {
            Self::NONE => "NONE",
            Self::DOLBY => "DOLBY",
            Self::DPLII => "DPLII",
            Self::DPLIIX => "DPLIIX",
            Self::DPLIIZ => "DPLIIZ",
            Self::DOLBY_EX => "DOLBY_EX",
            Self::DOLBY_HEADPHONE => "DOLBY_HEADPHONE",
            _ => return None,
        })
    }
}

impl fmt::Debug for MatrixEncoding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.name() {
            Some(name) => f.write_str(name),
            None => f.debug_tuple("MatrixEncoding").field(&self.0.0).finish(),
        }
    }
}

impl From<ffi::AVMatrixEncoding> for MatrixEncoding {
    #[inline]
    fn from(raw: ffi::AVMatrixEncoding) -> Self {
        Self::from_raw(raw)
    }
}

impl From<MatrixEncoding> for ffi::AVMatrixEncoding {
    #[inline]
    fn from(e: MatrixEncoding) -> Self {
        e.into_raw()
    }
}

// SAFETY: `MatrixEncoding` is `#[repr(transparent)]` over
// `ffi::AVMatrixEncoding`, itself `#[repr(transparent)]` over a `c_uint`, so
// every bit pattern is a valid value, and it is a value with no C-side
// identity: a copy read out of C memory is the whole of it.
unsafe impl CPlainElem for MatrixEncoding {}

/// Wraps: AVChannelOrder
/// Handwritten
///
/// How a channel layout describes its channels: by count alone, as a bitmask
/// of native channels, as an explicit map, or as ambisonic components.
///
/// A newtype over the C enum rather than a Rust `enum`: C stores the value in
/// the public `AVChannelLayout.order` field its users write directly, so it
/// may hold any `unsigned`, and a Rust view of that memory must accept every
/// one. Match on the associated constants, with a `_` arm for the values C
/// does not name:
///
/// ```
/// use ffmpeg::libavutil::channel_layout::ChannelOrder;
/// fn has_mask(order: ChannelOrder) -> bool {
///     matches!(order, ChannelOrder::NATIVE | ChannelOrder::AMBISONIC)
/// }
/// assert!(has_mask(ChannelOrder::NATIVE));
/// assert!(!has_mask(ChannelOrder::CUSTOM));
/// ```
///
/// `FF_CHANNEL_ORDER_NB` is not part of FFmpeg's ABI or API and has no
/// constant. The default is [`UNSPEC`](Self::UNSPEC), the zeroed C value.
///
/// No ffibox primitive covers a C enum: it owns nothing, has no handles, and
/// is `Send + Sync` as the integer it is.
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChannelOrder(ffi::AVChannelOrder);

impl ChannelOrder {
    /// Only the channel count is known, not which channels they are or their
    /// order.
    pub const UNSPEC: Self = Self(ffi::AVChannelOrder::AV_CHANNEL_ORDER_UNSPEC);
    /// The channels are those set in the layout's mask, in the order of the
    /// `AVChannel` values; up to 63 distinct channels.
    pub const NATIVE: Self = Self(ffi::AVChannelOrder::AV_CHANNEL_ORDER_NATIVE);
    /// The channels are listed one by one in the layout's map, in any order,
    /// with unused or repeated channels and 64 or more of them allowed.
    pub const CUSTOM: Self = Self(ffi::AVChannelOrder::AV_CHANNEL_ORDER_CUSTOM);
    /// Each channel is one spherical harmonic of the sound field, in ACN
    /// order (SN3D normalization); the mask, if any, marks the trailing
    /// non-diegetic channels.
    pub const AMBISONIC: Self = Self(ffi::AVChannelOrder::AV_CHANNEL_ORDER_AMBISONIC);

    /// The channel order C encodes as `raw`, whatever its value.
    #[inline]
    pub const fn from_raw(raw: ffi::AVChannelOrder) -> Self {
        Self(raw)
    }

    /// The C encoding of this channel order.
    #[inline]
    pub const fn into_raw(self) -> ffi::AVChannelOrder {
        self.0
    }

    /// The name of the constant this value equals, if any.
    const fn name(self) -> Option<&'static str> {
        Some(match self {
            Self::UNSPEC => "UNSPEC",
            Self::NATIVE => "NATIVE",
            Self::CUSTOM => "CUSTOM",
            Self::AMBISONIC => "AMBISONIC",
            _ => return None,
        })
    }
}

impl Default for ChannelOrder {
    #[inline]
    fn default() -> Self {
        Self::UNSPEC
    }
}

impl fmt::Debug for ChannelOrder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.name() {
            Some(name) => f.write_str(name),
            None => f.debug_tuple("ChannelOrder").field(&self.0.0).finish(),
        }
    }
}

impl From<ffi::AVChannelOrder> for ChannelOrder {
    #[inline]
    fn from(raw: ffi::AVChannelOrder) -> Self {
        Self::from_raw(raw)
    }
}

impl From<ChannelOrder> for ffi::AVChannelOrder {
    #[inline]
    fn from(order: ChannelOrder) -> Self {
        order.into_raw()
    }
}

// SAFETY: `ChannelOrder` is `#[repr(transparent)]` over `ffi::AVChannelOrder`,
// itself `#[repr(transparent)]` over a `c_uint`, so every bit pattern is a
// valid value, and it is a value with no C-side identity: a copy read out of C
// memory is the whole of it.
unsafe impl CPlainElem for ChannelOrder {}

#[cfg(test)]
mod unit_tests {
    use core::ffi::{c_int, c_uint};
    use core::mem::{align_of, size_of};

    use super::*;

    /// Every named constant with its header value and Debug name.
    const CHANNEL_NAMED: [(Channel, c_int, &str); 39] = [
        (Channel::NONE, -1, "NONE"),
        (Channel::FRONT_LEFT, 0, "FRONT_LEFT"),
        (Channel::FRONT_RIGHT, 1, "FRONT_RIGHT"),
        (Channel::FRONT_CENTER, 2, "FRONT_CENTER"),
        (Channel::LOW_FREQUENCY, 3, "LOW_FREQUENCY"),
        (Channel::BACK_LEFT, 4, "BACK_LEFT"),
        (Channel::BACK_RIGHT, 5, "BACK_RIGHT"),
        (Channel::FRONT_LEFT_OF_CENTER, 6, "FRONT_LEFT_OF_CENTER"),
        (Channel::FRONT_RIGHT_OF_CENTER, 7, "FRONT_RIGHT_OF_CENTER"),
        (Channel::BACK_CENTER, 8, "BACK_CENTER"),
        (Channel::SIDE_LEFT, 9, "SIDE_LEFT"),
        (Channel::SIDE_RIGHT, 10, "SIDE_RIGHT"),
        (Channel::TOP_CENTER, 11, "TOP_CENTER"),
        (Channel::TOP_FRONT_LEFT, 12, "TOP_FRONT_LEFT"),
        (Channel::TOP_FRONT_CENTER, 13, "TOP_FRONT_CENTER"),
        (Channel::TOP_FRONT_RIGHT, 14, "TOP_FRONT_RIGHT"),
        (Channel::TOP_BACK_LEFT, 15, "TOP_BACK_LEFT"),
        (Channel::TOP_BACK_CENTER, 16, "TOP_BACK_CENTER"),
        (Channel::TOP_BACK_RIGHT, 17, "TOP_BACK_RIGHT"),
        (Channel::STEREO_LEFT, 29, "STEREO_LEFT"),
        (Channel::STEREO_RIGHT, 30, "STEREO_RIGHT"),
        (Channel::WIDE_LEFT, 31, "WIDE_LEFT"),
        (Channel::WIDE_RIGHT, 32, "WIDE_RIGHT"),
        (Channel::SURROUND_DIRECT_LEFT, 33, "SURROUND_DIRECT_LEFT"),
        (Channel::SURROUND_DIRECT_RIGHT, 34, "SURROUND_DIRECT_RIGHT"),
        (Channel::LOW_FREQUENCY_2, 35, "LOW_FREQUENCY_2"),
        (Channel::TOP_SIDE_LEFT, 36, "TOP_SIDE_LEFT"),
        (Channel::TOP_SIDE_RIGHT, 37, "TOP_SIDE_RIGHT"),
        (Channel::BOTTOM_FRONT_CENTER, 38, "BOTTOM_FRONT_CENTER"),
        (Channel::BOTTOM_FRONT_LEFT, 39, "BOTTOM_FRONT_LEFT"),
        (Channel::BOTTOM_FRONT_RIGHT, 40, "BOTTOM_FRONT_RIGHT"),
        (Channel::SIDE_SURROUND_LEFT, 41, "SIDE_SURROUND_LEFT"),
        (Channel::SIDE_SURROUND_RIGHT, 42, "SIDE_SURROUND_RIGHT"),
        (Channel::TOP_SURROUND_LEFT, 43, "TOP_SURROUND_LEFT"),
        (Channel::TOP_SURROUND_RIGHT, 44, "TOP_SURROUND_RIGHT"),
        (Channel::BINAURAL_LEFT, 61, "BINAURAL_LEFT"),
        (Channel::BINAURAL_RIGHT, 62, "BINAURAL_RIGHT"),
        (Channel::UNUSED, 0x200, "UNUSED"),
        (Channel::UNKNOWN, 0x300, "UNKNOWN"),
    ];

    /// Ids no constant names: the gaps, user ids and the `int` extremes.
    const CHANNEL_UNNAMED: [c_int; 10] = [
        c_int::MIN,
        -2,
        18,
        28,
        45,
        63,
        0x1ff,
        0x3ff,
        0x800,
        c_int::MAX,
    ];

    fn channel(raw: c_int) -> Channel {
        Channel::from_raw(ffi::AVChannel(raw))
    }

    #[test]
    fn channel_layout_matches_the_c_enum() {
        assert_eq!(size_of::<Channel>(), size_of::<ffi::AVChannel>());
        assert_eq!(align_of::<Channel>(), align_of::<ffi::AVChannel>());
        assert_eq!(size_of::<Channel>(), size_of::<c_int>());
    }

    #[test]
    fn channel_constants_have_the_header_values() {
        for (ch, raw, _) in CHANNEL_NAMED {
            assert_eq!(ch.into_raw().0, raw, "{ch:?}");
        }
        assert_eq!(Channel::AMBISONIC_BASE.into_raw().0, 0x400);
        assert_eq!(Channel::AMBISONIC_END.into_raw().0, 0x7ff);
    }

    #[test]
    fn channel_constants_are_distinct() {
        let all: Vec<Channel> = CHANNEL_NAMED
            .iter()
            .map(|&(ch, _, _)| ch)
            .chain([Channel::AMBISONIC_BASE, Channel::AMBISONIC_END])
            .collect();
        for (i, a) in all.iter().enumerate() {
            assert!(all[i + 1..].iter().all(|b| a != b), "{a:?}");
        }
    }

    #[test]
    fn channel_debug_names_the_constants() {
        for (ch, _, name) in CHANNEL_NAMED {
            assert_eq!(format!("{ch:?}"), name);
        }
    }

    #[test]
    fn channel_debug_shows_ambisonic_components_by_acn() {
        assert_eq!(format!("{:?}", Channel::AMBISONIC_BASE), "AMBISONIC(0)");
        assert_eq!(format!("{:?}", channel(0x401)), "AMBISONIC(1)");
        assert_eq!(format!("{:?}", Channel::AMBISONIC_END), "AMBISONIC(1023)");
    }

    #[test]
    fn channel_debug_shows_unnamed_values_raw() {
        for raw in CHANNEL_UNNAMED {
            assert_eq!(format!("{:?}", channel(raw)), format!("Channel({raw})"));
        }
    }

    #[test]
    fn ambisonic_accepts_exactly_the_acn_range() {
        assert_eq!(Channel::ambisonic(0), Some(Channel::AMBISONIC_BASE));
        assert_eq!(Channel::ambisonic(1), Some(channel(0x401)));
        assert_eq!(Channel::ambisonic(1023), Some(Channel::AMBISONIC_END));
        for acn in [1024, 1025, 0x8000, u16::MAX] {
            assert_eq!(Channel::ambisonic(acn), None, "acn {acn}");
        }
    }

    #[test]
    fn ambisonic_index_inverts_ambisonic() {
        for acn in 0..=1023 {
            let ch = Channel::ambisonic(acn).unwrap();
            assert_eq!(ch.ambisonic_index(), Some(acn));
            assert_eq!(ch.into_raw().0, 0x400 + c_int::from(acn));
        }
    }

    #[test]
    fn ambisonic_index_is_none_outside_the_range() {
        for (ch, _, _) in CHANNEL_NAMED {
            assert_eq!(ch.ambisonic_index(), None, "{ch:?}");
        }
        for raw in CHANNEL_UNNAMED {
            assert_eq!(channel(raw).ambisonic_index(), None, "raw {raw}");
        }
    }

    #[test]
    fn channel_order_is_the_id_order() {
        let mut raws: Vec<c_int> = CHANNEL_NAMED
            .iter()
            .map(|&(_, raw, _)| raw)
            .chain(CHANNEL_UNNAMED)
            .chain([0x400, 0x7ff])
            .collect();
        let mut channels: Vec<Channel> = raws.iter().map(|&r| channel(r)).collect();
        raws.sort_unstable();
        channels.sort_unstable();
        assert_eq!(
            channels.iter().map(|c| c.into_raw().0).collect::<Vec<_>>(),
            raws
        );
        assert!(Channel::NONE < Channel::FRONT_LEFT);
        assert!(Channel::BINAURAL_RIGHT < Channel::UNUSED);
    }

    #[test]
    fn channel_raw_conversions_round_trip_every_value() {
        let named = CHANNEL_NAMED.iter().map(|&(_, raw, _)| raw);
        for raw in named.chain(CHANNEL_UNNAMED).chain([0x400, 0x5ab, 0x7ff]) {
            let c = ffi::AVChannel(raw);
            assert_eq!(Channel::from(c).into_raw(), c);
            assert_eq!(ffi::AVChannel::from(Channel::from_raw(c)), c);
        }
    }

    const MATRIX_ENCODING_NAMED: [MatrixEncoding; 7] = [
        MatrixEncoding::NONE,
        MatrixEncoding::DOLBY,
        MatrixEncoding::DPLII,
        MatrixEncoding::DPLIIX,
        MatrixEncoding::DPLIIZ,
        MatrixEncoding::DOLBY_EX,
        MatrixEncoding::DOLBY_HEADPHONE,
    ];

    #[test]
    fn matrix_encoding_named_values_index_in_declaration_order() {
        assert_eq!(MatrixEncoding::COUNT, MATRIX_ENCODING_NAMED.len());
        for (i, e) in MATRIX_ENCODING_NAMED.into_iter().enumerate() {
            assert_eq!(e.index(), Some(i));
            assert!(e.name().is_some());
        }
    }

    #[test]
    fn matrix_encoding_unnamed_values_have_no_index_and_debug_as_raw() {
        let nb = MatrixEncoding::from_raw(ffi::AVMatrixEncoding::AV_MATRIX_ENCODING_NB);
        let huge = MatrixEncoding::from_raw(ffi::AVMatrixEncoding(u32::MAX));
        for e in [nb, huge] {
            assert_eq!(e.index(), None);
            assert_eq!(e.name(), None);
            assert!(format!("{e:?}").starts_with("MatrixEncoding("));
        }
        assert_eq!(format!("{nb:?}"), "MatrixEncoding(7)");
    }

    #[test]
    fn matrix_encoding_debug_prints_constant_names() {
        assert_eq!(format!("{:?}", MatrixEncoding::DPLII), "DPLII");
        assert_eq!(
            format!("{:?}", MatrixEncoding::DOLBY_HEADPHONE),
            "DOLBY_HEADPHONE"
        );
    }

    #[test]
    fn matrix_encoding_raw_conversions_round_trip() {
        for e in MATRIX_ENCODING_NAMED {
            let raw: ffi::AVMatrixEncoding = e.into();
            assert_eq!(MatrixEncoding::from(raw), e);
            assert_eq!(e.into_raw(), raw);
        }
    }

    const CHANNEL_ORDER_NAMED: [(ChannelOrder, &str); 4] = [
        (ChannelOrder::UNSPEC, "UNSPEC"),
        (ChannelOrder::NATIVE, "NATIVE"),
        (ChannelOrder::CUSTOM, "CUSTOM"),
        (ChannelOrder::AMBISONIC, "AMBISONIC"),
    ];

    /// Values C may store though it names none of them,
    /// `FF_CHANNEL_ORDER_NB` included.
    const CHANNEL_ORDER_UNNAMED: [c_uint; 6] =
        [4, 5, 1 << 16, 1 << 31, c_uint::MAX - 1, c_uint::MAX];

    #[test]
    fn channel_order_layout_matches_the_c_enum() {
        assert_eq!(size_of::<ChannelOrder>(), size_of::<ffi::AVChannelOrder>());
        assert_eq!(
            align_of::<ChannelOrder>(),
            align_of::<ffi::AVChannelOrder>()
        );
        assert_eq!(size_of::<ChannelOrder>(), size_of::<c_uint>());
        assert_eq!(
            size_of::<Option<ChannelOrder>>(),
            2 * size_of::<ChannelOrder>(),
            "every bit pattern is a value, so there is no niche"
        );
    }

    #[test]
    fn channel_order_constants_are_distinct_and_in_c_order() {
        for (i, (order, _)) in CHANNEL_ORDER_NAMED.iter().enumerate() {
            assert_eq!(order.into_raw().0, i as c_uint);
        }
        let nb = ffi::AVChannelOrder::FF_CHANNEL_ORDER_NB.0;
        assert_eq!(nb as usize, CHANNEL_ORDER_NAMED.len());
        assert!(CHANNEL_ORDER_UNNAMED.contains(&nb));
    }

    #[test]
    fn channel_order_debug_names_the_constants() {
        for (order, name) in CHANNEL_ORDER_NAMED {
            assert_eq!(format!("{order:?}"), name);
            assert_eq!(order.name(), Some(name));
        }
    }

    #[test]
    fn channel_order_debug_shows_unnamed_values_raw() {
        for raw in CHANNEL_ORDER_UNNAMED {
            let order = ChannelOrder::from_raw(ffi::AVChannelOrder(raw));
            assert_eq!(order.name(), None);
            assert_eq!(format!("{order:?}"), format!("ChannelOrder({raw})"));
            assert!(CHANNEL_ORDER_NAMED.iter().all(|&(n, _)| n != order));
        }
    }

    #[test]
    fn channel_order_default_is_the_zeroed_value() {
        assert_eq!(ChannelOrder::default(), ChannelOrder::UNSPEC);
        assert_eq!(ChannelOrder::default().into_raw().0, 0);
    }

    #[test]
    fn channel_order_raw_conversions_round_trip_every_value() {
        let all = CHANNEL_ORDER_NAMED
            .map(|(o, _)| o.into_raw().0)
            .into_iter()
            .chain(CHANNEL_ORDER_UNNAMED);
        for raw in all {
            let c = ffi::AVChannelOrder(raw);
            assert_eq!(ChannelOrder::from(c).into_raw(), c);
            assert_eq!(ffi::AVChannelOrder::from(ChannelOrder::from_raw(c)), c);
            assert_eq!(ChannelOrder::from_raw(c), ChannelOrder::from(c));
        }
    }

    #[test]
    #[allow(clippy::clone_on_copy)]
    fn channel_order_clone_and_hash_follow_the_encoding() {
        use std::collections::HashSet;
        let all: Vec<ChannelOrder> = CHANNEL_ORDER_NAMED
            .map(|(v, _)| v)
            .into_iter()
            .chain(CHANNEL_ORDER_UNNAMED.map(|r| ChannelOrder::from_raw(ffi::AVChannelOrder(r))))
            .collect();
        for v in &all {
            assert_eq!(v.clone().into_raw(), v.into_raw());
        }
        let set: HashSet<ChannelOrder> = all.iter().copied().collect();
        assert_eq!(set.len(), all.len());
        assert!(set.contains(&ChannelOrder::from_raw(ffi::AVChannelOrder(2))));
        assert!(set.contains(&ChannelOrder::CUSTOM));
    }
}
