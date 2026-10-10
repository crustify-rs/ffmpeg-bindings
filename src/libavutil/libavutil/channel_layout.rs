//! Safe wrappers over `libavutil/channel_layout.h`.
//!
//! [`Channel::name`] and [`Channel::from_name`] convert a channel to and from
//! the abbreviation C writes in channel layout strings; C writes names into a
//! caller buffer, which [`Channel::write_name`] exposes as is, or appends
//! them to a print buffer ([`Channel::append_name`]).
//!
//! [`ChannelLayout`] is `AVChannelLayout`, a by-value structure that owns its
//! channel map in custom order. Rust holds one as a [`ChannelLayoutVal`],
//! which `av_channel_layout_uninit` disposes on drop, built by the C
//! initialisers ([`ChannelLayout::from_mask`], [`ChannelLayout::parse`],
//! [`ChannelLayout::custom`], [`ChannelLayout::default_for`]) or from a map
//! Rust filled ([`ChannelLayout::from_custom_map`]), and deep-copied by
//! `av_channel_layout_copy` ([`ChannelLayoutRef::try_clone`],
//! [`ChannelLayoutMut::copy_from`]). [`LayoutDetails`] reads the order and
//! the union it selects as one tagged value, and
//! [`ChannelLayoutMut::retype`] changes the order in place. The header's
//! standard layouts are [`ChannelLayout`]'s constants
//! (`AV_CHANNEL_LAYOUT_*`) over the [`masks`] (`AV_CH_LAYOUT_*`) of channel
//! bits ([`Channel::mask`], `AV_CH_*`); [`standard_layouts`] walks the ones
//! C knows by name.
//!
//! C's readers query a layout through its shared handle: the channel at an
//! index and back ([`ChannelLayoutRef::channel`],
//! [`ChannelLayoutRef::index_of`]), by name
//! ([`ChannelLayoutRef::index_by_name`],
//! [`ChannelLayoutRef::channel_by_name`]), its validity
//! ([`ChannelLayoutRef::is_valid`]), Ambisonic order and description
//! ([`ChannelLayoutRef::description`], which [`ChannelLayout::parse`] reads
//! back), and `==` compares two layouts as `av_channel_layout_compare` does.

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::hash::{Hash, Hasher};
use core::iter::FusedIterator;
use core::ptr::{self, NonNull, addr_of, addr_of_mut};
use core::{cmp, fmt};
use std::ffi::CString;

use ffibox::{CPlainElem, CSlice, CSliceMut, CVal, CZeroable, define_ctype, impl_cdispose};

use super::bprint::BPrintMut;
use super::ffi;
use super::mem::HeapVec;

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
/// The default is [`NONE`](Self::NONE), the value C returns for a channel it
/// cannot name (`av_channel_from_string`) or index
/// (`av_channel_layout_channel_from_index`).
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

    /// This channel's bit in a native-order mask, `1 << id`, which C spells
    /// `AV_CH_<NAME>` for the named channels (`AV_CH_FRONT_LEFT` is
    /// `1ULL << AV_CHAN_FRONT_LEFT`): `None` for a channel no mask can hold,
    /// an id outside `0..64` ([`NONE`](Self::NONE), [`UNUSED`](Self::UNUSED),
    /// [`UNKNOWN`](Self::UNKNOWN) and the Ambisonic components).
    ///
    /// ```
    /// use ffmpeg::libavutil::channel_layout::{Channel, masks};
    /// assert_eq!(Channel::FRONT_LEFT.mask(), Some(1));
    /// assert_eq!(
    ///     Channel::FRONT_LEFT.mask().unwrap() | Channel::FRONT_RIGHT.mask().unwrap(),
    ///     masks::STEREO
    /// );
    /// assert_eq!(Channel::UNKNOWN.mask(), None);
    /// ```
    #[inline]
    #[must_use]
    pub const fn mask(self) -> Option<u64> {
        let id = self.0.0;
        if id >= 0 && id < 64 {
            Some(1 << id)
        } else {
            None
        }
    }

    /// The name of the constant this value equals, if any, the Ambisonic
    /// bounds aside.
    const fn constant_name(self) -> Option<&'static str> {
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

impl Default for Channel {
    #[inline]
    fn default() -> Self {
        Self::NONE
    }
}

/// Named constants print their name, Ambisonic components
/// `AMBISONIC(<acn>)`, and every other value `Channel(<id>)`.
impl fmt::Debug for Channel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(name) = self.constant_name() {
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
/// `AV_MATRIX_ENCODING_NB` is a count, not an encoding, and has no constant
/// of its own: see [`COUNT`](Self::COUNT). The default is
/// [`NONE`](Self::NONE), the zeroed C value and the default of
/// libswresample's `matrix_encoding` option.
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

impl Default for MatrixEncoding {
    #[inline]
    fn default() -> Self {
        Self::NONE
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

define_ctype!(
    /// Wraps: AVChannelCustom
    ///
    /// One channel of a custom-order channel layout: its id, an optional
    /// name, and an untyped user pointer. Its getters are on
    /// [`ChannelCustomRef`] (`as_ref()`), its setters on
    /// [`ChannelCustomMut`] (`as_mut()`).
    ///
    /// C keeps these in the array a custom-order `AVChannelLayout` owns
    /// (`u.map`), which `av_channel_layout_custom_init` and
    /// `av_channel_layout_from_string` allocate with `av_calloc`,
    /// `av_channel_layout_copy` duplicates byte for byte, and
    /// `av_channel_layout_uninit` frees with `av_freep`; nothing allocates,
    /// frees or clones one on its own, so it is a value type with no owner.
    /// An element of such an array is reached as a [`ChannelCustomRef`] or
    /// [`ChannelCustomMut`] over the element, and a Rust-held value is a
    /// `ChannelCustom`, copied in and out with [`from_raw`](Self::from_raw)
    /// and [`into_raw`](Self::into_raw).
    #[derive(Clone, Copy)]
    ChannelCustom,
    ChannelCustomRef,
    ChannelCustomMut,
    ffi::AVChannelCustom
);

// SAFETY: all-zero is `FRONT_LEFT`, an empty name and a NULL `opaque`: the
// `AVChannelCustom custom = {0}` that `av_channel_layout_from_string` starts
// each channel from, and the element `av_calloc` gives
// `av_channel_layout_custom_init` before it sets the id. Every accessor takes
// any id, reads the name within its 16 bytes, and never dereferences
// `opaque`; the type has no teardown to reject it.
unsafe impl CZeroable for ChannelCustom {}

// SAFETY: an `AVChannelCustom` is plain data: an `int`, 16 `char`s and a
// pointer that neither FFmpeg (no C path reads, writes through or frees it)
// nor this crate dereferences, so a value, or an exclusive handle to one,
// moves between threads as an `(i32, [u8; 16], usize)` does; whoever
// dereferences the pointer does so in their own `unsafe` code, on their own
// terms.
unsafe impl Send for ChannelCustom {}
// SAFETY: as for `ChannelCustom`: the handle is the only access to the
// fields.
unsafe impl Send for ChannelCustomMut<'_> {}

// SAFETY: nothing reachable through a shared handle writes:
// `ChannelCustomRef` has getters only, which copy the fields out, and the
// struct has no interior mutability, so concurrent shared access reads
// unchanging bytes.
unsafe impl Sync for ChannelCustom {}
// SAFETY: as for `Sync for ChannelCustom`: copies of a shared handle only
// read.
unsafe impl Send for ChannelCustomRef<'_> {}
// SAFETY: as above.
unsafe impl Sync for ChannelCustomRef<'_> {}
// SAFETY: `&ChannelCustomMut` reaches the fields only through `as_ref`, a
// shared handle; the setters need `&mut`.
unsafe impl Sync for ChannelCustomMut<'_> {}

impl ChannelCustom {
    /// The channel `id`, with no name and a NULL user pointer.
    #[inline]
    #[must_use]
    pub fn new(id: Channel) -> Self {
        let mut ch = Self::zeroed();
        ch.as_mut().set_id(id);
        ch
    }

    /// The channel C encodes as `raw`, whatever its fields.
    #[inline]
    #[must_use]
    pub const fn from_raw(raw: ffi::AVChannelCustom) -> Self {
        Self(raw, core::marker::PhantomData)
    }

    /// The C encoding of this channel, for storing it in a channel map by
    /// value.
    #[inline]
    #[must_use]
    pub const fn into_raw(self) -> ffi::AVChannelCustom {
        self.0
    }
}

/// [`Channel::UNKNOWN`] with no name and a NULL user pointer: what
/// `av_channel_layout_custom_init` fills a new map with.
impl Default for ChannelCustom {
    #[inline]
    fn default() -> Self {
        Self::new(Channel::UNKNOWN)
    }
}

impl From<ffi::AVChannelCustom> for ChannelCustom {
    #[inline]
    fn from(raw: ffi::AVChannelCustom) -> Self {
        Self::from_raw(raw)
    }
}

impl From<ChannelCustom> for ffi::AVChannelCustom {
    #[inline]
    fn from(ch: ChannelCustom) -> Self {
        ch.into_raw()
    }
}

/// Field by field, as C observes them: the id, the name up to its NUL, and
/// the user pointer's address.
impl PartialEq for ChannelCustom {
    fn eq(&self, other: &Self) -> bool {
        let (a, b) = (self.as_ref(), other.as_ref());
        a.id() == b.id() && a.name() == b.name() && a.opaque() == b.opaque()
    }
}

impl Eq for ChannelCustom {}

/// Over the fields [`PartialEq`] compares: the id, the name up to its NUL,
/// and the user pointer's address.
impl Hash for ChannelCustom {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let r = self.as_ref();
        r.id().hash(state);
        r.name().hash(state);
        r.opaque().hash(state);
    }
}

impl fmt::Debug for ChannelCustom {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_ref().fmt(f)
    }
}

impl fmt::Debug for ChannelCustomRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ChannelCustom")
            .field("id", &self.id())
            .field("name", &self.name())
            .field("opaque", &self.opaque())
            .finish()
    }
}

impl ChannelCustomRef<'_> {
    /// Field: AVChannelCustom.id
    ///
    /// The channel's id: any value, user-defined ids included.
    #[must_use]
    pub fn id(&self) -> Channel {
        // SAFETY: a read of the `id` field projected through the shared
        // handle's pointer to a live, initialised `AVChannelCustom`; `Channel`
        // accepts every `int` C may store there.
        Channel::from_raw(unsafe { addr_of!((*self.as_ptr()).id).read() })
    }

    /// Field: AVChannelCustom.name
    ///
    /// A copy of the channel's name, empty when it has none: the bytes before
    /// the first NUL of the 16-byte field. A field with no NUL, which only a
    /// raw write can leave and C's readers would read past, reads as all 16
    /// bytes.
    #[must_use]
    pub fn name(&self) -> ChannelName {
        // SAFETY: a read of the 16-`char` `name` array projected through the
        // shared handle's pointer to a live, initialised `AVChannelCustom`;
        // every bit pattern is a valid `c_char`, and the copy reads no byte
        // outside the field.
        ChannelName::from_field(unsafe { addr_of!((*self.as_ptr()).name).read() })
    }

    /// Field: AVChannelCustom.opaque
    ///
    /// The user pointer, NULL unless a user stored one. FFmpeg never reads
    /// what it points at, never frees it, and copies the address as is when
    /// it copies the layout (`av_channel_layout_copy`), so what it points at
    /// and how long that lives are the user's alone: it is handed back as the
    /// raw cookie it is, and dereferencing it is the caller's `unsafe`
    /// business.
    #[must_use]
    pub fn opaque(&self) -> *mut c_void {
        // SAFETY: a read of the `opaque` field projected through the shared
        // handle's pointer to a live, initialised `AVChannelCustom`; the
        // pointer is copied, not dereferenced.
        unsafe { addr_of!((*self.as_ptr()).opaque).read() }
    }

    /// A copy of the whole channel, independent of the place it was read
    /// from; the user pointer is copied, not what it points at, as
    /// `av_channel_layout_copy` does.
    #[must_use]
    pub fn get(&self) -> ChannelCustom {
        // SAFETY: a read of the whole live, initialised `AVChannelCustom`
        // through the shared handle's pointer; it owns nothing, so the byte
        // copy is a complete, independent value.
        ChannelCustom::from_raw(unsafe { self.as_ptr().read() })
    }
}

impl ChannelCustomMut<'_> {
    /// Field: AVChannelCustom.id
    ///
    /// Sets the channel's id, which C stores as given.
    pub fn set_id(&mut self, id: Channel) {
        // SAFETY: a write of the `id` field projected through the exclusive
        // handle's pointer to a live `AVChannelCustom`; the old value owns
        // nothing to leak.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).id).write(id.into_raw()) }
    }

    /// Field: AVChannelCustom.name
    ///
    /// Sets the channel's name to `name` if it fits the field with its NUL
    /// (at most [`ChannelName::MAX_LEN`] bytes); otherwise leaves the name as
    /// it was and returns the error. An empty name clears it.
    pub fn set_name(&mut self, name: &CStr) -> Result<(), NameTooLong> {
        let len = name.to_bytes().len();
        if len > ChannelName::MAX_LEN {
            return Err(NameTooLong { len });
        }
        self.write_name(name.to_bytes());
        Ok(())
    }

    /// Field: AVChannelCustom.name
    ///
    /// Sets the channel's name to the first [`ChannelName::MAX_LEN`] bytes of
    /// `name`, as `av_channel_layout_from_string` stores a parsed name
    /// (`av_strlcpy`): the bytes and a NUL are written, and the field's bytes
    /// after them are left as they were. Returns whether `name` fit whole.
    pub fn set_name_truncated(&mut self, name: &CStr) -> bool {
        let bytes = name.to_bytes();
        let kept = &bytes[..bytes.len().min(ChannelName::MAX_LEN)];
        self.write_name(kept);
        kept.len() == bytes.len()
    }

    /// Writes `bytes`, which hold no NUL and fit with one, and a NUL to the
    /// start of the `name` field.
    fn write_name(&mut self, bytes: &[u8]) {
        debug_assert!(bytes.len() <= ChannelName::MAX_LEN);
        let len = bytes.len().min(ChannelName::MAX_LEN);
        // SAFETY: projects the `name` field through the exclusive handle's
        // pointer to a live `AVChannelCustom`, without forming a reference.
        let field = unsafe { addr_of_mut!((*self.as_mut_ptr()).name) }.cast::<u8>();
        // SAFETY: `field` addresses the 16-byte `name` array of the live
        // `AVChannelCustom` the exclusive handle covers, so its first
        // `len + 1 <= 16` bytes are writable; `bytes` is a Rust slice, which
        // cannot overlap C memory only this handle reaches; `u8` and `c_char`
        // share size, alignment and every bit pattern.
        unsafe {
            core::ptr::copy_nonoverlapping(bytes.as_ptr(), field, len);
            field.add(len).write(0);
        }
    }

    /// Field: AVChannelCustom.opaque
    ///
    /// Stores the user pointer `opaque`, NULL included. Nothing in FFmpeg or
    /// this crate dereferences or frees it, so any address is safe to store:
    /// keeping what it points at alive for as long as the caller means to
    /// read it back is the caller's concern, as is whatever it leaks when
    /// the map is freed.
    // The lint reads the write of `opaque` as a dereference of it: the
    // pointer is the value stored, through the handle's own pointer.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn set_opaque(&mut self, opaque: *mut c_void) {
        // SAFETY: a write of the `opaque` field projected through the
        // exclusive handle's pointer to a live `AVChannelCustom`; the old
        // pointer is a cookie nothing frees, so overwriting it leaks nothing
        // C or this crate owns.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).opaque).write(opaque) }
    }

    /// Overwrites the whole channel with `ch`'s fields, the name's bytes
    /// after its NUL included.
    pub fn set(&mut self, ch: ChannelCustom) {
        // SAFETY: a write of the whole `AVChannelCustom` through the
        // exclusive handle's pointer to a live one; the old value owns
        // nothing to leak.
        unsafe { self.as_mut_ptr().write(ch.into_raw()) }
    }
}

/// A copy of an [`AVChannelCustom`](ChannelCustom)'s name: up to
/// [`MAX_LEN`](Self::MAX_LEN) bytes with no NUL, read as a [`CStr`] or as
/// bytes. Names compare, hash and print by their bytes.
#[derive(Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ChannelName([u8; ChannelName::FIELD_LEN + 1]);

impl ChannelName {
    /// Size of the C field, `char name[16]`, its NUL included.
    const FIELD_LEN: usize = 16;

    /// The longest name the field stores with its NUL.
    pub const MAX_LEN: usize = Self::FIELD_LEN - 1;

    /// The name in `field`: its bytes before the first NUL, or all of them,
    /// with zeros after them.
    fn from_field(field: [c_char; Self::FIELD_LEN]) -> Self {
        let mut buf = [0; Self::FIELD_LEN + 1];
        for (b, c) in buf.iter_mut().zip(field) {
            if c == 0 {
                break;
            }
            *b = c as u8;
        }
        Self(buf)
    }

    /// The name as a C string: the bytes up to the copy's NUL.
    #[must_use]
    pub fn as_c_str(&self) -> &CStr {
        // The copy always ends in a NUL, after at most `FIELD_LEN` bytes.
        CStr::from_bytes_until_nul(&self.0).unwrap_or_default()
    }

    /// The name's bytes, without a NUL.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        self.as_c_str().to_bytes()
    }

    /// The name as UTF-8, if it is.
    pub fn to_str(&self) -> Result<&str, core::str::Utf8Error> {
        self.as_c_str().to_str()
    }

    /// Whether the channel has no name.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0[0] == 0
    }
}

impl PartialEq<CStr> for ChannelName {
    fn eq(&self, other: &CStr) -> bool {
        self.as_c_str() == other
    }
}

impl PartialEq<&CStr> for ChannelName {
    fn eq(&self, other: &&CStr) -> bool {
        self.as_c_str() == *other
    }
}

impl fmt::Debug for ChannelName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_c_str().fmt(f)
    }
}

/// [`ChannelCustomMut::set_name`]'s refusal: the name is longer than the
/// field holds with its NUL.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NameTooLong {
    /// The refused name's length, in bytes.
    pub len: usize,
}

impl fmt::Display for NameTooLong {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "channel name of {} bytes exceeds the {}-byte limit",
            self.len,
            ChannelName::MAX_LEN
        )
    }
}

impl core::error::Error for NameTooLong {}

/// The size to pass C for a buffer of `len` bytes: `len` capped at
/// `u32::MAX`, since C narrows the size to an `unsigned` (a length of 2^32
/// would read as count-only and leave the buffer unterminated).
fn c_buffer_size(len: usize) -> usize {
    len.min(c_uint::MAX as usize)
}

/// The C buffer for a `&mut [u8]` the channel name writers fill: NULL with
/// size 0 for an empty slice, whose pointer is dangling, and otherwise its
/// pointer with its [capped](c_buffer_size) length.
fn name_buffer(buf: &mut [u8]) -> (*mut c_char, usize) {
    if buf.is_empty() {
        (ptr::null_mut(), 0)
    } else {
        (buf.as_mut_ptr().cast(), c_buffer_size(buf.len()))
    }
}

/// A C channel name writer: `av_channel_name` or `av_channel_description`.
type NameWriter = unsafe extern "C" fn(*mut c_char, usize, ffi::AVChannel) -> c_int;

impl Channel {
    /// Calls `write` on `buf` as [`write_name`](Self::write_name) describes.
    fn write_with(self, write: NameWriter, buf: &mut [u8]) -> Result<usize, i32> {
        let (ptr, size) = name_buffer(buf);
        // SAFETY: `ptr` is NULL with size 0, which C takes as a count-only
        // request and never dereferences, or valid for `size` bytes of
        // writes, which bounds every byte C writes; C keeps no pointer to it.
        let ret = unsafe { write(ptr, size, self.into_raw()) };
        usize::try_from(ret).map_err(|_| ret)
    }

