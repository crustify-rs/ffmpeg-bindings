//! Safe wrappers over `libavutil/channel_layout.h`.
//!
//! [`Channel::name`] and [`Channel::from_name`] convert a channel to and from
//! the abbreviation C writes in channel layout strings; C writes names into a
//! caller buffer, which [`Channel::write_name`] exposes as is, or appends
//! them to a print buffer ([`Channel::append_name`]).

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::hash::{Hash, Hasher};
use core::ptr::{self, addr_of, addr_of_mut};
use core::{cmp, fmt};
use std::ffi::CString;

use ffibox::{CPlainElem, CZeroable, define_ctype};

use super::bprint::BPrintMut;
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
}