    /// The whole string `write` produces, measured first.
    fn to_c_string(self, write: NameWriter) -> CString {
        // C fails only for a string of `INT_MAX` bytes or more; no channel's
        // name or description comes near.
        let need = self
            .write_with(write, &mut [])
            .expect("channel names are short");
        let mut buf = vec![0; need];
        self.write_with(write, &mut buf)
            .expect("channel names are short");
        CStr::from_bytes_until_nul(&buf)
            .expect("C NUL-terminates a non-empty buffer")
            .to_owned()
    }

    /// Wraps: av_channel_name
    ///
    /// Writes this channel's abbreviated name, as [`name`](Self::name)
    /// returns it, into `buf`, truncated to fit with its NUL, and returns the
    /// size a buffer needs to hold it whole, NUL included: the name was
    /// truncated when that exceeds `buf.len()`. An empty `buf` only measures
    /// the name. 16 bytes hold every name.
    ///
    /// # Errors
    ///
    /// C's negative `AVERROR` for a name of `INT_MAX` bytes or more, which no
    /// channel has.
    pub fn write_name(self, buf: &mut [u8]) -> Result<usize, i32> {
        self.write_with(ffi::av_channel_name, buf)
    }

    /// Wraps: av_channel_name
    ///
    /// This channel's abbreviated name, which [`from_name`](Self::from_name)
    /// maps back: `FL`, `LFE` and the like for the named channels, `AMBI<n>`
    /// for Ambisonic component `n`, `NONE`, `UNK` and `UNSD` for
    /// [`NONE`](Self::NONE), [`UNKNOWN`](Self::UNKNOWN) and
    /// [`UNUSED`](Self::UNUSED), and `USR<id>` for every other value.
    ///
    /// ```
    /// use ffmpeg::libavutil::channel_layout::Channel;
    /// assert_eq!(Channel::FRONT_LEFT.name().as_c_str(), c"FL");
    /// assert_eq!(Channel::ambisonic(3).unwrap().name().as_c_str(), c"AMBI3");
    /// ```
    #[must_use]
    pub fn name(self) -> CString {
        self.to_c_string(ffi::av_channel_name)
    }

    /// Wraps: av_channel_description
    ///
    /// Writes this channel's description, as
    /// [`description`](Self::description) returns it, into `buf`, as
    /// [`write_name`](Self::write_name) writes the name. 24 bytes hold every
    /// description.
    ///
    /// # Errors
    ///
    /// As for [`write_name`](Self::write_name).
    pub fn write_description(self, buf: &mut [u8]) -> Result<usize, i32> {
        self.write_with(ffi::av_channel_description, buf)
    }

    /// Wraps: av_channel_description
    ///
    /// A human-readable description of this channel: `front left` and the
    /// like for the named channels, `ambisonic ACN <n>` for Ambisonic
    /// component `n`, `none`, `unknown` and `unused` for
    /// [`NONE`](Self::NONE), [`UNKNOWN`](Self::UNKNOWN) and
    /// [`UNUSED`](Self::UNUSED), and `user <id>` for every other value.
    #[must_use]
    pub fn description(self) -> CString {
        self.to_c_string(ffi::av_channel_description)
    }

    /// Wraps: av_channel_name_bprint
    ///
    /// Appends this channel's abbreviated name, as [`name`](Self::name)
    /// returns it, to `buf`, truncated as `buf` truncates what does not fit.
    /// `buf` is a handle, which `as_mut()` reborrows from an owner or another
    /// handle.
    ///
    /// ```
    /// use ffmpeg::libavutil::bprint::BPrint;
    /// use ffmpeg::libavutil::channel_layout::Channel;
    /// let mut buf = BPrint::new(0, BPrint::SIZE_UNLIMITED).unwrap();
    /// Channel::FRONT_LEFT.append_name(buf.as_mut());
    /// buf.as_mut().append_bytes(b"+");
    /// Channel::LOW_FREQUENCY.append_name(buf.as_mut());
    /// assert_eq!(buf.as_ref().as_bytes(), b"FL+LFE");
    /// ```
    pub fn append_name(self, mut buf: BPrintMut<'_>) {
        // SAFETY: the exclusive handle addresses an initialised buffer,
        // which C appends to with `av_bprintf` within its `size`; C accepts
        // any channel value and keeps no pointer.
        unsafe { ffi::av_channel_name_bprint(buf.as_mut_ptr(), self.into_raw()) }
    }

    /// Wraps: av_channel_description_bprint
    ///
    /// Appends this channel's description, as
    /// [`description`](Self::description) returns it, to `buf`, truncated
    /// as `buf` truncates what does not fit.
    pub fn append_description(self, mut buf: BPrintMut<'_>) {
        // SAFETY: as in `append_name`.
        unsafe { ffi::av_channel_description_bprint(buf.as_mut_ptr(), self.into_raw()) }
    }

    /// Wraps: av_channel_from_string
    ///
    /// The channel `name` names, the inverse of [`name`](Self::name): an
    /// abbreviation such as `FL`, `UNK` or `UNSD`, `AMBI<n>` for Ambisonic
    /// component `n` (0 to 1023), or `USR<id>` for a non-negative `id`, both
    /// numbers read as C's `strtol` reads them (decimal, `0x` hex or `0`
    /// octal). `None` for any other name, which C reports as
    /// [`NONE`](Self::NONE), the one channel no name yields.
    ///
    /// C's parsing is lenient, and this follows it: text after an `AMBI`
    /// number is ignored, and `AMBI` with no number is component 0; `USR`
    /// with no number is channel 0; and a number beyond `i32` is truncated
    /// to an `int` before it is checked.
    pub fn from_name(name: &CStr) -> Option<Self> {
        // SAFETY: `name` is a live NUL-terminated string, which C only reads
        // during the call.
        let raw = unsafe { ffi::av_channel_from_string(name.as_ptr()) };
        Some(Self::from_raw(raw)).filter(|&ch| ch != Self::NONE)
    }
}

/// `AVERROR(EINVAL)`: C's code for an argument it refuses.
const EINVAL: i32 = -22;

/// The native-order masks of the standard channel layouts, C's
/// `AV_CH_LAYOUT_<NAME>` constants, each the bits ([`Channel::mask`]) of the
/// channels it holds, composed as the header composes them. A C name that
/// starts with a digit is not a Rust identifier, so those carry an `L`
/// (`AV_CH_LAYOUT_5POINT1` is [`masks::L5POINT1`]); the deprecated
/// `FF_API_CHANNEL_LAYOUT_BACK` aliases are left out. The layouts themselves
/// are [`ChannelLayout`]'s constants of the same names.
///
/// ```
/// use ffmpeg::libavutil::channel_layout::{ChannelLayout, LayoutDetails, masks};
/// let l = ChannelLayout::from_mask(masks::L5POINT1).unwrap();
/// assert_eq!(l.as_ref().channel_count(), 6);
/// assert!(matches!(l.as_ref().details(), LayoutDetails::Native(masks::L5POINT1)));
/// ```
pub mod masks {
    use super::Channel;

    /// The bit of a named channel, which every one of them has.
    const fn bit(ch: Channel) -> u64 {
        match ch.mask() {
            Some(bit) => bit,
            None => panic!("a named channel has a mask bit"),
        }
    }

    /// `AV_CH_LAYOUT_MONO`.
    pub const MONO: u64 = bit(Channel::FRONT_CENTER);
    /// `AV_CH_LAYOUT_STEREO`.
    pub const STEREO: u64 = bit(Channel::FRONT_LEFT) | bit(Channel::FRONT_RIGHT);
    /// `AV_CH_LAYOUT_2POINT1`.
    pub const L2POINT1: u64 = STEREO | bit(Channel::LOW_FREQUENCY);
    /// `AV_CH_LAYOUT_2_1`.
    pub const L2_1: u64 = STEREO | bit(Channel::BACK_CENTER);
    /// `AV_CH_LAYOUT_SURROUND`.
    pub const SURROUND: u64 = STEREO | bit(Channel::FRONT_CENTER);
    /// `AV_CH_LAYOUT_3POINT1`.
    pub const L3POINT1: u64 = SURROUND | bit(Channel::LOW_FREQUENCY);
    /// `AV_CH_LAYOUT_4POINT0`.
    pub const L4POINT0: u64 = SURROUND | bit(Channel::BACK_CENTER);
    /// `AV_CH_LAYOUT_4POINT1`.
    pub const L4POINT1: u64 = L4POINT0 | bit(Channel::LOW_FREQUENCY);
    /// `AV_CH_LAYOUT_2_2`.
    pub const L2_2: u64 = STEREO | bit(Channel::SIDE_LEFT) | bit(Channel::SIDE_RIGHT);
    /// `AV_CH_LAYOUT_QUAD`.
    pub const QUAD: u64 = STEREO | bit(Channel::BACK_LEFT) | bit(Channel::BACK_RIGHT);
    /// `AV_CH_LAYOUT_5POINT0`.
    pub const L5POINT0: u64 = SURROUND | bit(Channel::SIDE_LEFT) | bit(Channel::SIDE_RIGHT);
    /// `AV_CH_LAYOUT_5POINT1`.
    pub const L5POINT1: u64 = L5POINT0 | bit(Channel::LOW_FREQUENCY);
    /// `AV_CH_LAYOUT_5POINT0_BACK`.
    pub const L5POINT0_BACK: u64 = SURROUND | bit(Channel::BACK_LEFT) | bit(Channel::BACK_RIGHT);
    /// `AV_CH_LAYOUT_5POINT1_BACK`.
    pub const L5POINT1_BACK: u64 = L5POINT0_BACK | bit(Channel::LOW_FREQUENCY);
    /// `AV_CH_LAYOUT_6POINT0`.
    pub const L6POINT0: u64 = L5POINT0 | bit(Channel::BACK_CENTER);
    /// `AV_CH_LAYOUT_6POINT0_FRONT`.
    pub const L6POINT0_FRONT: u64 =
        L2_2 | bit(Channel::FRONT_LEFT_OF_CENTER) | bit(Channel::FRONT_RIGHT_OF_CENTER);
    /// `AV_CH_LAYOUT_HEXAGONAL`.
    pub const HEXAGONAL: u64 = L5POINT0_BACK | bit(Channel::BACK_CENTER);
    /// `AV_CH_LAYOUT_3POINT1POINT2`.
    pub const L3POINT1POINT2: u64 =
        L3POINT1 | bit(Channel::TOP_FRONT_LEFT) | bit(Channel::TOP_FRONT_RIGHT);
    /// `AV_CH_LAYOUT_6POINT1`.
    pub const L6POINT1: u64 = L5POINT1 | bit(Channel::BACK_CENTER);
    /// `AV_CH_LAYOUT_6POINT1_BACK`.
    pub const L6POINT1_BACK: u64 = L5POINT1_BACK | bit(Channel::BACK_CENTER);
    /// `AV_CH_LAYOUT_6POINT1_FRONT`.
    pub const L6POINT1_FRONT: u64 = L6POINT0_FRONT | bit(Channel::LOW_FREQUENCY);
    /// `AV_CH_LAYOUT_7POINT0`.
    pub const L7POINT0: u64 = L5POINT0 | bit(Channel::BACK_LEFT) | bit(Channel::BACK_RIGHT);
    /// `AV_CH_LAYOUT_7POINT0_FRONT`.
    pub const L7POINT0_FRONT: u64 =
        L5POINT0 | bit(Channel::FRONT_LEFT_OF_CENTER) | bit(Channel::FRONT_RIGHT_OF_CENTER);
    /// `AV_CH_LAYOUT_7POINT1`.
    pub const L7POINT1: u64 = L5POINT1 | bit(Channel::BACK_LEFT) | bit(Channel::BACK_RIGHT);
    /// `AV_CH_LAYOUT_7POINT1_WIDE`.
    pub const L7POINT1_WIDE: u64 =
        L5POINT1 | bit(Channel::FRONT_LEFT_OF_CENTER) | bit(Channel::FRONT_RIGHT_OF_CENTER);
    /// `AV_CH_LAYOUT_7POINT1_WIDE_BACK`.
    pub const L7POINT1_WIDE_BACK: u64 =
        L5POINT1_BACK | bit(Channel::FRONT_LEFT_OF_CENTER) | bit(Channel::FRONT_RIGHT_OF_CENTER);
    /// `AV_CH_LAYOUT_5POINT1POINT2`.
    pub const L5POINT1POINT2: u64 =
        L5POINT1 | bit(Channel::TOP_FRONT_LEFT) | bit(Channel::TOP_FRONT_RIGHT);
    /// `AV_CH_LAYOUT_5POINT1POINT2_BACK`.
    pub const L5POINT1POINT2_BACK: u64 =
        L5POINT1_BACK | bit(Channel::TOP_FRONT_LEFT) | bit(Channel::TOP_FRONT_RIGHT);
    /// `AV_CH_LAYOUT_OCTAGONAL`.
    pub const OCTAGONAL: u64 =
        L5POINT0 | bit(Channel::BACK_LEFT) | bit(Channel::BACK_CENTER) | bit(Channel::BACK_RIGHT);
    /// `AV_CH_LAYOUT_CUBE`.
    pub const CUBE: u64 = QUAD
        | bit(Channel::TOP_FRONT_LEFT)
        | bit(Channel::TOP_FRONT_RIGHT)
        | bit(Channel::TOP_BACK_LEFT)
        | bit(Channel::TOP_BACK_RIGHT);
    /// `AV_CH_LAYOUT_5POINT1POINT4`.
    pub const L5POINT1POINT4: u64 =
        L5POINT1POINT2 | bit(Channel::TOP_BACK_LEFT) | bit(Channel::TOP_BACK_RIGHT);
    /// `AV_CH_LAYOUT_7POINT1POINT2`.
    pub const L7POINT1POINT2: u64 =
        L7POINT1 | bit(Channel::TOP_FRONT_LEFT) | bit(Channel::TOP_FRONT_RIGHT);
    /// `AV_CH_LAYOUT_7POINT1POINT4`.
    pub const L7POINT1POINT4: u64 =
        L7POINT1POINT2 | bit(Channel::TOP_BACK_LEFT) | bit(Channel::TOP_BACK_RIGHT);
    /// `AV_CH_LAYOUT_7POINT2POINT3`.
    pub const L7POINT2POINT3: u64 =
        L7POINT1POINT2 | bit(Channel::TOP_BACK_CENTER) | bit(Channel::LOW_FREQUENCY_2);
    /// `AV_CH_LAYOUT_9POINT1POINT4`.
    pub const L9POINT1POINT4: u64 =
        L7POINT1POINT4 | bit(Channel::FRONT_LEFT_OF_CENTER) | bit(Channel::FRONT_RIGHT_OF_CENTER);
    /// `AV_CH_LAYOUT_9POINT1POINT6`.
    pub const L9POINT1POINT6: u64 =
        L9POINT1POINT4 | bit(Channel::TOP_SIDE_LEFT) | bit(Channel::TOP_SIDE_RIGHT);
    /// `AV_CH_LAYOUT_HEXADECAGONAL`.
    pub const HEXADECAGONAL: u64 = OCTAGONAL
        | bit(Channel::WIDE_LEFT)
        | bit(Channel::WIDE_RIGHT)
        | bit(Channel::TOP_BACK_LEFT)
        | bit(Channel::TOP_BACK_RIGHT)
        | bit(Channel::TOP_BACK_CENTER)
        | bit(Channel::TOP_FRONT_CENTER)
        | bit(Channel::TOP_FRONT_LEFT)
        | bit(Channel::TOP_FRONT_RIGHT);
    /// `AV_CH_LAYOUT_BINAURAL`.
    pub const BINAURAL: u64 = bit(Channel::BINAURAL_LEFT) | bit(Channel::BINAURAL_RIGHT);
    /// `AV_CH_LAYOUT_STEREO_DOWNMIX`.
    pub const STEREO_DOWNMIX: u64 = bit(Channel::STEREO_LEFT) | bit(Channel::STEREO_RIGHT);
    /// `AV_CH_LAYOUT_22POINT2`.
    pub const L22POINT2: u64 = L9POINT1POINT6
        | bit(Channel::BACK_CENTER)
        | bit(Channel::LOW_FREQUENCY_2)
        | bit(Channel::TOP_FRONT_CENTER)
        | bit(Channel::TOP_CENTER)
        | bit(Channel::TOP_BACK_CENTER)
        | bit(Channel::BOTTOM_FRONT_CENTER)
        | bit(Channel::BOTTOM_FRONT_LEFT)
        | bit(Channel::BOTTOM_FRONT_RIGHT);
}

define_ctype!(
    /// Wraps: AVChannelLayout
    ///
    /// The channel layout of audio data: how many channels there are and,
    /// by its [order](ChannelOrder), which ones in what order. Its getters
    /// are on [`ChannelLayoutRef`] (`as_ref()`), its setters on
    /// [`ChannelLayoutMut`] (`as_mut()`).
    ///
    /// `sizeof(AVChannelLayout)` is public ABI: C users keep one on the
    /// stack or embed it by value (`AVFrame.ch_layout`,
    /// `AVCodecContext.ch_layout`), and C copies it by assignment, so it is
    /// not address-sensitive. It owns a resource in custom order only: the
    /// `u.map` array of [`ChannelCustom`]s, an `av_malloc`-family block that
    /// `av_channel_layout_uninit` frees. So a Rust-held layout is a
    /// [`ChannelLayoutVal`], which disposes it on drop, and a layout embedded
    /// in a C struct is reached as a [`ChannelLayoutRef`] or
    /// [`ChannelLayoutMut`] over the projected field, disposed by its
    /// parent's teardown.
    ///
    /// Copying the bytes of a custom layout would make two owners of one map
    /// (C forbids assigning one layout to another), so a `ChannelLayout` is
    /// neither `Copy` nor `Clone`, and no safe function adopts a raw
    /// `AVChannelLayout`: copies go through `av_channel_layout_copy`
    /// ([`ChannelLayoutRef::try_clone`]), and only a map Rust owns
    /// ([`HeapVec`]) enters a layout.
    ///
    /// Every safe path keeps a layout in a state C's readers and
    /// `av_channel_layout_uninit` accept: in custom order, `u.map` is a
    /// block the layout owns holding `nb_channels >= 1` elements; in the
    /// other orders it holds no map, and an ambisonic layout's count is
    /// never negative. C's readers also take each map element's name for a C
    /// string; [`ChannelCustomMut`]'s setters always terminate it, while
    /// [`ChannelCustom::from_raw`] can carry 16 non-NUL bytes into a map,
    /// which wrappers of those readers must refuse.
    ChannelLayout,
    ChannelLayoutRef,
    ChannelLayoutMut,
    ffi::AVChannelLayout
);

// SAFETY: all-zero is unspecified order, no channels, a zero mask and a NULL
// `opaque`: the `{0}` initialisation the header documents, and what
// `av_channel_layout_uninit` leaves. Every accessor reads it as such,
// `av_channel_layout_uninit` frees nothing for it (it frees only in custom
// order), and every C routine the safe wrappers pass it to takes it.
unsafe impl CZeroable for ChannelLayout {}

// SAFETY: a layout is an order, a count, a mask or the pointer to a map it
// owns, and a user pointer. The map is an `av_malloc`-family block, which
// `av_free` releases on any thread, of `ChannelCustom`s, which are `Send`;
// the user pointer is a cookie neither FFmpeg nor this crate dereferences
// (see `ChannelCustom`'s `Send`). So a value, or an exclusive handle to one,
// moves between threads with everything it owns.
unsafe impl Send for ChannelLayout {}
// SAFETY: as for `ChannelLayout`: the handle is the only access to the
// layout and its map.
unsafe impl Send for ChannelLayoutMut<'_> {}

// SAFETY: nothing reachable through a shared handle writes:
// `ChannelLayoutRef`'s getters copy fields out or lend a `CSlice` of
// getter-only `ChannelCustomRef`s (`ChannelCustom` is `Sync`), `try_clone`
// only reads its source, and the struct has no interior mutability, so
// concurrent shared access reads unchanging bytes.
unsafe impl Sync for ChannelLayout {}
// SAFETY: as for `Sync for ChannelLayout`: copies of a shared handle only
// read.
unsafe impl Send for ChannelLayoutRef<'_> {}
// SAFETY: as above.
unsafe impl Sync for ChannelLayoutRef<'_> {}
// SAFETY: `&ChannelLayoutMut` reaches the layout only through `as_ref`, a
// shared handle; the setters need `&mut`.
unsafe impl Sync for ChannelLayoutMut<'_> {}

/// Wraps: av_channel_layout_uninit
///
/// The disposal policy of a [`ChannelLayoutVal`]: `av_channel_layout_uninit`
/// frees a custom layout's map and zeroes every field, leaving the storage,
/// Rust's own, in place.
///
/// Thread-agnostic: the map is released with `av_free`, which any thread may
/// call.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ChannelLayoutUninit;

impl_cdispose!(
    ChannelLayoutUninit,
    ChannelLayout,
    ffi::av_channel_layout_uninit
);

/// An [`AVChannelLayout`](ChannelLayout) Rust holds by value, disposed by
/// `av_channel_layout_uninit` on drop.
///
/// `as_ref()` and `as_mut()` hand out its [`ChannelLayoutRef`] and
/// [`ChannelLayoutMut`]; `into_inner()` gives up disposal, for moving the
/// layout into storage whose owner disposes it.
pub type ChannelLayoutVal = CVal<ChannelLayout, ChannelLayoutUninit>;

impl ChannelLayout {
    /// A layout of `raw`'s fields, whatever they are; the caller sees to it
    /// that any map in it is one the layout may own.
    const fn from_parts(raw: ffi::AVChannelLayout) -> Self {
        Self(raw, core::marker::PhantomData)
    }

    /// The raw layout of `order`, `nb_channels` and `mask` with no map, and
    /// the user pointer `opaque`.
    fn masked(
        order: ChannelOrder,
        nb_channels: c_int,
        mask: u64,
        opaque: *mut c_void,
    ) -> ffi::AVChannelLayout {
        ffi::AVChannelLayout {
            order: order.into_raw(),
            nb_channels,
            u: ffi::AVChannelLayout__bindgen_ty_1 { mask },
            opaque,
        }
    }

    /// A fresh zeroed layout initialised in place by `init`, a C initialiser
    /// returning a negative `AVERROR` on failure.
    fn init_with(
        init: impl FnOnce(*mut ffi::AVChannelLayout) -> c_int,
    ) -> Result<ChannelLayoutVal, i32> {
        let mut layout = ChannelLayoutVal::new(Self::zeroed());
        let ret = init(layout.as_mut().as_mut_ptr());
        // On failure C leaves no map the layout owns, or (out of memory in
        // `av_channel_layout_from_string`'s ambisonic path) a custom order
        // with a NULL map: `av_channel_layout_uninit` takes either when
        // `layout` drops.
        if ret < 0 { Err(ret) } else { Ok(layout) }
    }

    /// A layout of `nb_channels` channels in unspecified order, with no user
    /// pointer, as C users build one from `{0}`: only the count is known.
    /// `unspecified(0)` is the zeroed layout.
    pub fn unspecified(nb_channels: i32) -> ChannelLayoutVal {
        let raw = Self::masked(ChannelOrder::UNSPEC, nb_channels, 0, ptr::null_mut());
        ChannelLayoutVal::new(Self::from_parts(raw))
    }

    /// Wraps: av_channel_layout_custom_init
    ///
    /// A custom-order layout of `nb_channels` channels, each
    /// [`Channel::UNKNOWN`] and unnamed, to be designated through
    /// [`ChannelLayoutMut::custom_map_mut`].
    ///
    /// # Errors
    ///
    /// `AVERROR(EINVAL)` when `nb_channels` is not positive,
    /// `AVERROR(ENOMEM)` when the map cannot be allocated.
    pub fn custom(nb_channels: i32) -> Result<ChannelLayoutVal, i32> {
        // SAFETY: `l` addresses the fresh zeroed layout `init_with` owns, so
        // C's overwrite without uninit leaks nothing; on success it holds the
        // `av_calloc` map C stored, which the layout owns.
        Self::init_with(|l| unsafe { ffi::av_channel_layout_custom_init(l, nb_channels) })
    }

    /// Wraps: av_channel_layout_from_mask
    ///
    /// The native-order layout of the channels set in `mask`: bit `n` is
    /// the [`Channel`] of id `n`.
    ///
    /// # Errors
    ///
    /// `AVERROR(EINVAL)` for an empty mask.
    pub fn from_mask(mask: u64) -> Result<ChannelLayoutVal, i32> {
        // SAFETY: `l` addresses the fresh zeroed layout `init_with` owns; C
        // writes the order, count and mask, allocating nothing.
        Self::init_with(|l| unsafe { ffi::av_channel_layout_from_mask(l, mask) })
    }

    /// Wraps: av_channel_layout_from_string
    ///
    /// The layout `description` describes, in any of the forms C accepts: a
    /// standard layout name (`stereo`, `5.1(side)`), channel names joined by
    /// `+`, each optionally named after an `@` (`FL@left+FR@right+LFE`), a
    /// native mask in decimal or hex (`0x3`), a count with the default
    /// layout (`4c`), a count of unordered channels (`4C`, `4 channels`), or
    /// an ambisonic order with optional extra channels
    /// (`ambisonic 2+stereo`).
    ///
    /// ```
    /// use ffmpeg::libavutil::channel_layout::{ChannelLayout, ChannelOrder, LayoutDetails};
    /// let stereo = ChannelLayout::parse(c"stereo").unwrap();
    /// assert_eq!(stereo.as_ref().order(), ChannelOrder::NATIVE);
    /// assert_eq!(stereo.as_ref().channel_count(), 2);
    /// assert!(matches!(stereo.as_ref().details(), LayoutDetails::Native(0x3)));
    /// ```
    ///
    /// # Errors
    ///
    /// `AVERROR(EINVAL)` for a description C cannot parse,
    /// `AVERROR(ENOMEM)` when memory runs out.
    pub fn parse(description: &CStr) -> Result<ChannelLayoutVal, i32> {
        // SAFETY: `l` addresses the fresh zeroed layout `init_with` owns,
        // which C overwrites without uninit, leaking nothing; `description`
        // is a live C string C only reads during the call. On success any
        // map C stored is an `av_malloc`-family block the layout owns.
        Self::init_with(|l| unsafe { ffi::av_channel_layout_from_string(l, description.as_ptr()) })
    }

    /// Wraps: av_channel_layout_default
    ///
    /// The default layout of `nb_channels` channels: the first standard
    /// native layout with that many (`mono`, `stereo`, `2.1`, ...), or, for a
    /// count none has, the count in unspecified order.
    pub fn default_for(nb_channels: i32) -> ChannelLayoutVal {
        let mut layout = ChannelLayoutVal::new(Self::zeroed());
        // SAFETY: the pointer addresses the fresh zeroed layout `layout`
        // owns; C overwrites it with a copy of a static native layout, or
        // sets its order and count, storing no map either way.
        unsafe { ffi::av_channel_layout_default(layout.as_mut().as_mut_ptr(), nb_channels) };
        layout
    }

    /// A custom-order layout of the channels in `map`, which it takes over,
    /// with no user pointer; `map` comes back as the error when it is empty
    /// or longer than `i32::MAX`, which a layout cannot count.
    ///
    /// ```
    /// use ffmpeg::libavutil::channel_layout::{Channel, ChannelCustom, ChannelLayout};
    /// use ffmpeg::libavutil::mem::alloc_cell_array_zeroed;
    /// let mut map = alloc_cell_array_zeroed::<ChannelCustom>(2).unwrap();
    /// for (i, id) in [Channel::FRONT_LEFT, Channel::LOW_FREQUENCY].into_iter().enumerate() {
    ///     map.as_handles_mut().get_mut(i).unwrap().set_id(id);
    /// }
    /// let layout = ChannelLayout::from_custom_map(map).unwrap();
    /// assert_eq!(layout.as_ref().channel_count(), 2);
    /// ```
    pub fn from_custom_map(
        map: HeapVec<ChannelCustom>,
    ) -> Result<ChannelLayoutVal, HeapVec<ChannelCustom>> {
        let mut layout = Self::unspecified(0);
        layout.as_mut().set_custom_map(map)?;
        Ok(layout)
    }
}

/// The standard layouts, C's `AV_CHANNEL_LAYOUT_<NAME>` initialisers: the
/// native layout of each [`masks`] constant, under the same name, and the
/// first-order Ambisonic layout. Each is a bare value owning nothing;
/// [`ChannelLayoutVal::from`] (or `.into()`) makes it an owner, which
/// [`ChannelLayoutMut::set`] and its siblings take.
///
/// ```
/// use ffmpeg::libavutil::channel_layout::{ChannelLayout, ChannelLayoutVal, ChannelOrder};
/// assert_eq!(ChannelLayout::L5POINT1.as_ref().channel_count(), 6);
/// let mut l = ChannelLayoutVal::from(ChannelLayout::STEREO);
/// l.as_mut().set(ChannelLayout::AMBISONIC_FIRST_ORDER.into());
/// assert_eq!(l.as_ref().order(), ChannelOrder::AMBISONIC);
/// assert_eq!(l.as_ref().channel_count(), 4);
/// ```
impl ChannelLayout {
    /// The native-order layout of the channels set in `mask`, counting
    /// them: what C's `AV_CHANNEL_LAYOUT_MASK(nb, m)` initialises for a
    /// consistent `nb` (the only way the header and FFmpeg use it), and what
    /// [`from_mask`](Self::from_mask) builds, with no error for an empty
    /// mask, which leaves no channels.
    #[must_use]
    pub const fn native(mask: u64) -> Self {
        Self::from_parts(ffi::AVChannelLayout {
            order: ChannelOrder::NATIVE.into_raw(),
            nb_channels: mask.count_ones() as c_int,
            u: ffi::AVChannelLayout__bindgen_ty_1 { mask },
            opaque: ptr::null_mut(),
        })
    }

    /// `AV_CHANNEL_LAYOUT_MONO`: the native layout of [`masks::MONO`].
    pub const MONO: Self = Self::native(masks::MONO);
    /// `AV_CHANNEL_LAYOUT_STEREO`: the native layout of [`masks::STEREO`].
    pub const STEREO: Self = Self::native(masks::STEREO);
    /// `AV_CHANNEL_LAYOUT_2POINT1`: the native layout of [`masks::L2POINT1`].
    pub const L2POINT1: Self = Self::native(masks::L2POINT1);
    /// `AV_CHANNEL_LAYOUT_2_1`: the native layout of [`masks::L2_1`].
    pub const L2_1: Self = Self::native(masks::L2_1);
    /// `AV_CHANNEL_LAYOUT_SURROUND`: the native layout of [`masks::SURROUND`].
    pub const SURROUND: Self = Self::native(masks::SURROUND);
    /// `AV_CHANNEL_LAYOUT_3POINT1`: the native layout of [`masks::L3POINT1`].
    pub const L3POINT1: Self = Self::native(masks::L3POINT1);
    /// `AV_CHANNEL_LAYOUT_4POINT0`: the native layout of [`masks::L4POINT0`].
    pub const L4POINT0: Self = Self::native(masks::L4POINT0);
    /// `AV_CHANNEL_LAYOUT_4POINT1`: the native layout of [`masks::L4POINT1`].
    pub const L4POINT1: Self = Self::native(masks::L4POINT1);
    /// `AV_CHANNEL_LAYOUT_2_2`: the native layout of [`masks::L2_2`].
    pub const L2_2: Self = Self::native(masks::L2_2);
    /// `AV_CHANNEL_LAYOUT_QUAD`: the native layout of [`masks::QUAD`].
    pub const QUAD: Self = Self::native(masks::QUAD);
    /// `AV_CHANNEL_LAYOUT_5POINT0`: the native layout of [`masks::L5POINT0`].
    pub const L5POINT0: Self = Self::native(masks::L5POINT0);
    /// `AV_CHANNEL_LAYOUT_5POINT1`: the native layout of [`masks::L5POINT1`].
    pub const L5POINT1: Self = Self::native(masks::L5POINT1);
    /// `AV_CHANNEL_LAYOUT_5POINT0_BACK`: the native layout of [`masks::L5POINT0_BACK`].
    pub const L5POINT0_BACK: Self = Self::native(masks::L5POINT0_BACK);
    /// `AV_CHANNEL_LAYOUT_5POINT1_BACK`: the native layout of [`masks::L5POINT1_BACK`].
    pub const L5POINT1_BACK: Self = Self::native(masks::L5POINT1_BACK);
    /// `AV_CHANNEL_LAYOUT_6POINT0`: the native layout of [`masks::L6POINT0`].
    pub const L6POINT0: Self = Self::native(masks::L6POINT0);
    /// `AV_CHANNEL_LAYOUT_6POINT0_FRONT`: the native layout of [`masks::L6POINT0_FRONT`].
    pub const L6POINT0_FRONT: Self = Self::native(masks::L6POINT0_FRONT);
    /// `AV_CHANNEL_LAYOUT_HEXAGONAL`: the native layout of [`masks::HEXAGONAL`].
    pub const HEXAGONAL: Self = Self::native(masks::HEXAGONAL);
    /// `AV_CHANNEL_LAYOUT_3POINT1POINT2`: the native layout of [`masks::L3POINT1POINT2`].
    pub const L3POINT1POINT2: Self = Self::native(masks::L3POINT1POINT2);
    /// `AV_CHANNEL_LAYOUT_6POINT1`: the native layout of [`masks::L6POINT1`].
    pub const L6POINT1: Self = Self::native(masks::L6POINT1);
    /// `AV_CHANNEL_LAYOUT_6POINT1_BACK`: the native layout of [`masks::L6POINT1_BACK`].
    pub const L6POINT1_BACK: Self = Self::native(masks::L6POINT1_BACK);
    /// `AV_CHANNEL_LAYOUT_6POINT1_FRONT`: the native layout of [`masks::L6POINT1_FRONT`].
    pub const L6POINT1_FRONT: Self = Self::native(masks::L6POINT1_FRONT);
    /// `AV_CHANNEL_LAYOUT_7POINT0`: the native layout of [`masks::L7POINT0`].
    pub const L7POINT0: Self = Self::native(masks::L7POINT0);
    /// `AV_CHANNEL_LAYOUT_7POINT0_FRONT`: the native layout of [`masks::L7POINT0_FRONT`].
    pub const L7POINT0_FRONT: Self = Self::native(masks::L7POINT0_FRONT);
    /// `AV_CHANNEL_LAYOUT_7POINT1`: the native layout of [`masks::L7POINT1`].
    pub const L7POINT1: Self = Self::native(masks::L7POINT1);
    /// `AV_CHANNEL_LAYOUT_7POINT1_WIDE`: the native layout of [`masks::L7POINT1_WIDE`].
    pub const L7POINT1_WIDE: Self = Self::native(masks::L7POINT1_WIDE);
    /// `AV_CHANNEL_LAYOUT_7POINT1_WIDE_BACK`: the native layout of [`masks::L7POINT1_WIDE_BACK`].
    pub const L7POINT1_WIDE_BACK: Self = Self::native(masks::L7POINT1_WIDE_BACK);
    /// `AV_CHANNEL_LAYOUT_5POINT1POINT2`: the native layout of [`masks::L5POINT1POINT2`].
    pub const L5POINT1POINT2: Self = Self::native(masks::L5POINT1POINT2);
    /// `AV_CHANNEL_LAYOUT_5POINT1POINT2_BACK`: the native layout of [`masks::L5POINT1POINT2_BACK`].
    pub const L5POINT1POINT2_BACK: Self = Self::native(masks::L5POINT1POINT2_BACK);
    /// `AV_CHANNEL_LAYOUT_OCTAGONAL`: the native layout of [`masks::OCTAGONAL`].
    pub const OCTAGONAL: Self = Self::native(masks::OCTAGONAL);
    /// `AV_CHANNEL_LAYOUT_CUBE`: the native layout of [`masks::CUBE`].
    pub const CUBE: Self = Self::native(masks::CUBE);
    /// `AV_CHANNEL_LAYOUT_5POINT1POINT4`: the native layout of [`masks::L5POINT1POINT4`].
    pub const L5POINT1POINT4: Self = Self::native(masks::L5POINT1POINT4);
    /// `AV_CHANNEL_LAYOUT_7POINT1POINT2`: the native layout of [`masks::L7POINT1POINT2`].
    pub const L7POINT1POINT2: Self = Self::native(masks::L7POINT1POINT2);
    /// `AV_CHANNEL_LAYOUT_7POINT1POINT4`: the native layout of [`masks::L7POINT1POINT4`].
    pub const L7POINT1POINT4: Self = Self::native(masks::L7POINT1POINT4);
    /// `AV_CHANNEL_LAYOUT_7POINT2POINT3`: the native layout of [`masks::L7POINT2POINT3`].
    pub const L7POINT2POINT3: Self = Self::native(masks::L7POINT2POINT3);
    /// `AV_CHANNEL_LAYOUT_9POINT1POINT4`: the native layout of [`masks::L9POINT1POINT4`].
    pub const L9POINT1POINT4: Self = Self::native(masks::L9POINT1POINT4);
    /// `AV_CHANNEL_LAYOUT_9POINT1POINT6`: the native layout of [`masks::L9POINT1POINT6`].
    pub const L9POINT1POINT6: Self = Self::native(masks::L9POINT1POINT6);
    /// `AV_CHANNEL_LAYOUT_HEXADECAGONAL`: the native layout of [`masks::HEXADECAGONAL`].
    pub const HEXADECAGONAL: Self = Self::native(masks::HEXADECAGONAL);
    /// `AV_CHANNEL_LAYOUT_BINAURAL`: the native layout of [`masks::BINAURAL`].
    pub const BINAURAL: Self = Self::native(masks::BINAURAL);
    /// `AV_CHANNEL_LAYOUT_STEREO_DOWNMIX`: the native layout of [`masks::STEREO_DOWNMIX`].
    pub const STEREO_DOWNMIX: Self = Self::native(masks::STEREO_DOWNMIX);
    /// `AV_CHANNEL_LAYOUT_22POINT2`: the native layout of [`masks::L22POINT2`].
    pub const L22POINT2: Self = Self::native(masks::L22POINT2);
    /// `AV_CHANNEL_LAYOUT_AMBISONIC_FIRST_ORDER`: the four first-order
    /// Ambisonic components and no non-diegetic channel.
    pub const AMBISONIC_FIRST_ORDER: Self = Self::from_parts(ffi::AVChannelLayout {
        order: ChannelOrder::AMBISONIC.into_raw(),
        nb_channels: 4,
        u: ffi::AVChannelLayout__bindgen_ty_1 { mask: 0 },
        opaque: ptr::null_mut(),
    });
}

/// Takes a bare layout, such as one of [`ChannelLayout`]'s standard
/// constants, into an owner that disposes it.
impl From<ChannelLayout> for ChannelLayoutVal {
    #[inline]
    fn from(layout: ChannelLayout) -> Self {
        Self::new(layout)
    }
}

/// A layout's [order](ChannelOrder) and the `u` member it selects, as
/// [`ChannelLayoutRef::details`] reads them.
#[derive(Clone, Copy)]
pub enum LayoutDetails<'a> {
    /// [`ChannelOrder::UNSPEC`]: only the count is known; C leaves `u`
    /// undefined.
    Unspecified,
    /// [`ChannelOrder::NATIVE`]: `u.mask`, whose bit `n` marks the channel
    /// of id `n`, present in id order.
    Native(u64),
    /// [`ChannelOrder::CUSTOM`]: `u.map`, the channels one by one, as many
    /// as the layout counts. Empty for a map C left NULL (after a failed
    /// `av_channel_layout_copy`) or a negative count, which no safe path
    /// writes.
    Custom(CSlice<'a, ChannelCustom>),
    /// [`ChannelOrder::AMBISONIC`]: `u.mask`, the non-diegetic channels
    /// that follow the ambisonic components, as in native order.
    Ambisonic(u64),
    /// An order C does not know; its readers do not read `u`.
    Unknown(ChannelOrder),
}

impl fmt::Debug for LayoutDetails<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unspecified => f.write_str("Unspecified"),
            Self::Native(mask) => f
                .debug_tuple("Native")
                .field(&format_args!("{mask:#x}"))
                .finish(),
            Self::Custom(map) => f
                .debug_tuple("Custom")
                .field(&format_args!("{:?}", DebugMap(*map)))
                .finish(),
            Self::Ambisonic(mask) => f
                .debug_tuple("Ambisonic")
                .field(&format_args!("{mask:#x}"))
                .finish(),
            Self::Unknown(order) => f.debug_tuple("Unknown").field(order).finish(),
        }
    }
}

/// A custom map's elements, listed for [`LayoutDetails`]'s `Debug`.
struct DebugMap<'a>(CSlice<'a, ChannelCustom>);

impl fmt::Debug for DebugMap<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.0.iter()).finish()
    }
}

impl fmt::Debug for ChannelLayout {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_ref().fmt(f)
    }
}

impl fmt::Debug for ChannelLayoutRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ChannelLayout")
            .field("order", &self.order())
            .field("nb_channels", &self.channel_count())
            .field("details", &self.details())
            .field("opaque", &self.opaque())
            .finish()
    }
}

impl<'a> ChannelLayoutRef<'a> {
    /// Field: AVChannelLayout.order
    ///
    /// How the layout describes its channels, any value C stored included.
    #[must_use]
    pub fn order(&self) -> ChannelOrder {
        // SAFETY: a read of the `order` field projected through the shared
        // handle's pointer to a live, initialised `AVChannelLayout`;
        // `ChannelOrder` accepts every `unsigned`.
        ChannelOrder::from_raw(unsafe { addr_of!((*self.as_ptr()).order).read() })
    }

    /// Field: AVChannelLayout.nb_channels
    ///
    /// The number of channels, as C stores it: never negative on a layout
    /// a safe path built, except one built [unspecified](ChannelLayout::unspecified)
    /// or by [`default_for`](ChannelLayout::default_for) from a negative
    /// count.
    #[must_use]
    pub fn channel_count(&self) -> i32 {
        // SAFETY: a read of the `nb_channels` field projected through the
        // shared handle's pointer to a live, initialised `AVChannelLayout`.
        unsafe { addr_of!((*self.as_ptr()).nb_channels).read() }
    }

    /// The map pointer and the count of a custom-order layout.
    fn map_parts(&self) -> Option<(*mut ffi::AVChannelCustom, c_int)> {
        if self.order() != ChannelOrder::CUSTOM {
            return None;
        }
        // SAFETY: a read of the `u.map` union member projected through the
        // shared handle's pointer; a custom-order layout stores `map` there,
        // and the pointer is copied, not dereferenced.
        let map = unsafe { addr_of!((*self.as_ptr()).u.map).read() };
        Some((map, self.channel_count()))
    }

    /// Field: AVChannelLayout.u
    /// Field: AVChannelLayout.u.mask
    /// Field: AVChannelLayout.u.map
    ///
    /// The `u` member the order selects: the mask of a native or ambisonic
    /// layout, or the map of a custom one, borrowed as long as this handle's
    /// layout.
    ///
    /// ```
    /// use ffmpeg::libavutil::channel_layout::{Channel, ChannelLayout, LayoutDetails};
    /// let layout = ChannelLayout::parse(c"FL@left+LFE").unwrap();
    /// let LayoutDetails::Custom(map) = layout.as_ref().details() else { panic!() };
    /// assert_eq!(map.get(0).unwrap().name().as_c_str(), c"left");
    /// assert_eq!(map.get(1).unwrap().id(), Channel::LOW_FREQUENCY);
    /// ```
    #[must_use]
    pub fn details(&self) -> LayoutDetails<'a> {
        let order = self.order();
        let mask = || {
            // SAFETY: a read of the `u.mask` union member projected through
            // the shared handle's pointer; a native or ambisonic layout
            // stores the mask there, and every bit pattern is a `u64`.
            unsafe { addr_of!((*self.as_ptr()).u.mask).read() }
        };
        match order {
            ChannelOrder::UNSPEC => LayoutDetails::Unspecified,
            ChannelOrder::NATIVE => LayoutDetails::Native(mask()),
            ChannelOrder::AMBISONIC => LayoutDetails::Ambisonic(mask()),
            ChannelOrder::CUSTOM => {
                let (map, count) = self.map_parts().expect("the order is custom");
                let run = match (NonNull::new(map), usize::try_from(count)) {
                    (Some(map), Ok(len)) => {
                        // SAFETY: a custom layout's non-NULL map holds at
                        // least `nb_channels` initialised elements, owned by
                        // the layout and freed only by its uninit, which
                        // needs exclusive access, so they outlive `'a`;
                        // `ChannelCustom` is transparent over
                        // `AVChannelCustom`.
                        unsafe { CSlice::from_raw_parts(map.cast(), len) }
                    }
                    // SAFETY: an empty run reads no element; a dangling,
                    // aligned pointer is a valid base for it.
                    _ => unsafe { CSlice::from_raw_parts(NonNull::dangling(), 0) },
                };
                LayoutDetails::Custom(run)
            }
            _ => LayoutDetails::Unknown(order),
        }
    }

    /// Field: AVChannelLayout.opaque
    ///
    /// The user pointer, NULL unless a user stored one. FFmpeg never reads
    /// what it points at or frees it: `av_channel_layout_copy` copies the
    /// address, `av_channel_layout_retype` keeps it and
    /// `av_channel_layout_uninit` forgets it. So it is handed back as the raw
    /// cookie it is, and dereferencing it is the caller's `unsafe` business.
    #[must_use]
    pub fn opaque(&self) -> *mut c_void {
        // SAFETY: a read of the `opaque` field projected through the shared
        // handle's pointer to a live, initialised `AVChannelLayout`; the
        // pointer is copied, not dereferenced.
        unsafe { addr_of!((*self.as_ptr()).opaque).read() }
    }

    /// Wraps: av_channel_layout_copy
    ///
    /// A deep copy of this layout: a custom map is duplicated, the user
    /// pointer copied by address.
    ///
    /// # Errors
    ///
    /// `AVERROR(ENOMEM)` when the map cannot be allocated, and, as
    /// [`ChannelLayoutMut::copy_from`] describes, `AVERROR(EINVAL)` for a
    /// custom layout C left without a map.
    pub fn try_clone(&self) -> Result<ChannelLayoutVal, i32> {
        let mut copy = ChannelLayout::unspecified(0);
        copy.as_mut().copy_from(*self)?;
        Ok(copy)
    }
}

impl<'a> ChannelLayoutMut<'a> {
    /// Field: AVChannelLayout.opaque
    ///
    /// Stores the user pointer `opaque`, NULL included. Nothing in FFmpeg or
    /// this crate dereferences or frees it, so any address is safe to store:
    /// keeping what it points at alive for as long as the caller means to
    /// read it back is the caller's concern.
    // The lint reads the write of `opaque` as a dereference of it: the
    // pointer is the value stored, through the handle's own pointer.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn set_opaque(&mut self, opaque: *mut c_void) {
        // SAFETY: a write of the `opaque` field projected through the
        // exclusive handle's pointer to a live `AVChannelLayout`; the old
        // pointer is a cookie nothing frees.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).opaque).write(opaque) }
    }

    /// Field: AVChannelLayout.u.map
    ///
    /// The channels of a custom-order layout, for designating and naming
    /// them in place; `None` in any other order. The count, and so the map's
    /// length, stays as it is.
    #[must_use]
    pub fn custom_map_mut(&mut self) -> Option<CSliceMut<'_, ChannelCustom>> {
        self.as_mut().into_custom_map_mut()
    }

    /// Field: AVChannelLayout.u.map
    ///
    /// The channels of a custom-order layout, exclusively borrowed for the
    /// handle's whole lifetime, giving the handle up: the by-value
    /// counterpart of [`custom_map_mut`](Self::custom_map_mut), for a run
    /// that outlives a temporary handle (`l.as_mut().into_custom_map_mut()`).
    #[must_use]
    pub fn into_custom_map_mut(self) -> Option<CSliceMut<'a, ChannelCustom>> {
        let (map, count) = self.as_ref().map_parts()?;
        Some(match (NonNull::new(map), usize::try_from(count)) {
            (Some(map), Ok(len)) => {
                // SAFETY: as in `ChannelLayoutRef::details`; the handle is
                // consumed, so the run is the only access left to the
                // elements for `'a`, and nothing frees the map meanwhile.
                unsafe { CSliceMut::from_raw_parts(map.cast(), len) }
            }
            // SAFETY: an empty run reaches no element.
            _ => unsafe { CSliceMut::from_raw_parts(NonNull::dangling(), 0) },
        })
    }

    /// Overwrites the layout with `layout` and hands the previous one back,
    /// which then owns the previous map, if any.
    pub fn replace(&mut self, layout: ChannelLayoutVal) -> ChannelLayoutVal {
        let new = layout.into_inner().0;
        // SAFETY: the exclusive handle covers a live, initialised
        // `AVChannelLayout`, read out and overwritten in one step; ownership
        // of the old map moves to the returned owner, that of the new one
        // into the layout, so neither is freed or leaked twice.
        let old = unsafe { ptr::replace(self.as_mut_ptr(), new) };
        ChannelLayoutVal::new(ChannelLayout::from_parts(old))
    }

    /// Overwrites the layout with `layout`, disposing of the previous one.
    pub fn set(&mut self, layout: ChannelLayoutVal) {
        drop(self.replace(layout));
    }

    /// Hands the layout over, leaving the zeroed one in its place: the
    /// owning counterpart of `as_ref`, as `mem::take`.
    pub fn take(&mut self) -> ChannelLayoutVal {
        self.replace(ChannelLayout::unspecified(0))
    }

    /// Replaces the layout with the raw `order`, count and mask, keeping
    /// the user pointer, and disposes of the previous one, as
    /// `av_channel_layout_retype` does when it leaves custom order.
    fn reset_to(&mut self, order: ChannelOrder, nb_channels: c_int, mask: u64) {
        let opaque = self.as_ref().opaque();
        let raw = ChannelLayout::masked(order, nb_channels, mask, opaque);
        self.set(ChannelLayoutVal::new(ChannelLayout::from_parts(raw)));
    }

    /// Field: AVChannelLayout.order
    /// Field: AVChannelLayout.nb_channels
    ///
    /// Makes this a layout of `nb_channels` channels in unspecified order,
    /// freeing a custom map and keeping the user pointer.
    pub fn set_unspecified(&mut self, nb_channels: i32) {
        self.reset_to(ChannelOrder::UNSPEC, nb_channels, 0);
    }

    /// Field: AVChannelLayout.order
    /// Field: AVChannelLayout.nb_channels
    /// Field: AVChannelLayout.u.mask
    ///
    /// Makes this the native-order layout of the channels set in `mask`,
    /// counting them, as `av_channel_layout_from_mask` does; frees a custom
    /// map and keeps the user pointer. An empty mask, which
    /// [`ChannelLayout::from_mask`] refuses, leaves no channels.
    pub fn set_native(&mut self, mask: u64) {
        self.reset_to(ChannelOrder::NATIVE, mask.count_ones() as c_int, mask);
    }

    /// Field: AVChannelLayout.order
    /// Field: AVChannelLayout.nb_channels
    /// Field: AVChannelLayout.u.mask
    ///
    /// Makes this the layout of the `(order + 1)^2` ambisonic components of
    /// `order`, followed by the non-diegetic channels set in `non_diegetic`,
    /// as [`ChannelLayout::parse`] builds `ambisonic <order>+<mask>`; frees a
    /// custom map and keeps the user pointer.
    ///
    /// # Errors
    ///
    /// `AVERROR(EINVAL)`, leaving the layout as it was, when the count
    /// overflows the bounds C's parser sets: `(order + 1)^2` at most
    /// `i32::MAX`, and the total below it.
    pub fn set_ambisonic(&mut self, order: u32, non_diegetic: u64) -> Result<(), i32> {
        let n = i64::from(order) + 1;
        let extra = i64::from(non_diegetic.count_ones());
        let max = i64::from(c_int::MAX);
        if n > max / n || extra >= max - n * n {
            return Err(EINVAL);
        }
        let count = c_int::try_from(n * n + extra).map_err(|_| EINVAL)?;
        self.reset_to(ChannelOrder::AMBISONIC, count, non_diegetic);
        Ok(())
    }

    /// Field: AVChannelLayout.u.map
    ///
    /// Makes this the custom-order layout of the channels in `map`, which it
    /// takes over, freeing the previous map and keeping the user pointer.
    /// `map` comes back as the error, the layout left as it was, when it is
    /// empty (C's `av_channel_layout_custom_init` refuses no channels too)
    /// or longer than `i32::MAX`.
    pub fn set_custom_map(
        &mut self,
        map: HeapVec<ChannelCustom>,
    ) -> Result<(), HeapVec<ChannelCustom>> {
        let Some(count) = c_int::try_from(map.len()).ok().filter(|&n| n > 0) else {
            return Err(map);
        };
        let opaque = self.as_ref().opaque();
        let (map, _) = map.into_raw_parts();
        let raw = ffi::AVChannelLayout {
            order: ChannelOrder::CUSTOM.into_raw(),
            nb_channels: count,
            u: ffi::AVChannelLayout__bindgen_ty_1 { map: map.cast() },
            opaque,
        };
        // The layout now owns the `av_malloc`-family block of `count`
        // initialised elements, which `av_channel_layout_uninit` frees.
        self.set(ChannelLayoutVal::new(ChannelLayout::from_parts(raw)));
        Ok(())
    }

    /// Field: AVChannelLayout.u.map
    ///
    /// Takes a custom layout's map over, leaving the count in unspecified
    /// order and the user pointer, as `av_channel_layout_retype` leaves a
    /// custom layout it makes unspecified; `None`, the layout untouched, in
    /// any other order. The map is empty for a negative count, and `None`
    /// is also returned, the layout still made unspecified, for a map C left
    /// NULL.
    pub fn take_custom_map(&mut self) -> Option<HeapVec<ChannelCustom>> {
        let (map, count) = self.as_ref().map_parts()?;
        let opaque = self.as_ref().opaque();
        let raw = ChannelLayout::masked(ChannelOrder::UNSPEC, count, 0, opaque);
        // SAFETY: the exclusive handle covers a live `AVChannelLayout`; the
        // write replaces the map pointer, whose ownership moves to the
        // vector built below, by a layout that owns nothing.
        unsafe { self.as_mut_ptr().write(raw) };
        // SAFETY: a custom layout's non-NULL map is an `av_malloc`-family
        // block it owned (and owns no more), holding at least `nb_channels`
        // initialised elements; `Heap` frees it with `av_free` whatever its
        // capacity, and `ChannelCustom` is transparent over
        // `AVChannelCustom`.
        unsafe { HeapVec::from_raw_parts(map.cast(), usize::try_from(count).unwrap_or(0)) }
    }

    /// Wraps: av_channel_layout_uninit
    ///
    /// Frees a custom layout's map and zeroes every field, the user pointer
    /// included, leaving the empty unspecified layout.
    pub fn clear(&mut self) {
        // SAFETY: the exclusive handle covers a live layout in a state
        // `av_channel_layout_uninit` accepts, as every safe path leaves one;
        // it frees a custom map the layout owns and zeroes the fields.
        unsafe { ffi::av_channel_layout_uninit(self.as_mut_ptr()) }
    }

    /// Wraps: av_channel_layout_copy
    ///
    /// Overwrites this layout with a deep copy of `src`, disposing of the
    /// previous one first: a custom map is duplicated, the user pointer
    /// copied by address.
    ///
    /// # Errors
    ///
    /// `AVERROR(ENOMEM)` when the map cannot be allocated, which leaves this
    /// layout cleared (as [`clear`](Self::clear) does) rather than in the
    /// mapless custom state C leaves. `AVERROR(EINVAL)`, before C is called
    /// and with this layout untouched, for a custom `src` whose map C left
    /// NULL (after its own failed copy), which C would read through.
    pub fn copy_from(&mut self, src: ChannelLayoutRef<'_>) -> Result<(), i32> {
        if src.map_parts().is_some_and(|(map, _)| map.is_null()) {
            return Err(EINVAL);
        }
        // SAFETY: `self` and `src` cover live layouts that cannot be the same
        // (one borrow is exclusive), each in a state C accepts; C uninits
        // `self`, freeing its map, copies `src`'s fields and, for a custom
        // `src`, its non-NULL map of `nb_channels` elements into a fresh
        // `av_malloc_array` block `self` then owns.
        let ret = unsafe { ffi::av_channel_layout_copy(self.as_mut_ptr(), src.as_ptr()) };
        if ret < 0 {
            // C leaves a custom order with a NULL map behind, which its
            // readers would dereference.
            self.clear();
            return Err(ret);
        }
        Ok(())
    }
}

/// `AV_CHANNEL_LAYOUT_RETYPE_FLAG_LOSSLESS`: refuse a lossy retype.
const RETYPE_LOSSLESS: c_int = ffi::AV_CHANNEL_LAYOUT_RETYPE_FLAG_LOSSLESS as c_int;
/// `AV_CHANNEL_LAYOUT_RETYPE_FLAG_CANONICAL`: retype to the simplest order
/// that keeps every designation and name, ignoring the order asked for.
const RETYPE_CANONICAL: c_int = ffi::AV_CHANNEL_LAYOUT_RETYPE_FLAG_CANONICAL as c_int;

/// Whether [`ChannelLayoutMut::retype`] kept every channel designation and
/// name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Conversion {
    /// Nothing was lost, or the layout already had the order asked for.
    Lossless,
    /// The new order cannot hold the channel names, or the designations, of
    /// the old one, which were dropped.
    Lossy,
}

impl ChannelLayoutRef<'_> {
    /// Wraps: av_channel_layout_subset
    ///
    /// The channels of `mask` (bit `n` the [`Channel`] of id `n`, as in
    /// [`masks`]) that the layout holds, wherever it holds them: `mask`
    /// intersected with a native or ambisonic layout's mask, the bits of the
    /// ids a custom map designates among its channels, and none for an
    /// unspecified layout or an order C does not know.
    ///
    /// ```
    /// use ffmpeg::libavutil::channel_layout::{Channel, ChannelLayout, masks};
    /// let l = ChannelLayout::parse(c"LFE@sub+FL+USR40").unwrap();
    /// let front_right = Channel::FRONT_RIGHT.mask().unwrap();
    /// assert_eq!(l.as_ref().subset(masks::L2POINT1), masks::L2POINT1 ^ front_right);
    /// assert_eq!(ChannelLayout::STEREO.as_ref().subset(u64::MAX), masks::STEREO);
    /// ```
    #[must_use]
    pub fn subset(&self, mask: u64) -> u64 {
        // C walks a custom map with no NULL check; a custom layout C left
        // without one (after its own failed copy, which no safe path keeps)
        // holds no channel.
        if self.map_parts().is_some_and(|(map, _)| map.is_null()) {
            return 0;
        }
        // SAFETY: the shared handle covers a live layout that nothing writes
        // while it lives, and C only reads it: the mask of a native or
        // ambisonic layout, or, in custom order, the ids of the `nb_channels`
        // initialised elements of its non-NULL map (checked above), which
        // `av_channel_layout_index_from_channel` compares.
        unsafe { ffi::av_channel_layout_subset(self.as_ptr(), mask) }
    }
}

impl ChannelLayoutMut<'_> {
    /// `av_channel_layout_retype` of this layout to `order` under `flags`:
    /// C's non-negative result, or its `AVERROR`.
    fn retype_with(&mut self, order: ChannelOrder, flags: c_int) -> Result<c_int, i32> {
        // C refuses what `av_channel_layout_check` rejects (a NULL custom
        // map included) before reading anything else, and leaves the layout
        // as it was on every failure. Success keeps the user pointer and
        // either frees a custom map through `av_channel_layout_uninit`,
        // leaving none, or replaces the layout by a custom one owning a fresh
        // `av_calloc` map of `nb_channels >= 1` elements, which that frees.
        let layout = self.as_mut_ptr();
        // SAFETY: the exclusive handle covers a live layout in a state C's
        // readers accept, as every safe path leaves one; past the check C
        // reads at most the ids and first name bytes of the map's
        // `nb_channels` elements, and it leaves a layout owning its map, if
        // any, as described above.
        let ret = unsafe { ffi::av_channel_layout_retype(layout, order.into_raw(), flags) };
        if ret < 0 { Err(ret) } else { Ok(ret) }
    }

    /// Wraps: av_channel_layout_retype
    ///
    /// Changes the layout's order to `order`, keeping its channel count and
    /// user pointer, and reports whether channel names or designations were
    /// lost on the way. C converts
    ///
    /// - any layout to [custom](ChannelOrder::CUSTOM) order, losslessly;
    /// - any layout to [unspecified](ChannelOrder::UNSPEC) order, lossily
    ///   unless it is a custom map of unnamed [`Channel::UNKNOWN`]s;
    /// - a custom layout to [native](ChannelOrder::NATIVE) order when its
    ///   map lists distinct native channels (ids below 63) in increasing
    ///   order, lossily when they are named;
    /// - a custom layout to [ambisonic](ChannelOrder::AMBISONIC) order when
    ///   its map starts with every component of some ambisonic order, in
    ///   ACN order, followed by native channels as above, lossily when they
    ///   are named.
    ///
    /// ```
    /// use ffmpeg::libavutil::channel_layout::{ChannelLayout, ChannelOrder, Conversion};
    /// let mut l = ChannelLayout::parse(c"FL@left+FR").unwrap();
    /// assert_eq!(l.as_ref().order(), ChannelOrder::CUSTOM);
    /// assert_eq!(l.as_mut().retype(ChannelOrder::NATIVE), Ok(Conversion::Lossy));
    /// assert_eq!(l.as_mut().retype(ChannelOrder::CUSTOM), Ok(Conversion::Lossless));
    /// assert_eq!(l.as_mut().retype(ChannelOrder::UNSPEC), Ok(Conversion::Lossy));
    /// assert_eq!(l.as_ref().channel_count(), 2);
    /// ```
    ///
    /// # Errors
    ///
    /// Leaving the layout as it was: `AVERROR(ENOSYS)` for a conversion C
    /// does not make (native to ambisonic, a custom map native order cannot
    /// list, ...), `AVERROR(EINVAL)` for a layout C deems invalid (no
    /// channels, a native count other than its mask's, a map holding
    /// [`Channel::NONE`], ...) or an `order` C does not know, and
    /// `AVERROR(ENOMEM)` when a custom map cannot be allocated.
    pub fn retype(&mut self, order: ChannelOrder) -> Result<Conversion, i32> {
        let lossy = self.retype_with(order, 0)?;
        Ok(if lossy > 0 {
            Conversion::Lossy
        } else {
            Conversion::Lossless
        })
    }

    /// Wraps: av_channel_layout_retype
    ///
    /// Changes the layout's order to `order` as [`retype`](Self::retype)
    /// does, but only if every channel designation and name survives.
    ///
    /// ```
    /// use ffmpeg::libavutil::channel_layout::{ChannelLayout, ChannelOrder};
    /// let mut l = ChannelLayout::parse(c"FL@left+FR").unwrap();
    /// assert_eq!(l.as_mut().retype_lossless(ChannelOrder::NATIVE), Err(-38)); // ENOSYS
    /// assert_eq!(l.as_ref().order(), ChannelOrder::CUSTOM);
    /// ```
    ///
    /// # Errors
    ///
    /// As [`retype`](Self::retype)'s, and `AVERROR(ENOSYS)` for a lossy
    /// conversion, each leaving the layout as it was.
    pub fn retype_lossless(&mut self, order: ChannelOrder) -> Result<(), i32> {
        // A lossy conversion is refused, so C succeeds with 0 only.
        self.retype_with(order, RETYPE_LOSSLESS).map(drop)
    }

    /// Wraps: av_channel_layout_retype
    ///
    /// Changes the layout to the simplest order that keeps every channel
    /// designation and name: a custom map of unnamed channels becomes
    /// unspecified when every one is [`Channel::UNKNOWN`], native when
    /// native order can list it, else ambisonic when ambisonic order can,
    /// and stays custom otherwise; a layout in any other order is left as it
    /// is.
    ///
    /// ```
    /// use ffmpeg::libavutil::channel_layout::{ChannelLayout, ChannelOrder, masks};
    /// let mut l = ChannelLayout::from_mask(masks::L5POINT1).unwrap();
    /// l.as_mut().retype(ChannelOrder::CUSTOM).unwrap();
    /// l.as_mut().canonicalize().unwrap();
    /// assert_eq!(l.as_ref().order(), ChannelOrder::NATIVE);
    /// ```
    ///
    /// # Errors
    ///
    /// `AVERROR(EINVAL)`, leaving the layout as it was, for a layout C
    /// deems invalid, as for [`retype`](Self::retype).
    pub fn canonicalize(&mut self) -> Result<(), i32> {
        // C picks an order it converts to losslessly, so it succeeds with 0
        // only; the order passed is ignored.
        self.retype_with(ChannelOrder::UNSPEC, RETYPE_CANONICAL)
            .map(drop)
    }
}

/// Wraps: av_channel_layout_standard
///
/// Every standard layout in C's table, in the table's order: the native
/// layouts [`ChannelLayout::parse`] knows by name, from `mono` and `stereo`
/// to `22.2`, each borrowed for the whole program.
///
/// ```
/// use ffmpeg::libavutil::channel_layout::{ChannelLayout, LayoutDetails, masks, standard_layouts};
/// let first = standard_layouts().next().unwrap();
/// assert!(matches!(first.details(), LayoutDetails::Native(masks::MONO)));
/// assert!(standard_layouts().any(|l| l.channel_count() == 24));
/// let owned = standard_layouts().last().unwrap().try_clone().unwrap();
/// assert_eq!(owned.as_ref().subset(u64::MAX), masks::L22POINT2);
/// ```
///
/// The entries are C's shared, read-only table: only a copy converts.
///
/// ```compile_fail,E0599
/// use ffmpeg::libavutil::channel_layout::{ChannelOrder, standard_layouts};
/// let mut stereo = standard_layouts().nth(1).unwrap();
/// let _ = stereo.retype(ChannelOrder::CUSTOM);
/// ```
#[must_use]
pub fn standard_layouts() -> StandardLayouts {
    StandardLayouts { next: 0 }
}

/// The iterator over C's standard channel layouts that [`standard_layouts`]
/// returns.
#[derive(Clone, Copy, Debug)]
pub struct StandardLayouts {
    /// C's iteration state: the index of the next table entry, which C keeps
    /// in a `void *` without ever dereferencing it.
    next: usize,
}

impl Iterator for StandardLayouts {
    type Item = ChannelLayoutRef<'static>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut state = ptr::without_provenance_mut::<c_void>(self.next);
        // SAFETY: `state` is a live, writable slot holding an index, which C
        // reads back as an integer and replaces by the next one while its
        // table has entries left, never dereferencing it.
        let layout = unsafe { ffi::av_channel_layout_standard(&mut state) };
        self.next = state.addr();
        // SAFETY: C returns NULL past its table's end, or the address of an
        // entry of its `static const` table, which lives unwritten for the
        // whole program; every entry is a native layout holding no map, so a
        // shared `'static` handle only reads it.
        unsafe { ChannelLayoutRef::from_ptr(layout.cast_mut()) }
    }
}

// Past the table's end C leaves the index unchanged, so every later call
// returns NULL again.
impl FusedIterator for StandardLayouts {}

/// The `highest_ambi` index past which C's `av_channel_layout_ambisonic_order`
/// overflows an `int`: from `46340^2` on, `floor(sqrt(highest_ambi))` is
/// 46340 and C squares 46341.
const AMBISONIC_HIGHEST_LIMIT: i64 = 46_340 * 46_340;

/// Whether `av_channel_layout_index_from_string(layout, name)` compares
/// `name` with a custom map's names (`strcmp`): when it holds an `@` with at
/// least one byte after it.
fn compares_map_names(name: &CStr) -> bool {
    let bytes = name.to_bytes();
    bytes
        .iter()
        .position(|&b| b == b'@')
        .is_some_and(|at| at + 1 < bytes.len())
}

impl<'a> ChannelLayoutRef<'a> {
    /// Whether C's per-channel readers (`av_channel_layout_channel_from_index`,
    /// `_index_from_channel` and the readers built on them) can read this
    /// layout: false for a custom layout C left without a map (after its own
    /// failed copy) or with a negative count, which C would read through or
    /// past, and for a native or ambisonic count so negative that C's
    /// `nb_channels - popcount(mask)` overflows an `int`. No safe path leaves
    /// either.
    fn walkable(&self) -> bool {
        match self.details() {
            LayoutDetails::Custom(_) => self
                .map_parts()
                .is_some_and(|(map, count)| !map.is_null() && count >= 0),
            LayoutDetails::Native(mask) | LayoutDetails::Ambisonic(mask) => self
                .channel_count()
                .checked_sub(mask.count_ones() as c_int)
                .is_some(),
            _ => true,
        }
    }

    /// Whether C computes an ambisonic layout's order without overflowing
    /// an `int` (true in any other order): its `highest_ambi`, the count
    /// less the non-diegetic channels and one, is not negative and below
    /// [`AMBISONIC_HIGHEST_LIMIT`]. C answers `AVERROR(EINVAL)` past both
    /// bounds where it does not overflow: no order exists there.
    fn ambisonic_computable(&self) -> bool {
        let LayoutDetails::Ambisonic(mask) = self.details() else {
            return true;
        };
        let highest = i64::from(self.channel_count()) - i64::from(mask.count_ones()) - 1;
        (0..AMBISONIC_HIGHEST_LIMIT).contains(&highest)
    }

    /// Whether every name of a custom map ends within its field, as C's
    /// `%s` and `strcmp` readers of the names assume; true in any other
    /// order. Only [`ChannelCustom::from_raw`] carries an unterminated name
    /// into a map.
    fn names_terminated(&self) -> bool {
        match self.details() {
            LayoutDetails::Custom(map) => map
                .iter()
                .all(|c| c.name().as_bytes().len() <= ChannelName::MAX_LEN),
            _ => true,
        }
    }

    /// `AVERROR(EINVAL)` for a layout C's describers cannot read: one the
    /// per-channel readers cannot [walk](Self::walkable), an ambisonic count
    /// C cannot [compute an order of](Self::ambisonic_computable), or a
    /// map name with no NUL, which C prints with `%s`.
    fn describable(&self) -> Result<(), i32> {
        if self.walkable() && self.ambisonic_computable() && self.names_terminated() {
            Ok(())
        } else {
            Err(EINVAL)
        }
    }

    /// Wraps: av_channel_layout_check
    ///
    /// Whether C deems the layout valid: it holds at least one channel and,
    /// by its order, a native mask of as many channels, a custom map with no
    /// [`Channel::NONE`], or an ambisonic count above its non-diegetic
    /// channels'; any count in unspecified order, and no layout in an order
    /// C does not know.
    ///
    /// ```
    /// use ffmpeg::libavutil::channel_layout::ChannelLayout;
    /// assert!(ChannelLayout::STEREO.as_ref().is_valid());
    /// assert!(!ChannelLayout::unspecified(0).as_ref().is_valid());
    /// ```
    #[must_use]
    pub fn is_valid(&self) -> bool {
        // SAFETY: the shared handle covers a live layout that nothing writes
        // while it lives, and C only reads it: the order, the count, the
        // mask, and, in custom order with a positive count, the map pointer
        // (checked for NULL) and the ids of the `nb_channels` initialised
        // elements every safe path keeps behind a non-NULL one.
        unsafe { ffi::av_channel_layout_check(self.as_ptr()) == 1 }
    }

    /// Wraps: av_channel_layout_ambisonic_order
    ///
    /// The Ambisonic order `n` of a layout that starts with all `(n + 1)^2`
    /// components of order `n`, each at its own ACN index: an ambisonic
    /// layout's, or a custom map's leading [`Channel::ambisonic`] channels,
    /// none of them after a channel of another kind. `None` in any other
    /// order, for a custom map with no Ambisonic channel or with one out of
    /// place, and for an incomplete order (C's `AVERROR(EINVAL)`).
    ///
    /// ```
    /// use ffmpeg::libavutil::channel_layout::ChannelLayout;
    /// assert_eq!(ChannelLayout::AMBISONIC_FIRST_ORDER.as_ref().ambisonic_order(), Some(1));
    /// let custom = ChannelLayout::parse(c"AMBI0+AMBI1+AMBI2+AMBI3+FL@x").unwrap();
    /// assert_eq!(custom.as_ref().ambisonic_order(), Some(1));
    /// assert_eq!(ChannelLayout::STEREO.as_ref().ambisonic_order(), None);
    /// ```
    #[must_use]
    pub fn ambisonic_order(&self) -> Option<u32> {
        if !self.walkable() || !self.ambisonic_computable() {
            return None;
        }
        // SAFETY: the shared handle covers a live layout C only reads: an
        // ambisonic layout's count and mask, whose arithmetic stays in `int`
        // range (checked above), or the ids of a custom layout's
        // `nb_channels >= 0` elements behind its non-NULL map (checked
        // above), the only map C walks.
        let ret = unsafe { ffi::av_channel_layout_ambisonic_order(self.as_ptr()) };
        u32::try_from(ret).ok()
    }

    /// Wraps: av_channel_layout_channel_from_index
    ///
    /// The channel at `index` in the layout's order: a custom map's id at
    /// `index`, an ambisonic layout's component `index` before its
    /// non-diegetic channels, which follow in id order as a native layout's
    /// do. `None` past the last channel, in unspecified order or an order C
    /// does not know, and for a map element holding [`Channel::NONE`], which
    /// C returns for no channel.
    ///
    /// ```
    /// use ffmpeg::libavutil::channel_layout::{Channel, ChannelLayout};
    /// let l = ChannelLayout::L2POINT1;
    /// assert_eq!(l.as_ref().channel(2), Some(Channel::LOW_FREQUENCY));
    /// assert_eq!(l.as_ref().channel(3), None);
    /// ```
    #[must_use]
    pub fn channel(&self, index: usize) -> Option<Channel> {
        // C takes an `unsigned` index; no layout counts more than `i32::MAX`
        // channels, so a wider one is past the end of all of them.
        let index = c_uint::try_from(index).ok()?;
        if !self.walkable() {
            return None;
        }
        // SAFETY: the shared handle covers a live layout C only reads: the
        // order, count and mask, and, in custom order, element `index` of
        // the non-NULL map (checked above) only when `index` is below the
        // non-negative count (checked above), which the map holds.
        let raw = unsafe { ffi::av_channel_layout_channel_from_index(self.as_ptr(), index) };
        Some(Channel::from_raw(raw)).filter(|&ch| ch != Channel::NONE)
    }

    /// Wraps: av_channel_layout_index_from_channel
    ///
    /// The index of `channel` in the layout, the inverse of
    /// [`channel`](Self::channel): its first position in a custom map, its
    /// ACN index among an ambisonic layout's components, or its position
    /// among a native layout's (or an ambisonic layout's non-diegetic)
    /// channels. `None` when the layout does not hold it, in unspecified
    /// order or an order C does not know, and for [`Channel::NONE`].
    ///
    /// ```
    /// use ffmpeg::libavutil::channel_layout::{Channel, ChannelLayout};
    /// let l = ChannelLayout::L5POINT1;
    /// assert_eq!(l.as_ref().index_of(Channel::LOW_FREQUENCY), Some(3));
    /// assert_eq!(l.as_ref().index_of(Channel::BACK_LEFT), None);
    /// ```
    #[must_use]
    pub fn index_of(&self, channel: Channel) -> Option<usize> {
        if !self.walkable() {
            return None;
        }
        let layout = self.as_ptr();
        // SAFETY: the shared handle covers a live layout C only reads: a
        // native or ambisonic layout's count and mask, whose difference
        // stays in `int` range (checked above), or the ids of a custom
        // layout's `nb_channels >= 0` elements behind its non-NULL map
        // (checked above).
        let ret = unsafe { ffi::av_channel_layout_index_from_channel(layout, channel.into_raw()) };
        usize::try_from(ret).ok()
    }

    /// Whether C's lookups by name can read this layout for `name`: when
    /// they compare it with a custom map's names, every name must end in its
    /// field.
    fn searchable_for(&self, name: &CStr) -> bool {
        self.walkable() && (!compares_map_names(name) || self.names_terminated())
    }

    /// Wraps: av_channel_layout_index_from_string
    ///
    /// The index of the channel `name` names in the layout, as
    /// [`index_of`](Self::index_of) finds the channel
    /// [`Channel::from_name`] reads from it. A custom map is first searched
    /// by name: `<abbr>@<name>` finds the first element of that name whose
    /// id is the channel `<abbr>` names, and `@<name>` the first of that
    /// name whatever its id; failing that, the whole string is read as a
    /// channel name.
    ///
    /// ```
    /// use ffmpeg::libavutil::channel_layout::ChannelLayout;
    /// let l = ChannelLayout::parse(c"FL@left+FR@right+FL@rear").unwrap();
    /// assert_eq!(l.as_ref().index_by_name(c"FR"), Some(1));
    /// assert_eq!(l.as_ref().index_by_name(c"@rear"), Some(2));
    /// assert_eq!(l.as_ref().index_by_name(c"FL@rear"), Some(2));
    /// assert_eq!(l.as_ref().index_by_name(c"FR@rear"), None);
    /// ```
    ///
    /// `None` where C reports `AVERROR(EINVAL)`: when nothing matches, for a
    /// `<abbr>` before the `@` that names no channel, in unspecified order or
    /// an order C does not know. Also `None`, before C is called, when the
    /// search by name would meet a map name with no NUL, which C would read
    /// past.
    #[must_use]
    pub fn index_by_name(&self, name: &CStr) -> Option<usize> {
        if !self.searchable_for(name) {
            return None;
        }
        let (layout, name) = (self.as_ptr(), name.as_ptr());
        // SAFETY: the shared handle covers a live layout C only reads,
        // within the bounds `index_of` relies on (checked above), and, when
        // it compares `name` with the map's names, names that each end in
        // their field (checked above); `name` is a live C string C only
        // reads during the call.
        let ret = unsafe { ffi::av_channel_layout_index_from_string(layout, name) };
        usize::try_from(ret).ok()
    }

    /// Wraps: av_channel_layout_channel_from_string
    ///
    /// The channel `name` names in the layout: the [`channel`](Self::channel)
    /// at the [`index_by_name`](Self::index_by_name) of `name`, so a custom
    /// map element is found by its name too. `None` where `index_by_name`
    /// finds nothing.
    ///
    /// ```
    /// use ffmpeg::libavutil::channel_layout::{Channel, ChannelLayout};
    /// let l = ChannelLayout::parse(c"FL@left+LFE@sub").unwrap();
    /// assert_eq!(l.as_ref().channel_by_name(c"@sub"), Some(Channel::LOW_FREQUENCY));
    /// assert_eq!(l.as_ref().channel_by_name(c"FR"), None);
    /// ```
    #[must_use]
    pub fn channel_by_name(&self, name: &CStr) -> Option<Channel> {
        if !self.searchable_for(name) {
            return None;
        }
        let (layout, name) = (self.as_ptr(), name.as_ptr());
        // SAFETY: as in `index_by_name`; C then reads the channel at the
        // index it found, which is below the count.
        let raw = unsafe { ffi::av_channel_layout_channel_from_string(layout, name) };
        Some(Channel::from_raw(raw)).filter(|&ch| ch != Channel::NONE)
    }

    /// Wraps: av_channel_layout_describe
    ///
    /// Writes the layout's description, as
    /// [`description`](Self::description) returns it, into `buf`, truncated
    /// to fit with its NUL, and returns the size a buffer needs to hold it
    /// whole, NUL included: it was truncated when that exceeds `buf.len()`.
    /// An empty `buf` only measures it.
    ///
    /// # Errors
    ///
    /// As for [`description`](Self::description), and `AVERROR(ERANGE)` for
    /// a description of `INT_MAX` bytes or more.
    pub fn write_description(&self, buf: &mut [u8]) -> Result<usize, i32> {
        self.describable()?;
        let (ptr, size) = name_buffer(buf);
        // SAFETY: the shared handle covers a live layout C only reads, within
        // the bounds its readers need (checked above); `ptr` is NULL with
        // size 0, which C takes as a count-only request and never
        // dereferences, or valid for `size` bytes of writes, which bound
        // every byte C writes; C keeps no pointer to either.
        let ret = unsafe { ffi::av_channel_layout_describe(self.as_ptr(), ptr, size) };
        usize::try_from(ret).map_err(|_| ret)
    }

    /// Wraps: av_channel_layout_describe
    ///
    /// The description of the layout, which [`ChannelLayout::parse`] reads
    /// back: the name of a standard native layout (`stereo`, `5.1(side)`),
    /// `<n> channels (<names>)` listing other native layouts and custom maps
    /// channel by channel, `@<name>` after a named channel, `ambisonic <n>`
    /// with any non-diegetic channels after a `+`, and `<n> channels` in
    /// unspecified order. A custom map C can list as native or ambisonic
    /// (unnamed native channels in increasing order, after any complete
    /// Ambisonic order) is described as such.
    ///
    /// ```
    /// use ffmpeg::libavutil::channel_layout::{ChannelLayout, ChannelLayoutRef, masks};
    /// let d = |l: ChannelLayoutRef<'_>| l.description().unwrap().into_string().unwrap();
    /// assert_eq!(d(ChannelLayout::L5POINT1.as_ref()), "5.1(side)");
    /// let odd = ChannelLayout::native(masks::STEREO | 1 << 50);
    /// assert_eq!(d(odd.as_ref()), "3 channels (FL+FR+USR50)");
    /// assert_eq!(d(ChannelLayout::AMBISONIC_FIRST_ORDER.as_ref()), "ambisonic 1");
    /// let custom = ChannelLayout::parse(c"FR@r+FL").unwrap();
    /// assert_eq!(d(custom.as_ref()), "2 channels (FR@r+FL)");
    /// ```
    ///
    /// # Errors
    ///
    /// `AVERROR(EINVAL)` for an order C does not know, and, before C is
    /// called, for a map name with no NUL, which C would print past.
    pub fn description(&self) -> Result<CString, i32> {
        let need = self.write_description(&mut [])?;
        let mut buf = vec![0; need];
        self.write_description(&mut buf)?;
        Ok(CString::from_vec_with_nul(buf).expect("C writes one NUL, at the end"))
    }

    /// Wraps: av_channel_layout_describe_bprint
    ///
    /// Appends the layout's [description](Self::description) to `buf`,
    /// truncated as `buf` truncates what does not fit. `buf` is a handle,
    /// which `as_mut()` reborrows from an owner or another handle.
    ///
    /// ```
    /// use ffmpeg::libavutil::bprint::BPrint;
    /// use ffmpeg::libavutil::channel_layout::ChannelLayout;
    /// let mut buf = BPrint::new(0, BPrint::SIZE_UNLIMITED).unwrap();
    /// buf.as_mut().append_bytes(b"layout: ");
    /// ChannelLayout::STEREO.as_ref().append_description(buf.as_mut()).unwrap();
    /// assert_eq!(buf.as_ref().as_bytes(), b"layout: stereo");
    /// ```
    ///
    /// # Errors
    ///
    /// As for [`description`](Self::description), leaving `buf` as it was.
    pub fn append_description(&self, mut buf: BPrintMut<'_>) -> Result<(), i32> {
        self.describable()?;
        let (layout, bp) = (self.as_ptr(), buf.as_mut_ptr());
        // SAFETY: the shared handle covers a live layout C only reads, within
        // the bounds its readers need (checked above); the exclusive handle
        // addresses an initialised print buffer, which C appends to with
        // `av_bprintf` within its `size`; C keeps no pointer to either.
        let ret = unsafe { ffi::av_channel_layout_describe_bprint(layout, bp) };
        if ret < 0 { Err(ret) } else { Ok(()) }
    }
}

/// Wraps: av_channel_layout_compare
///
/// Whether two layouts hold the same channels in the same order, as C
/// compares them: their counts must be equal; two unspecified layouts are
/// equal, and one equals no layout in another order; two native or two
/// ambisonic layouts compare their masks; any other pair compares
/// [channel](ChannelLayoutRef::channel) by channel, so a custom map of
/// native channels in id order equals the native layout of the same mask.
/// Names and user pointers are not compared.
///
/// ```
/// use ffmpeg::libavutil::channel_layout::{ChannelLayout, ChannelOrder};
/// let mut l = ChannelLayout::parse(c"FL@left+FR").unwrap();
/// assert!(l.as_ref() == ChannelLayout::STEREO.as_ref());
/// l.as_mut().retype(ChannelOrder::UNSPEC).unwrap();
/// assert!(l.as_ref() != ChannelLayout::STEREO.as_ref());
/// assert!(l.as_ref() == ChannelLayout::unspecified(2).as_ref());
/// ```
///
/// Not `Eq`: a layout C's readers cannot walk (a custom layout C left
/// without a map), which no safe path keeps, equals nothing, itself
/// included, rather than being read through.
///
/// ```compile_fail,E0277
/// use ffmpeg::libavutil::channel_layout::ChannelLayout;
/// fn total<T: Eq>(_: &T) {}
/// total(&ChannelLayout::STEREO);
/// ```
impl PartialEq<ChannelLayoutRef<'_>> for ChannelLayoutRef<'_> {
    fn eq(&self, other: &ChannelLayoutRef<'_>) -> bool {
        if !self.walkable() || !other.walkable() {
            return false;
        }
        // SAFETY: both shared handles cover live layouts that nothing writes
        // while they live, possibly the same one, which C only reads: their
        // counts, orders and masks, and the channels below their (equal,
        // non-negative) count, read as `channel` reads them (checked above).
        let ret = unsafe { ffi::av_channel_layout_compare(self.as_ptr(), other.as_ptr()) };
        ret == 0
    }
}

/// As the layouts' shared handles compare.
impl PartialEq for ChannelLayout {
    fn eq(&self, other: &Self) -> bool {
        self.as_ref() == other.as_ref()
    }
}

#[cfg(test)]
mod unit_tests {
    use core::ffi::{c_int, c_uint};
    use core::mem::{align_of, size_of};
    use std::ffi::CString;

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

    #[test]
    fn channel_default_is_none() {
        assert_eq!(Channel::default(), Channel::NONE);
        assert_eq!(Channel::default().into_raw().0, -1);
        assert_eq!(Channel::default().ambisonic_index(), None);
        assert_eq!(format!("{:?}", Channel::default()), "NONE");
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

    #[test]
    fn matrix_encoding_default_is_none_the_zeroed_value() {
        assert_eq!(MatrixEncoding::default(), MatrixEncoding::NONE);
        assert_eq!(MatrixEncoding::default().into_raw().0, 0);
        assert_eq!(MatrixEncoding::default().index(), Some(0));
        assert_eq!(format!("{:?}", MatrixEncoding::default()), "NONE");
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

    /// A raw channel whose name field holds `name`'s bytes, a NUL if they
    /// leave room for one, then `fill` up to the end of the field.
    fn channel_custom_raw(id: c_int, name: &[u8], fill: u8, opaque: usize) -> ffi::AVChannelCustom {
        let mut field = [fill as c_char; 16];
        for (f, &b) in field.iter_mut().zip(name) {
            *f = b as c_char;
        }
        if let Some(nul) = field.get_mut(name.len()) {
            *nul = 0;
        }
        ffi::AVChannelCustom {
            id: ffi::AVChannel(id),
            name: field,
            opaque: core::ptr::without_provenance_mut(opaque),
        }
    }

    /// Every byte of a raw channel, as a comparable tuple.
    fn channel_custom_bytes(r: ffi::AVChannelCustom) -> (c_int, [c_char; 16], *mut c_void) {
        (r.id.0, r.name, r.opaque)
    }

    #[test]
    fn channel_custom_layout_is_the_c_struct() {
        assert_eq!(size_of::<ChannelCustom>(), size_of::<ffi::AVChannelCustom>());
        assert_eq!(size_of::<ChannelCustom>(), 32);
        assert_eq!(align_of::<ChannelCustom>(), align_of::<ffi::AVChannelCustom>());
        assert_eq!(ChannelName::MAX_LEN, 15);
    }

    #[test]
    fn channel_custom_new_default_and_zeroed() {
        let z = ChannelCustom::zeroed();
        assert_eq!(z.as_ref().id(), Channel::FRONT_LEFT);
        assert!(z.as_ref().name().is_empty());
        assert!(z.as_ref().opaque().is_null());
        let d = ChannelCustom::default();
        assert_eq!(d.as_ref().id(), Channel::UNKNOWN);
        assert!(d.as_ref().name().is_empty());
        assert!(d.as_ref().opaque().is_null());
        let n = ChannelCustom::new(Channel::LOW_FREQUENCY);
        assert_eq!(n.into_raw().id, ffi::AVChannel::AV_CHAN_LOW_FREQUENCY);
        assert_eq!(n.into_raw().name, [0; 16]);
        assert_ne!(n, d);
        assert_eq!(ChannelCustom::new(Channel::UNKNOWN), d);
    }

    #[test]
    fn channel_custom_raw_round_trips() {
        let raw = channel_custom_raw(-7, b"abc", 0x55, 0x1234);
        let ch = ChannelCustom::from(raw);
        let back = ffi::AVChannelCustom::from(ch);
        assert_eq!(back.id, raw.id);
        assert_eq!(back.name, raw.name);
        assert_eq!(back.opaque, raw.opaque);
        assert_eq!(ch.as_ref().id(), Channel::from_raw(ffi::AVChannel(-7)));
        assert_eq!(ch.as_ref().name(), c"abc");
        assert!(ch.as_ref().name() == *c"abc");
        assert!(ch.as_ref().name() != *c"ab");
        assert_eq!(ch.as_ref().opaque().addr(), 0x1234);
        assert_eq!(ch.as_ref().get().into_raw().name, raw.name);
    }

    #[test]
    fn channel_custom_setters_write_each_field_alone() {
        let mut ch = ChannelCustom::from_raw(channel_custom_raw(1, b"x", 0, 0));
        let mut m = ch.as_mut();
        m.set_id(Channel::from_raw(ffi::AVChannel(c_int::MAX)));
        assert_eq!(m.as_ref().name(), c"x");
        let mut cookie = 5u64;
        m.set_opaque((&raw mut cookie).cast());
        assert_eq!(m.as_ref().id().into_raw().0, c_int::MAX);
        assert_eq!(m.as_ref().name(), c"x");
        assert_eq!(m.set_name(c"rear"), Ok(()));
        assert_eq!(m.as_ref().opaque(), (&raw mut cookie).cast());
        assert_eq!(m.as_ref().id().into_raw().0, c_int::MAX);
        m.set_opaque(core::ptr::null_mut());
        assert!(ch.as_ref().opaque().is_null());
        assert_eq!(ch.as_ref().name(), c"rear");
    }

    #[test]
    fn channel_custom_set_name_accepts_up_to_fifteen_bytes() {
        let long = c"0123456789abcdefXYZ";
        for len in 0..=long.to_bytes().len() {
            let mut ch = ChannelCustom::from_raw(channel_custom_raw(0, b"previous", 0x7f, 0));
            let name = CString::new(&long.to_bytes()[..len]).unwrap();
            let r = ch.as_mut().set_name(&name);
            if len <= 15 {
                assert_eq!(r, Ok(()), "{len}");
                assert_eq!(ch.as_ref().name(), name.as_c_str());
                assert_eq!(ch.into_raw().name[len], 0);
            } else {
                assert_eq!(r, Err(NameTooLong { len }));
                // Refused: the old name is untouched.
                assert_eq!(ch.as_ref().name(), c"previous");
                assert_eq!(
                    channel_custom_bytes(ch.into_raw()),
                    channel_custom_bytes(channel_custom_raw(0, b"previous", 0x7f, 0))
                );
            }
        }
    }

    #[test]
    fn channel_custom_set_name_truncated_mirrors_strlcpy() {
        let long = c"0123456789abcdefXYZ";
        for len in 0..=long.to_bytes().len() {
            let mut ch = ChannelCustom::from_raw(channel_custom_raw(0, b"", 0x7f, 0));
            let name = CString::new(&long.to_bytes()[..len]).unwrap();
            let whole = ch.as_mut().set_name_truncated(&name);
            assert_eq!(whole, len <= 15, "{len}");
            let kept = len.min(15);
            assert_eq!(ch.as_ref().name().as_bytes(), &long.to_bytes()[..kept]);
            // The bytes and a NUL; the rest of the field is left as it was,
            // as `av_strlcpy` leaves it.
            let raw = ch.into_raw().name;
            assert_eq!(raw[kept], 0);
            assert!(raw[kept + 1..].iter().all(|&c| c == 0x7f), "{len}");
        }
        // A shorter name over a longer one keeps the longer one's tail.
        let mut ch = ChannelCustom::zeroed();
        assert!(ch.as_mut().set_name_truncated(c"abcdef"));
        assert!(ch.as_mut().set_name_truncated(c"xy"));
        assert_eq!(ch.as_ref().name(), c"xy");
        assert_eq!(&ch.into_raw().name[..7], &[120, 121, 0, 100, 101, 102, 0]);
    }

    #[test]
    fn channel_name_reads_within_the_field() {
        // Unterminated: all 16 bytes, and no byte past the field.
        let ch = ChannelCustom::from_raw(channel_custom_raw(0, b"0123456789abcdef", 0, 0));
        assert_eq!(ch.as_ref().name().as_bytes(), b"0123456789abcdef");
        assert_eq!(ch.as_ref().name().as_c_str(), c"0123456789abcdef");
        // The first NUL ends it; bytes after it do not count.
        let a = ChannelCustom::from_raw(channel_custom_raw(0, b"ab\0cd", 0x41, 0));
        let b = ChannelCustom::from_raw(channel_custom_raw(0, b"ab", 0, 0));
        assert_eq!(a.as_ref().name(), b.as_ref().name());
        assert_eq!(a, b);
        assert_eq!(a.as_ref().name().to_str(), Ok("ab"));
        // Non-UTF-8 bytes are kept.
        let c = ChannelCustom::from_raw(channel_custom_raw(0, b"\xff\xfe", 0, 0));
        assert_eq!(c.as_ref().name().as_bytes(), b"\xff\xfe");
        assert!(c.as_ref().name().to_str().is_err());
        assert!(!c.as_ref().name().is_empty());
        assert_eq!(format!("{:?}", c.as_ref().name()), format!("{:?}", c"\xff\xfe"));
        assert_eq!(ChannelName::default().as_c_str(), c"");
    }

    #[test]
    fn channel_name_compares_and_hashes_by_bytes() {
        use std::collections::HashSet;
        let names: HashSet<ChannelName> = [b"ab".as_slice(), b"ab\0x", b"abc", b""]
            .into_iter()
            .map(|n| ChannelCustom::from_raw(channel_custom_raw(0, n, 0x30, 0)).as_ref().name())
            .collect();
        // `"ab\0x"` and `"ab"` (tail `0x30`s after the NUL) are the same
        // name; `""` is an empty one.
        assert_eq!(names.len(), 3);
    }

    #[test]
    fn channel_custom_equality_is_by_c_observable_fields() {
        let base = channel_custom_raw(3, b"lfe", 0, 0x10);
        let a = ChannelCustom::from_raw(base);
        assert_eq!(a, ChannelCustom::from_raw(channel_custom_raw(3, b"lfe", 0x44, 0x10)));
        assert_ne!(a, ChannelCustom::from_raw(channel_custom_raw(4, b"lfe", 0, 0x10)));
        assert_ne!(a, ChannelCustom::from_raw(channel_custom_raw(3, b"lf", 0, 0x10)));
        assert_ne!(a, ChannelCustom::from_raw(channel_custom_raw(3, b"lfe", 0, 0x11)));
    }

    #[test]
    fn channel_custom_hashes_as_it_compares() {
        use std::collections::HashSet;
        use std::hash::{BuildHasher, RandomState};

        let hasher = RandomState::new();
        let hash = |ch: ChannelCustom| hasher.hash_one(ch);
        let a = ChannelCustom::from_raw(channel_custom_raw(3, b"lfe", 0, 0x10));
        // Equal values hash alike, whatever the bytes after the name's NUL.
        let twin = ChannelCustom::from_raw(channel_custom_raw(3, b"lfe", 0x44, 0x10));
        assert_eq!(hash(a), hash(twin));
        let unequal = [
            ChannelCustom::from_raw(channel_custom_raw(4, b"lfe", 0, 0x10)),
            ChannelCustom::from_raw(channel_custom_raw(3, b"lf", 0, 0x10)),
            ChannelCustom::from_raw(channel_custom_raw(3, b"lfe", 0, 0x11)),
            ChannelCustom::default(),
            ChannelCustom::zeroed(),
        ];
        let mut set: HashSet<ChannelCustom> = unequal.into_iter().collect();
        assert_eq!(set.len(), unequal.len());
        assert!(set.insert(a));
        assert!(!set.insert(twin));
        assert!(set.contains(&ChannelCustom::new(Channel::UNKNOWN)));
        assert_eq!(set.len(), unequal.len() + 1);
    }

    #[test]
    fn channel_custom_get_and_set_copy_whole_values() {
        let src = ChannelCustom::from_raw(channel_custom_raw(9, b"side", 0x22, 0x99));
        let mut dst = ChannelCustom::zeroed();
        dst.as_mut().set(src);
        assert_eq!(channel_custom_bytes(dst.into_raw()), channel_custom_bytes(src.into_raw()));
        let copy = dst.as_ref().get();
        dst.as_mut().set_id(Channel::NONE);
        assert_eq!(copy, src);
        #[allow(clippy::clone_on_copy)] // exercises the explicit `Clone` impl
        let c = Clone::clone(&copy);
        assert_eq!(c, src);
    }

    #[test]
    fn channel_custom_debug_names_every_field() {
        let ch = ChannelCustom::from_raw(channel_custom_raw(3, b"sub", 0, 0));
        let s = format!("{ch:?}");
        assert!(s.starts_with("ChannelCustom {"), "{s}");
        assert!(s.contains("id: LOW_FREQUENCY"), "{s}");
        assert!(s.contains("name: \"sub\""), "{s}");
        assert!(s.contains("opaque: 0x0"), "{s}");
    }

    #[test]
    fn channel_custom_name_too_long_displays_both_lengths() {
        let e = NameTooLong { len: 20 };
        assert_eq!(e.to_string(), "channel name of 20 bytes exceeds the 15-byte limit");
        let _: &dyn core::error::Error = &e;
    }

    #[test]
    fn channel_custom_handles_borrow_raw_storage() {
        let mut raw = channel_custom_raw(2, b"c", 0, 0);
        // SAFETY: `raw` is a live, initialised `AVChannelCustom` on this
        // frame, outliving the handle, and nothing else touches it meanwhile.
        let mut m = unsafe { ChannelCustomMut::from_ptr(&raw mut raw) }.unwrap();
        m.set_id(Channel::FRONT_CENTER);
        assert_eq!(m.set_name(c"centre"), Ok(()));
        assert_eq!(raw.id, ffi::AVChannel::AV_CHAN_FRONT_CENTER);
        // SAFETY: as above, for a shared handle.
        let r = unsafe { ChannelCustomRef::from_ptr(&raw mut raw) }.unwrap();
        assert_eq!(r.name(), c"centre");
        // SAFETY: NULL is accepted and yields `None`.
        assert!(unsafe { ChannelCustomRef::from_ptr(core::ptr::null_mut()) }.is_none());
        // SAFETY: as above.
        assert!(unsafe { ChannelCustomMut::from_ptr(core::ptr::null_mut()) }.is_none());
    }

    #[test]
    fn channel_custom_is_a_plain_shareable_value() {
        fn assert_traits<
            T: Copy + Send + Sync + Eq + core::hash::Hash + core::fmt::Debug + Default + 'static,
        >() {
        }
        fn assert_handles<T: Send + Sync>() {}
        assert_traits::<ChannelCustom>();
        assert_traits::<ChannelName>();
        assert_handles::<ChannelCustomRef<'static>>();
        assert_handles::<ChannelCustomMut<'static>>();
    }

    #[test]
    fn buffer_size_is_capped_at_cs_unsigned() {
        assert_eq!(c_buffer_size(0), 0);
        assert_eq!(c_buffer_size(16), 16);
        assert_eq!(c_buffer_size(c_uint::MAX as usize), c_uint::MAX as usize);
        assert_eq!(
            c_buffer_size(c_uint::MAX as usize + 1),
            c_uint::MAX as usize
        );
        assert_eq!(c_buffer_size(usize::MAX), c_uint::MAX as usize);
    }

    #[test]
    fn buffer_passes_null_for_empty_and_the_slice_otherwise() {
        let (p, n) = name_buffer(&mut []);
        assert!(p.is_null() && n == 0);
        let mut b = [0u8; 3];
        let base = b.as_mut_ptr();
        let (p, n) = name_buffer(&mut b);
        assert_eq!((p.cast::<u8>(), n), (base, 3));
    }

    #[test]
    fn channel_names_and_descriptions_cover_each_id_class() {
        let cases = [
            (Channel::FRONT_LEFT, c"FL", c"front left"),
            (Channel::BINAURAL_RIGHT, c"BIR", c"binaural right"),
            (Channel::ambisonic(0).unwrap(), c"AMBI0", c"ambisonic ACN 0"),
            (
                Channel::ambisonic(1023).unwrap(),
                c"AMBI1023",
                c"ambisonic ACN 1023",
            ),
            (Channel::NONE, c"NONE", c"none"),
            (Channel::UNKNOWN, c"UNK", c"unknown"),
            (Channel::UNUSED, c"UNSD", c"unused"),
            (
                Channel::from_raw(ffi::AVChannel(500)),
                c"USR500",
                c"user 500",
            ),
            (
                Channel::from_raw(ffi::AVChannel(i32::MIN)),
                c"USR-2147483648",
                c"user -2147483648",
            ),
        ];
        for (ch, name, desc) in cases {
            assert_eq!(ch.name().as_c_str(), name, "{ch:?}");
            assert_eq!(ch.description().as_c_str(), desc, "{ch:?}");
        }
    }

    #[test]
    fn write_name_truncates_and_reports_the_whole_size() {
        let ch = Channel::LOW_FREQUENCY_2;
        let mut buf = [0xAAu8; 8];
        assert_eq!(ch.write_name(&mut buf), Ok(5));
        assert_eq!(&buf[..6], b"LFE2\0\xAA");
        let mut small = [0xAAu8; 3];
        assert_eq!(ch.write_name(&mut small), Ok(5));
        assert_eq!(&small, b"LF\0");
        assert_eq!(ch.write_name(&mut []), Ok(5));
        let mut one = [0xAAu8; 1];
        assert_eq!(ch.write_description(&mut one), Ok(16));
        assert_eq!(one, [0]);
    }

    #[test]
    fn from_name_reads_cs_none_as_not_found() {
        assert_eq!(Channel::from_name(c"FC"), Some(Channel::FRONT_CENTER));
        assert_eq!(Channel::from_name(c"AMBI2"), Channel::ambisonic(2));
        assert_eq!(
            Channel::from_name(c"USR7"),
            Some(Channel::from_raw(ffi::AVChannel(7)))
        );
        assert_eq!(Channel::from_name(c"UNSD"), Some(Channel::UNUSED));
        for name in [c"", c"NONE", c"fc", c"AMBI1024", c"USR-1", c"USR7x"] {
            assert_eq!(Channel::from_name(name), None, "{name:?}");
        }
    }

    /// The raw fields of `l`, the union read as the mask.
    fn layout_raw(l: ChannelLayoutRef<'_>) -> (c_uint, c_int, u64, usize) {
        // SAFETY: a whole-struct read through the shared handle's pointer to
        // a live layout; reading the union as a `u64` copies its bits.
        let raw = unsafe { l.as_ptr().read() };
        // SAFETY: every bit pattern of the union's 8 bytes is a `u64`.
        let mask = unsafe { raw.u.mask };
        (raw.order.0, raw.nb_channels, mask, raw.opaque as usize)
    }

    /// A custom map of `ids`, each named `n<i>`.
    fn named_map(ids: &[Channel]) -> HeapVec<ChannelCustom> {
        let mut map =
            super::super::mem::alloc_cell_array_zeroed::<ChannelCustom>(ids.len()).unwrap();
        let mut handles = map.as_handles_mut();
        for (i, &id) in ids.iter().enumerate() {
            let mut m = handles.get_mut(i).unwrap();
            m.set_id(id);
            let name = CString::new(format!("n{i}")).unwrap();
            assert_eq!(m.set_name(&name), Ok(()));
        }
        map
    }

    fn cookie(addr: usize) -> *mut c_void {
        ptr::without_provenance_mut(addr)
    }

    /// Whether `av_channel_layout_from_string` may be given a description it
    /// parses through glibc `strtol` end pointers (the `ambisonic` forms and
    /// whatever `parse_channel_list` rejects): not under BSan, which reports
    /// a stale stack shadow there as a use after free (see
    /// `tests/common/channel_layout.rs`).
    fn c_parser_takes_numbers() -> bool {
        std::env::var("CRUSTIFY_FFMPEG_VARIANT").as_deref() != Ok("bsan")
    }

    #[test]
    fn channel_layout_layout_is_the_c_struct() {
        use core::mem::offset_of;
        assert_eq!(
            size_of::<ChannelLayout>(),
            size_of::<ffi::AVChannelLayout>()
        );
        assert_eq!(size_of::<ChannelLayout>(), 24);
        assert_eq!(
            align_of::<ChannelLayout>(),
            align_of::<ffi::AVChannelLayout>()
        );
        assert_eq!(offset_of!(ffi::AVChannelLayout, u), 8);
        assert_eq!(offset_of!(ffi::AVChannelLayout, opaque), 16);
        // The owner holds the value inline, with a zero-sized policy.
        assert_eq!(
            size_of::<ChannelLayoutVal>(),
            size_of::<ffi::AVChannelLayout>()
        );
    }

    #[test]
    fn channel_layout_unspecified_and_zeroed_hold_only_a_count() {
        assert_eq!(layout_raw(ChannelLayout::zeroed().as_ref()), (0, 0, 0, 0));
        for n in [0, 1, 7, -1, i32::MIN, i32::MAX] {
            let l = ChannelLayout::unspecified(n);
            assert_eq!(layout_raw(l.as_ref()), (0, n, 0, 0), "{n}");
            assert_eq!(l.as_ref().order(), ChannelOrder::UNSPEC);
            assert_eq!(l.as_ref().channel_count(), n);
            assert!(matches!(l.as_ref().details(), LayoutDetails::Unspecified));
            assert!(l.as_ref().opaque().is_null());
        }
    }

    #[test]
    fn channel_layout_debug_names_every_field_and_variant() {
        let mut l = ChannelLayout::unspecified(2);
        assert_eq!(
            format!("{:?}", l.as_ref()),
            "ChannelLayout { order: UNSPEC, nb_channels: 2, details: Unspecified, opaque: 0x0 }"
        );
        l.as_mut().set_native(0x3);
        assert_eq!(format!("{:?}", l.as_ref().details()), "Native(0x3)");
        assert!(l.as_mut().set_ambisonic(1, 0x4).is_ok());
        assert_eq!(format!("{:?}", l.as_ref().details()), "Ambisonic(0x4)");
        l.as_mut()
            .set_custom_map(named_map(&[Channel::FRONT_LEFT]))
            .unwrap();
        assert_eq!(
            format!("{:?}", l.as_ref().details()),
            "Custom([ChannelCustom { id: FRONT_LEFT, name: \"n0\", opaque: 0x0 }])"
        );
        let value = l.into_inner();
        assert!(
            format!("{value:?}").starts_with("ChannelLayout { order: CUSTOM, nb_channels: 1, ")
        );
        // `into_inner` gave up disposal: hand the map back to an owner.
        drop(ChannelLayoutVal::new(value));
        let unknown = ChannelLayout::masked(
            ChannelOrder::from_raw(ffi::AVChannelOrder(9)),
            0,
            0,
            ptr::null_mut(),
        );
        let unknown = ChannelLayout::from_parts(unknown);
        assert_eq!(
            format!("{:?}", unknown.as_ref().details()),
            "Unknown(ChannelOrder(9))"
        );
    }

    #[test]
    fn channel_layout_custom_map_setter_refuses_an_empty_map_untouched() {
        let mut l = ChannelLayout::unspecified(3);
        l.as_mut().set_opaque(cookie(8));
        let back = l.as_mut().set_custom_map(HeapVec::empty()).unwrap_err();
        assert!(back.is_empty());
        assert_eq!(layout_raw(l.as_ref()), (0, 3, 0, 8));
        assert!(ChannelLayout::from_custom_map(HeapVec::empty()).is_err());
    }

    #[test]
    fn channel_layout_custom_map_moves_in_and_out_keeping_the_user_pointer() {
        let ids = [
            Channel::FRONT_LEFT,
            Channel::from_raw(ffi::AVChannel(70)),
            Channel::UNKNOWN,
        ];
        let mut l = ChannelLayout::from_custom_map(named_map(&ids)).unwrap();
        l.as_mut().set_opaque(cookie(16));
        assert_eq!(l.as_ref().order(), ChannelOrder::CUSTOM);
        assert_eq!(l.as_ref().channel_count(), 3);
        let LayoutDetails::Custom(map) = l.as_ref().details() else {
            panic!()
        };
        let got: Vec<_> = map.iter().map(|c| (c.id(), c.name())).collect();
        assert_eq!(got.len(), 3);
        assert_eq!((got[1].0, got[1].1.as_c_str()), (ids[1], c"n1"));
        // A replacing map frees the old one; the user pointer stays.
        l.as_mut().set_custom_map(named_map(&ids[..1])).unwrap();
        assert_eq!(layout_raw(l.as_ref()).1, 1);
        let map = l.as_mut().take_custom_map().unwrap();
        assert_eq!(map.len(), 1);
        assert_eq!(map.as_handles().get(0).unwrap().name(), c"n0");
        assert_eq!(layout_raw(l.as_ref()), (0, 1, 0, 16));
        // Only a custom layout has a map to take.
        assert!(l.as_mut().take_custom_map().is_none());
        assert_eq!(layout_raw(l.as_ref()), (0, 1, 0, 16));
        assert!(l.as_mut().custom_map_mut().is_none());
    }

    #[test]
    fn channel_layout_order_setters_keep_the_user_pointer_and_free_the_map() {
        let mut l = ChannelLayout::from_custom_map(named_map(&[Channel::BACK_LEFT; 4])).unwrap();
        l.as_mut().set_opaque(cookie(24));
        l.as_mut().set_native(0b1011);
        assert_eq!(layout_raw(l.as_ref()), (1, 3, 0b1011, 24));
        l.as_mut().set_native(0);
        assert_eq!(layout_raw(l.as_ref()), (1, 0, 0, 24));
        l.as_mut().set_native(u64::MAX);
        assert_eq!(layout_raw(l.as_ref()), (1, 64, u64::MAX, 24));
        l.as_mut()
            .set_custom_map(named_map(&[Channel::BACK_LEFT; 2]))
            .unwrap();
        l.as_mut().set_unspecified(-5);
        assert_eq!(layout_raw(l.as_ref()), (0, -5, 0, 24));
        l.as_mut()
            .set_custom_map(named_map(&[Channel::BACK_LEFT; 2]))
            .unwrap();
        assert_eq!(l.as_mut().set_ambisonic(2, 0b11), Ok(()));
        assert_eq!(layout_raw(l.as_ref()), (3, 11, 0b11, 24));
        l.as_mut().clear();
        assert_eq!(layout_raw(l.as_ref()), (0, 0, 0, 0));
    }

    #[test]
    fn channel_layout_ambisonic_setter_bounds_the_count_as_cs_parser() {
        let mut l = ChannelLayout::unspecified(1);
        assert_eq!(l.as_mut().set_ambisonic(0, 0), Ok(()));
        assert_eq!(layout_raw(l.as_ref()), (3, 1, 0, 0));
        // 46340^2 fits an `int`; 46341^2 does not.
        assert_eq!(l.as_mut().set_ambisonic(46339, u64::MAX), Ok(()));
        assert_eq!(layout_raw(l.as_ref()), (3, 46340 * 46340 + 64, u64::MAX, 0));
        for order in [46340, 46341, u32::MAX / 2, u32::MAX] {
            l.as_mut().set_unspecified(9);
            assert_eq!(l.as_mut().set_ambisonic(order, 1), Err(EINVAL), "{order}");
            assert_eq!(layout_raw(l.as_ref()), (0, 9, 0, 0));
        }
    }

    #[test]
    fn channel_layout_replace_take_and_set_move_whole_layouts() {
        let mut a = ChannelLayout::from_custom_map(named_map(&[Channel::FRONT_CENTER])).unwrap();
        a.as_mut().set_opaque(cookie(32));
        let old = a.as_mut().replace(ChannelLayout::unspecified(6));
        assert_eq!(layout_raw(a.as_ref()), (0, 6, 0, 0));
        assert_eq!(old.as_ref().order(), ChannelOrder::CUSTOM);
        assert_eq!(old.as_ref().opaque(), cookie(32));
        a.as_mut().set(old);
        assert_eq!(a.as_ref().channel_count(), 1);
        let taken = a.as_mut().take();
        assert_eq!(layout_raw(a.as_ref()), (0, 0, 0, 0));
        let LayoutDetails::Custom(map) = taken.as_ref().details() else {
            panic!()
        };
        assert_eq!(map.get(0).unwrap().id(), Channel::FRONT_CENTER);
    }

    #[test]
    fn channel_layout_copies_are_deep_and_independent() {
        let mut src =
            ChannelLayout::from_custom_map(named_map(&[Channel::SIDE_LEFT, Channel::SIDE_RIGHT]))
                .unwrap();
        src.as_mut().set_opaque(cookie(40));
        let copy = src.as_ref().try_clone().unwrap();
        let (LayoutDetails::Custom(a), LayoutDetails::Custom(b)) =
            (src.as_ref().details(), copy.as_ref().details())
        else {
            panic!()
        };
        assert_ne!(a.as_ptr(), b.as_ptr());
        assert_eq!(a.get(1).unwrap().get(), b.get(1).unwrap().get());
        assert_eq!(copy.as_ref().opaque(), cookie(40));
        // Writing the source leaves the copy alone.
        src.as_mut()
            .custom_map_mut()
            .unwrap()
            .get_mut(1)
            .unwrap()
            .set_id(Channel::BACK_CENTER);
        drop(src);
        let LayoutDetails::Custom(b) = copy.as_ref().details() else {
            panic!()
        };
        assert_eq!(b.get(1).unwrap().id(), Channel::SIDE_RIGHT);
        // Copying over a custom layout frees its map first.
        let mut dst = ChannelLayout::from_custom_map(named_map(&[Channel::TOP_CENTER; 5])).unwrap();
        let native = ChannelLayout::from_mask(0x7).unwrap();
        assert_eq!(dst.as_mut().copy_from(native.as_ref()), Ok(()));
        assert_eq!(layout_raw(dst.as_ref()), (1, 3, 0x7, 0));
        assert_eq!(dst.as_mut().copy_from(copy.as_ref()), Ok(()));
        assert_eq!(dst.as_ref().channel_count(), 2);
    }

    /// A custom layout C left without its map, as `av_channel_layout_copy`
    /// does when the allocation fails: the readers must not follow it.
    #[test]
    fn channel_layout_mapless_custom_state_is_read_as_empty_and_not_copied() {
        let mut raw = ffi::AVChannelLayout {
            order: ffi::AVChannelOrder::AV_CHANNEL_ORDER_CUSTOM,
            nb_channels: 2,
            u: ffi::AVChannelLayout__bindgen_ty_1 {
                map: ptr::null_mut(),
            },
            opaque: cookie(48),
        };
        {
            // SAFETY: `raw` is a live, initialised layout on this frame,
            // outliving the handle, and nothing else touches it meanwhile; it
            // owns no map, so nothing below frees one.
            let mut m = unsafe { ChannelLayoutMut::from_ptr(&raw mut raw) }.unwrap();
            let LayoutDetails::Custom(map) = m.as_ref().details() else {
                panic!()
            };
            assert!(map.is_empty());
            assert!(m.custom_map_mut().unwrap().is_empty());
            assert_eq!(m.as_ref().try_clone().unwrap_err(), EINVAL);
            let mut dst = ChannelLayout::from_mask(0x3).unwrap();
            assert_eq!(dst.as_mut().copy_from(m.as_ref()), Err(EINVAL));
            assert_eq!(layout_raw(dst.as_ref()), (1, 2, 0x3, 0));
            assert!(m.take_custom_map().is_none());
        }
        assert_eq!(raw.order, ffi::AVChannelOrder::AV_CHANNEL_ORDER_UNSPEC);
        assert_eq!((raw.nb_channels, raw.opaque), (2, cookie(48)));
    }

    /// A custom count C stored negative reads as an empty map.
    #[test]
    fn channel_layout_negative_custom_count_reads_as_empty() {
        let mut map = named_map(&[Channel::FRONT_LEFT]);
        let mut raw = ffi::AVChannelLayout {
            order: ffi::AVChannelOrder::AV_CHANNEL_ORDER_CUSTOM,
            nb_channels: -1,
            u: ffi::AVChannelLayout__bindgen_ty_1 {
                map: map.as_handles_mut().as_mut_ptr(),
            },
            opaque: ptr::null_mut(),
        };
        // SAFETY: `raw` is live on this frame and borrows `map`'s block, which
        // outlives the handle; nothing frees through it.
        let m = unsafe { ChannelLayoutRef::from_ptr(&raw mut raw) }.unwrap();
        let LayoutDetails::Custom(run) = m.details() else {
            panic!()
        };
        assert!(run.is_empty());
    }

    #[test]
    fn channel_layout_c_initialisers_fill_a_fresh_layout() {
        let l = ChannelLayout::from_mask(0x60f).unwrap();
        assert_eq!(layout_raw(l.as_ref()), (1, 6, 0x60f, 0));
        assert_eq!(ChannelLayout::from_mask(0).unwrap_err(), EINVAL);
        let l = ChannelLayout::custom(3).unwrap();
        let LayoutDetails::Custom(map) = l.as_ref().details() else {
            panic!()
        };
        assert!(
            map.iter()
                .all(|c| c.id() == Channel::UNKNOWN && c.name().is_empty())
        );
        for n in [0, -1, i32::MIN] {
            assert_eq!(ChannelLayout::custom(n).unwrap_err(), EINVAL, "{n}");
        }
        let l = ChannelLayout::default_for(2);
        assert_eq!(layout_raw(l.as_ref()), (1, 2, 0x3, 0));
        let l = ChannelLayout::default_for(-4);
        assert_eq!(layout_raw(l.as_ref()), (0, -4, 0, 0));
        let l = ChannelLayout::parse(c"FL@x+FR").unwrap();
        assert_eq!(l.as_ref().order(), ChannelOrder::CUSTOM);
        assert_eq!(l.as_ref().channel_count(), 2);
        assert!(ChannelLayout::parse(c"FL+NOPE").is_err());
        if c_parser_takes_numbers() {
            let l = ChannelLayout::parse(c"ambisonic 1+FL@x").unwrap();
            assert_eq!(l.as_ref().order(), ChannelOrder::CUSTOM);
            assert_eq!(l.as_ref().channel_count(), 5);
            assert!(ChannelLayout::parse(c"no such layout").is_err());
        }
    }

    /// Allocation failures, under a process-wide `av_max_alloc` limit, so in
    /// an isolated child.
    #[test]
    fn channel_layout_allocation_failures_leave_valid_layouts() {
        const CHILD: &str = "CRUSTIFY_CHANNEL_LAYOUT_ALLOCATION_FAILURE_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "libavutil::channel_layout::unit_tests::channel_layout_allocation_failures_leave_valid_layouts",
                    "--test-threads=1",
                ])
                .env(CHILD, "1")
                .status()
                .unwrap();
            assert!(status.success());
            return;
        }
        const ENOMEM: i32 = -12;
        let src = ChannelLayout::from_custom_map(named_map(&[Channel::FRONT_LEFT; 3])).unwrap();
        let mut dst = ChannelLayout::from_custom_map(named_map(&[Channel::BACK_LEFT; 2])).unwrap();
        dst.as_mut().set_opaque(cookie(56));
        super::super::mem::set_max_alloc(0);
        assert_eq!(src.as_ref().try_clone().unwrap_err(), ENOMEM);
        // C uninits `dst` and leaves it mapless; the wrapper clears it.
        assert_eq!(dst.as_mut().copy_from(src.as_ref()), Err(ENOMEM));
        assert_eq!(layout_raw(dst.as_ref()), (0, 0, 0, 0));
        assert_eq!(ChannelLayout::custom(2).unwrap_err(), ENOMEM);
        assert_eq!(ChannelLayout::parse(c"FL+FR").unwrap_err(), ENOMEM);
        if c_parser_takes_numbers() {
            assert_eq!(
                ChannelLayout::parse(c"ambisonic 1+FL@x").unwrap_err(),
                ENOMEM
            );
        }
        // Native copies allocate nothing.
        let native = ChannelLayout::from_mask(0x3).unwrap();
        assert_eq!(
            layout_raw(native.as_ref().try_clone().unwrap().as_ref()),
            (1, 2, 0x3, 0)
        );
        super::super::mem::set_max_alloc(i32::MAX as usize);
        assert!(src.as_ref().try_clone().is_ok());
    }

    #[test]
    fn channel_layout_values_owners_and_handles_cross_threads() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<ChannelLayout>();
        assert_send_sync::<ChannelLayoutVal>();
        assert_send_sync::<ChannelLayoutRef<'static>>();
        assert_send_sync::<ChannelLayoutMut<'static>>();
        assert_send_sync::<LayoutDetails<'static>>();
        assert_send_sync::<ChannelLayoutUninit>();
    }

    #[test]
    fn channel_mask_is_the_bit_of_ids_below_64() {
        for raw in (-3..0)
            .chain(64..70)
            .chain([c_int::MIN, 0x200, 0x300, 0x400, c_int::MAX])
        {
            assert_eq!(channel(raw).mask(), None, "raw {raw}");
        }
        for raw in 0..64 {
            assert_eq!(channel(raw).mask(), Some(1u64 << raw), "raw {raw}");
        }
        for (ch, raw, _) in CHANNEL_NAMED {
            let want = (0..64).contains(&raw).then(|| 1u64 << raw);
            assert_eq!(ch.mask(), want, "{ch:?}");
        }
        assert_eq!(Channel::NONE.mask(), None);
        assert_eq!(Channel::AMBISONIC_BASE.mask(), None);
        assert_eq!(Channel::BINAURAL_RIGHT.mask(), Some(1 << 62));
        // `mask` is usable in constants.
        const FL: Option<u64> = Channel::FRONT_LEFT.mask();
        assert_eq!(FL, Some(1));
    }

    #[test]
    fn masks_compose_as_the_header_and_count_their_channels() {
        let bit = |ch: Channel| ch.mask().unwrap();
        assert_eq!(masks::MONO, bit(Channel::FRONT_CENTER));
        assert_eq!(masks::STEREO, 0x3);
        assert_eq!(masks::L2POINT1, masks::STEREO | bit(Channel::LOW_FREQUENCY));
        assert_eq!(masks::L2_1, masks::STEREO | bit(Channel::BACK_CENTER));
        assert_eq!(masks::L5POINT1, 0x60f);
        assert_eq!(masks::L7POINT1, 0x63f);
        assert_eq!(masks::L5POINT1_BACK, 0x3f);
        assert_eq!(masks::QUAD, 0x33);
        assert_eq!(masks::STEREO_DOWNMIX, (1 << 29) | (1 << 30));
        assert_eq!(masks::BINAURAL, (1 << 61) | (1 << 62));
        assert_eq!(masks::L22POINT2.count_ones(), 24);
        assert_eq!(masks::HEXADECAGONAL.count_ones(), 16);
        assert_eq!(masks::L9POINT1POINT6.count_ones(), 16);
        // Every mask holds named channels only, and the header's supersets
        // hold their subsets.
        let named: u64 = CHANNEL_NAMED
            .iter()
            .filter_map(|&(ch, _, _)| ch.mask())
            .fold(0, |m, b| m | b);
        for (name, mask) in [
            ("L7POINT1POINT4", masks::L7POINT1POINT4),
            ("L9POINT1POINT4", masks::L9POINT1POINT4),
            ("CUBE", masks::CUBE),
            ("OCTAGONAL", masks::OCTAGONAL),
        ] {
            assert_eq!(mask & !named, 0, "{name}");
        }
        assert_eq!(masks::L7POINT1POINT4 & masks::L7POINT1, masks::L7POINT1);
        assert_eq!(
            masks::L22POINT2 & masks::L9POINT1POINT6,
            masks::L9POINT1POINT6
        );
        assert_eq!(masks::CUBE & masks::QUAD, masks::QUAD);
    }

    #[test]
    fn standard_layouts_are_native_over_their_masks() {
        for (name, layout, mask) in [
            ("MONO", ChannelLayout::MONO, masks::MONO),
            ("STEREO", ChannelLayout::STEREO, masks::STEREO),
            ("L2POINT1", ChannelLayout::L2POINT1, masks::L2POINT1),
            ("L5POINT1", ChannelLayout::L5POINT1, masks::L5POINT1),
            (
                "L7POINT1_WIDE_BACK",
                ChannelLayout::L7POINT1_WIDE_BACK,
                masks::L7POINT1_WIDE_BACK,
            ),
            (
                "HEXADECAGONAL",
                ChannelLayout::HEXADECAGONAL,
                masks::HEXADECAGONAL,
            ),
            (
                "STEREO_DOWNMIX",
                ChannelLayout::STEREO_DOWNMIX,
                masks::STEREO_DOWNMIX,
            ),
            ("L22POINT2", ChannelLayout::L22POINT2, masks::L22POINT2),
        ] {
            assert_eq!(
                layout_raw(layout.as_ref()),
                (1, mask.count_ones() as c_int, mask, 0),
                "{name}"
            );
            assert!(matches!(layout.as_ref().details(), LayoutDetails::Native(m) if m == mask));
        }
        assert_eq!(
            layout_raw(ChannelLayout::AMBISONIC_FIRST_ORDER.as_ref()),
            (3, 4, 0, 0)
        );
        assert!(matches!(
            ChannelLayout::AMBISONIC_FIRST_ORDER.as_ref().details(),
            LayoutDetails::Ambisonic(0)
        ));
        // `native` counts any mask, the empty one included, as `set_native`.
        for mask in [0, 1, 0x3, 1 << 63, u64::MAX, 0x8000_0000_0000_0001] {
            let l = ChannelLayout::native(mask);
            assert_eq!(
                layout_raw(l.as_ref()),
                (1, mask.count_ones() as c_int, mask, 0)
            );
            let mut set = ChannelLayout::unspecified(9);
            set.as_mut().set_native(mask);
            assert_eq!(layout_raw(set.as_ref()), layout_raw(l.as_ref()));
        }
        const QUAD: ChannelLayout = ChannelLayout::native(masks::QUAD);
        assert_eq!(QUAD.as_ref().channel_count(), 4);
        // A constant is a fresh value at each use: an owner made of it and
        // written stays its own.
        let mut owned = ChannelLayoutVal::from(ChannelLayout::STEREO);
        owned.as_mut().set_opaque(cookie(64));
        owned.as_mut().set(ChannelLayout::L5POINT1.into());
        assert_eq!(layout_raw(owned.as_ref()), (1, 6, masks::L5POINT1, 0));
        assert_eq!(layout_raw(ChannelLayout::STEREO.as_ref()), (1, 2, 0x3, 0));
        let copy = ChannelLayout::L7POINT1.as_ref().try_clone().unwrap();
        assert_eq!(layout_raw(copy.as_ref()), (1, 8, masks::L7POINT1, 0));
    }

    #[test]
    fn retype_flags_are_the_header_values() {
        assert_eq!((RETYPE_LOSSLESS, RETYPE_CANONICAL), (1 << 0, 1 << 1));
        assert_eq!(format!("{:?}", Conversion::Lossy), "Lossy");
        assert_ne!(Conversion::Lossy, Conversion::Lossless);
    }

    /// A custom layout C left without a map (after its own failed copy): the
    /// subset query answers before C walks the NULL map, and C's retype
    /// refuses it untouched.
    #[test]
    fn subset_and_retype_refuse_a_mapless_custom_layout() {
        let raw = ffi::AVChannelLayout {
            order: ChannelOrder::CUSTOM.into_raw(),
            nb_channels: 3,
            u: ffi::AVChannelLayout__bindgen_ty_1 {
                map: ptr::null_mut(),
            },
            opaque: cookie(40),
        };
        let mut l = ChannelLayoutVal::new(ChannelLayout::from_parts(raw));
        assert_eq!(l.as_ref().subset(u64::MAX), 0);
        assert_eq!(l.as_ref().subset(0), 0);
        for order in [
            ChannelOrder::UNSPEC,
            ChannelOrder::NATIVE,
            ChannelOrder::CUSTOM,
            ChannelOrder::AMBISONIC,
        ] {
            assert_eq!(l.as_mut().retype(order), Err(EINVAL), "{order:?}");
            assert_eq!(l.as_mut().retype_lossless(order), Err(EINVAL), "{order:?}");
        }
        assert_eq!(l.as_mut().canonicalize(), Err(EINVAL));
        assert_eq!(layout_raw(l.as_ref()), (2, 3, 0, 40));
        // `av_channel_layout_uninit` frees the NULL map as a no-op on drop.
    }

    #[test]
    fn standard_layouts_iterator_is_fused_and_copies_resume_independently() {
        let mut it = standard_layouts();
        assert_eq!(format!("{it:?}"), "StandardLayouts { next: 0 }");
        let first: Vec<_> = it.by_ref().take(3).map(|l| layout_raw(l)).collect();
        assert_eq!(first[0], (1, 1, masks::MONO, 0));
        assert_eq!(first[1], (1, 2, masks::STEREO, 0));
        let copy = it;
        let rest: Vec<_> = it.map(layout_raw).collect();
        assert_eq!(copy.map(layout_raw).collect::<Vec<_>>(), rest);
        assert_eq!(first.len() + rest.len(), 40);
        assert_eq!(*rest.last().unwrap(), (1, 24, masks::L22POINT2, 0));
        // C leaves its index at the end, so the walk stays finished.
        let mut done = standard_layouts();
        done.by_ref().for_each(drop);
        assert!(done.next().is_none());
        assert!(done.next().is_none());
        assert_eq!(format!("{done:?}"), "StandardLayouts { next: 40 }");
    }

    /// A raw layout of `order`, `nb_channels` and the union bits `u`, with
    /// no user pointer: states no safe path leaves, for the readers' guards.
    fn raw_layout(order: ChannelOrder, nb_channels: c_int, u: u64) -> ChannelLayoutVal {
        let raw = ChannelLayout::masked(order, nb_channels, u, ptr::null_mut());
        ChannelLayoutVal::new(ChannelLayout::from_parts(raw))
    }

    /// Every reader refuses, before calling C.
    fn assert_refused(l: ChannelLayoutRef<'_>) {
        assert_eq!(l.ambisonic_order(), None);
        assert_eq!(l.channel(0), None);
        assert_eq!(l.channel(1), None);
        assert_eq!(l.index_of(Channel::FRONT_LEFT), None);
        assert_eq!(l.index_by_name(c"FL"), None);
        assert_eq!(l.channel_by_name(c"@x"), None);
        assert_eq!(l.description(), Err(EINVAL));
        assert_eq!(l.write_description(&mut [0; 8]), Err(EINVAL));
        let same = l;
        assert!(l != same, "equals nothing, itself included");
    }

    /// A custom layout C left without a map, and one whose negative count
    /// C would compare an `unsigned` index with: the readers answer before C
    /// reads through the NULL map or past the real one.
    #[test]
    fn readers_refuse_custom_layouts_c_cannot_walk() {
        let mapless = raw_layout(ChannelOrder::CUSTOM, 3, 0);
        assert_refused(mapless.as_ref());
        // C checks the map itself.
        assert!(!mapless.as_ref().is_valid());
        let map = named_map(&[Channel::FRONT_LEFT, Channel::FRONT_RIGHT]);
        let (map, _) = map.into_raw_parts();
        let raw = ffi::AVChannelLayout {
            order: ChannelOrder::CUSTOM.into_raw(),
            nb_channels: -2,
            u: ffi::AVChannelLayout__bindgen_ty_1 { map: map.cast() },
            opaque: ptr::null_mut(),
        };
        let negative = ChannelLayoutVal::new(ChannelLayout::from_parts(raw));
        assert_refused(negative.as_ref());
        assert!(!negative.as_ref().is_valid());
        assert!(negative.as_ref() != mapless.as_ref());
        // `av_channel_layout_uninit` frees the map on drop, whatever the
        // count.
    }

    /// Native and ambisonic counts at the `int` extremes: the readers answer
    /// what C answers where C's `int` arithmetic does not overflow, and
    /// refuse where it would.
    #[test]
    fn readers_keep_cs_int_arithmetic_in_range() {
        let ambi = |n| raw_layout(ChannelOrder::AMBISONIC, n, 0);
        // `highest_ambi` at the limit's edges: 46339 squares within `int`.
        let top = c_int::try_from(AMBISONIC_HIGHEST_LIMIT).unwrap();
        let l = ambi(top);
        assert_eq!(l.as_ref().ambisonic_order(), Some(46_339));
        assert_eq!(
            l.as_ref().description().unwrap().as_bytes(),
            b"ambisonic 46339"
        );
        for n in [top + 1, c_int::MAX, 0, -1, c_int::MIN] {
            let l = ambi(n);
            assert_eq!(l.as_ref().ambisonic_order(), None, "{n}");
            assert_eq!(l.as_ref().description(), Err(EINVAL), "{n}");
        }
        // The components are still indexed, which needs no order.
        let l = ambi(c_int::MAX);
        let acn5 = Channel::ambisonic(5).unwrap();
        assert_eq!(l.as_ref().channel(5), Some(acn5));
        assert_eq!(l.as_ref().index_of(acn5), Some(5));
        assert!(l.as_ref().is_valid());
        // A count so negative that `nb_channels - popcount(mask)` overflows.
        for (order, n) in [
            (ChannelOrder::NATIVE, c_int::MIN),
            (ChannelOrder::AMBISONIC, c_int::MIN + 3),
        ] {
            let l = raw_layout(order, n, u64::MAX);
            assert_refused(l.as_ref());
            assert!(!l.as_ref().is_valid());
        }
        // One channel short of overflowing is read by C.
        let l = raw_layout(ChannelOrder::NATIVE, c_int::MIN + 64, u64::MAX);
        assert_eq!(l.as_ref().index_of(Channel::FRONT_LEFT), None);
        assert_eq!(l.as_ref().channel(0), Some(Channel::FRONT_LEFT));
    }

    #[test]
    fn map_names_are_compared_only_after_an_at_with_text() {
        for (name, compares) in [
            (c"", false),
            (c"FL", false),
            (c"@", false),
            (c"FL@", false),
            (c"@x", true),
            (c"FL@x", true),
            (c"FL@@", true),
            (c"@@", true),
            (c"a@b@c", true),
        ] {
            assert_eq!(compares_map_names(name), compares, "{name:?}");
        }
    }

    /// A name with no NUL is refused only where C reads it as a string.
    #[test]
    fn unterminated_names_refuse_only_string_readers() {
        let mut map = named_map(&[Channel::FRONT_LEFT, Channel::LOW_FREQUENCY]);
        map.as_handles_mut()
            .get_mut(0)
            .unwrap()
            .set(ChannelCustom::from_raw(ffi::AVChannelCustom {
                id: Channel::FRONT_LEFT.into_raw(),
                name: [b'y' as c_char; 16],
                opaque: ptr::null_mut(),
            }));
        let l = ChannelLayout::from_custom_map(map).unwrap();
        let r = l.as_ref();
        assert!(!r.names_terminated());
        assert_eq!(r.description(), Err(EINVAL));
        let mut b = super::super::bprint::BPrint::new(0, u32::MAX).unwrap();
        assert_eq!(r.append_description(b.as_mut()), Err(EINVAL));
        assert!(b.as_ref().as_bytes().is_empty());
        assert_eq!(r.index_by_name(c"@n1"), None);
        assert_eq!(r.channel_by_name(c"LFE@n1"), None);
        assert_eq!(r.index_by_name(c"LFE"), Some(1));
        // C reads `LFE@` whole as a channel name, which names none.
        assert_eq!(r.index_by_name(c"LFE@"), None);
        assert_eq!(r.index_by_name(c"USR0@"), None);
        assert_eq!(r.channel(0), Some(Channel::FRONT_LEFT));
        assert!(r.is_valid());
        let same = r;
        assert!(r == same);
        // Terminated, the same names are searched.
        let named = ChannelLayout::from_custom_map(named_map(&[
            Channel::FRONT_LEFT,
            Channel::LOW_FREQUENCY,
        ]))
        .unwrap();
        assert!(named.as_ref().names_terminated());
        assert_eq!(named.as_ref().index_by_name(c"@n1"), Some(1));
        assert_eq!(
            named.as_ref().channel_by_name(c"LFE@n1"),
            Some(Channel::LOW_FREQUENCY)
        );
        assert!(named.as_ref() == r);
    }
}
