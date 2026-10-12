//! Equivalence of `libavutil/channel_layout.h`'s [`Channel`] with the C enum,
//! as C observes it: through `av_channel_name`, `av_channel_description` and
//! `av_channel_from_string`, and through C-allocated storage holding the
//! values.

use core::ffi::{CStr, c_int, c_void};
use core::mem::size_of;

use ffmpeg::libavutil::channel_layout::Channel;
use ffmpeg::libavutil::ffi;
use ffmpeg::libavutil::mem::{HeapVec, alloc_array_zeroed};

/// Which descriptions C's parser may be given under BSan.
#[path = "../common/channel_layout.rs"]
mod parser_inputs;

/// Every named channel beside its C enumerator and C name.
const CHANNEL_NAMED: [(Channel, ffi::AVChannel, &str); 41] = {
    use ffi::AVChannel as C;
    [
        (Channel::NONE, C::AV_CHAN_NONE, "NONE"),
        (Channel::FRONT_LEFT, C::AV_CHAN_FRONT_LEFT, "FL"),
        (Channel::FRONT_RIGHT, C::AV_CHAN_FRONT_RIGHT, "FR"),
        (Channel::FRONT_CENTER, C::AV_CHAN_FRONT_CENTER, "FC"),
        (Channel::LOW_FREQUENCY, C::AV_CHAN_LOW_FREQUENCY, "LFE"),
        (Channel::BACK_LEFT, C::AV_CHAN_BACK_LEFT, "BL"),
        (Channel::BACK_RIGHT, C::AV_CHAN_BACK_RIGHT, "BR"),
        (
            Channel::FRONT_LEFT_OF_CENTER,
            C::AV_CHAN_FRONT_LEFT_OF_CENTER,
            "FLC",
        ),
        (
            Channel::FRONT_RIGHT_OF_CENTER,
            C::AV_CHAN_FRONT_RIGHT_OF_CENTER,
            "FRC",
        ),
        (Channel::BACK_CENTER, C::AV_CHAN_BACK_CENTER, "BC"),
        (Channel::SIDE_LEFT, C::AV_CHAN_SIDE_LEFT, "SL"),
        (Channel::SIDE_RIGHT, C::AV_CHAN_SIDE_RIGHT, "SR"),
        (Channel::TOP_CENTER, C::AV_CHAN_TOP_CENTER, "TC"),
        (Channel::TOP_FRONT_LEFT, C::AV_CHAN_TOP_FRONT_LEFT, "TFL"),
        (
            Channel::TOP_FRONT_CENTER,
            C::AV_CHAN_TOP_FRONT_CENTER,
            "TFC",
        ),
        (Channel::TOP_FRONT_RIGHT, C::AV_CHAN_TOP_FRONT_RIGHT, "TFR"),
        (Channel::TOP_BACK_LEFT, C::AV_CHAN_TOP_BACK_LEFT, "TBL"),
        (Channel::TOP_BACK_CENTER, C::AV_CHAN_TOP_BACK_CENTER, "TBC"),
        (Channel::TOP_BACK_RIGHT, C::AV_CHAN_TOP_BACK_RIGHT, "TBR"),
        (Channel::STEREO_LEFT, C::AV_CHAN_STEREO_LEFT, "DL"),
        (Channel::STEREO_RIGHT, C::AV_CHAN_STEREO_RIGHT, "DR"),
        (Channel::WIDE_LEFT, C::AV_CHAN_WIDE_LEFT, "WL"),
        (Channel::WIDE_RIGHT, C::AV_CHAN_WIDE_RIGHT, "WR"),
        (
            Channel::SURROUND_DIRECT_LEFT,
            C::AV_CHAN_SURROUND_DIRECT_LEFT,
            "SDL",
        ),
        (
            Channel::SURROUND_DIRECT_RIGHT,
            C::AV_CHAN_SURROUND_DIRECT_RIGHT,
            "SDR",
        ),
        (Channel::LOW_FREQUENCY_2, C::AV_CHAN_LOW_FREQUENCY_2, "LFE2"),
        (Channel::TOP_SIDE_LEFT, C::AV_CHAN_TOP_SIDE_LEFT, "TSL"),
        (Channel::TOP_SIDE_RIGHT, C::AV_CHAN_TOP_SIDE_RIGHT, "TSR"),
        (
            Channel::BOTTOM_FRONT_CENTER,
            C::AV_CHAN_BOTTOM_FRONT_CENTER,
            "BFC",
        ),
        (
            Channel::BOTTOM_FRONT_LEFT,
            C::AV_CHAN_BOTTOM_FRONT_LEFT,
            "BFL",
        ),
        (
            Channel::BOTTOM_FRONT_RIGHT,
            C::AV_CHAN_BOTTOM_FRONT_RIGHT,
            "BFR",
        ),
        (
            Channel::SIDE_SURROUND_LEFT,
            C::AV_CHAN_SIDE_SURROUND_LEFT,
            "SSL",
        ),
        (
            Channel::SIDE_SURROUND_RIGHT,
            C::AV_CHAN_SIDE_SURROUND_RIGHT,
            "SSR",
        ),
        (
            Channel::TOP_SURROUND_LEFT,
            C::AV_CHAN_TOP_SURROUND_LEFT,
            "TTL",
        ),
        (
            Channel::TOP_SURROUND_RIGHT,
            C::AV_CHAN_TOP_SURROUND_RIGHT,
            "TTR",
        ),
        (Channel::BINAURAL_LEFT, C::AV_CHAN_BINAURAL_LEFT, "BIL"),
        (Channel::BINAURAL_RIGHT, C::AV_CHAN_BINAURAL_RIGHT, "BIR"),
        (Channel::UNUSED, C::AV_CHAN_UNUSED, "UNSD"),
        (Channel::UNKNOWN, C::AV_CHAN_UNKNOWN, "UNK"),
        (Channel::AMBISONIC_BASE, C::AV_CHAN_AMBISONIC_BASE, "AMBI0"),
        (Channel::AMBISONIC_END, C::AV_CHAN_AMBISONIC_END, "AMBI1023"),
    ]
};

/// Ids C names only as user channels, or not at all: the enum's gaps, the
/// ids around the Ambisonic range and the `int` extremes.
const CHANNEL_UNNAMED: [c_int; 11] = [
    c_int::MIN,
    -2,
    18,
    28,
    45,
    60,
    63,
    0x1ff,
    0x3ff,
    0x800,
    c_int::MAX,
];

/// `av_channel_name` or `av_channel_description` into a buffer, checking
/// the length C returns.
fn c_string(
    f: unsafe extern "C" fn(*mut core::ffi::c_char, usize, ffi::AVChannel) -> c_int,
    ch: ffi::AVChannel,
) -> String {
    let mut buf = [0u8; 64];
    // SAFETY: `buf` is `buf.len()` writable bytes, all `f` writes; it
    // accepts every channel id.
    let ret = unsafe { f(buf.as_mut_ptr().cast(), buf.len(), ch) };
    let s = CStr::from_bytes_until_nul(&buf).unwrap().to_str().unwrap();
    assert_eq!(ret, s.len() as c_int + 1);
    s.to_owned()
}

/// What C calls `ch`: `av_channel_name`.
fn c_name(ch: ffi::AVChannel) -> String {
    c_string(ffi::av_channel_name, ch)
}

/// How C describes `ch`: `av_channel_description`.
fn c_description(ch: ffi::AVChannel) -> String {
    c_string(ffi::av_channel_description, ch)
}

/// The channel C parses `name` as: `av_channel_from_string`.
fn c_from_string(name: &str) -> ffi::AVChannel {
    let name = std::ffi::CString::new(name).unwrap();
    // SAFETY: `name` is a NUL-terminated string `av_channel_from_string`
    // only reads.
    unsafe { ffi::av_channel_from_string(name.as_ptr()) }
}

/// Every channel the tests compare, named first.
fn values() -> Vec<ffi::AVChannel> {
    CHANNEL_NAMED
        .iter()
        .map(|&(_, c, _)| c)
        .chain(CHANNEL_UNNAMED.map(ffi::AVChannel))
        .chain([ffi::AVChannel(0x401), ffi::AVChannel(0x5ab)])
        .collect()
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn channel_constants_are_the_c_enumerators() {
    for (ch, c, name) in CHANNEL_NAMED {
        assert_eq!(ch.into_raw(), c);
        assert_eq!(Channel::from_raw(c), ch);
        assert_eq!(c_name(ch.into_raw()), name);
        assert_eq!(c_name(ch.into_raw()), c_name(c));
        assert_eq!(c_description(ch.into_raw()), c_description(c));
        assert_eq!(Channel::from_raw(c_from_string(name)), ch, "{name}");
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn channel_default_is_what_c_returns_for_an_unknown_channel() {
    // `av_channel_from_string` answers a name it cannot parse, a negative
    // user id, or an out-of-range Ambisonic index with `AV_CHAN_NONE`.
    for name in ["", "no-such", "USR-1", "USRx", "AMBI1024", "AMBI-1", "fl"] {
        assert_eq!(
            Channel::from_raw(c_from_string(name)),
            Channel::default(),
            "{name}"
        );
    }
    assert_eq!(Channel::default().into_raw(), ffi::AVChannel::AV_CHAN_NONE);
    assert_eq!(c_name(Channel::default().into_raw()), "NONE");
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn channel_unnamed_values_are_held_unchanged() {
    for raw in CHANNEL_UNNAMED {
        let ch = Channel::from_raw(ffi::AVChannel(raw));
        assert!(CHANNEL_NAMED.iter().all(|&(n, _, _)| n != ch));
        assert_eq!(ch.into_raw().0, raw);
        assert_eq!(format!("{ch:?}"), format!("Channel({raw})"));
        let name = c_name(ch.into_raw());
        assert_eq!(name, format!("USR{raw}"));
        assert_eq!(c_description(ch.into_raw()), format!("user {raw}"));
        // C parses a non-negative user id back to the same channel.
        let back = Channel::from_raw(c_from_string(&name));
        assert_eq!(back == ch, raw >= 0, "{name}");
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn ambisonic_is_cs_ambi_mapping() {
    for acn in 0..=1023u16 {
        let ch = Channel::ambisonic(acn).unwrap();
        let name = format!("AMBI{acn}");
        assert_eq!(ch.into_raw(), c_from_string(&name));
        assert_eq!(c_name(ch.into_raw()), name);
        assert_eq!(c_description(ch.into_raw()), format!("ambisonic ACN {acn}"));
        assert_eq!(ch.ambisonic_index(), Some(acn));
        assert_eq!(format!("{ch:?}"), format!("AMBISONIC({acn})"));
    }
    // Past the range C refuses the name and Rust the index.
    for acn in [1024u16, 1025, 4096, u16::MAX] {
        assert_eq!(Channel::ambisonic(acn), None);
        let c = c_from_string(&format!("AMBI{acn}"));
        assert_eq!(Channel::from_raw(c), Channel::NONE);
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn ambisonic_index_is_some_exactly_where_c_names_an_ambi() {
    let probes = values().into_iter().chain((-3..=0x900).map(ffi::AVChannel));
    for c in probes {
        let ch = Channel::from_raw(c);
        let name = c_name(c);
        let acn = name.strip_prefix("AMBI").map(|n| n.parse::<u16>().unwrap());
        assert_eq!(ch.ambisonic_index(), acn, "{name}");
    }
}

#[test]
fn channel_order_is_cs_id_order() {
    let values = values();
    for &a in &values {
        for &b in &values {
            let (ra, rb) = (Channel::from_raw(a), Channel::from_raw(b));
            assert_eq!(ra.cmp(&rb), a.0.cmp(&b.0), "{ra:?} vs {rb:?}");
        }
    }
}

/// A C block of `ffi::AVChannel`, freed with `av_free` on drop.
struct CChannels(*mut ffi::AVChannel, usize);

impl CChannels {
    fn zeroed(len: usize) -> Self {
        // SAFETY: `av_calloc` takes its sizes by value.
        let ptr = unsafe { ffi::av_calloc(len, size_of::<ffi::AVChannel>()) };
        assert!(!ptr.is_null());
        Self(ptr.cast(), len)
    }

    fn get(&self, i: usize) -> ffi::AVChannel {
        assert!(i < self.1);
        // SAFETY: `i` is in bounds of the live, initialised block.
        unsafe { self.0.add(i).read() }
    }

    fn set(&mut self, i: usize, ch: ffi::AVChannel) {
        assert!(i < self.1);
        // SAFETY: `i` is in bounds of the live block, which we own.
        unsafe { self.0.add(i).write(ch) }
    }
}

impl Drop for CChannels {
    fn drop(&mut self) {
        // SAFETY: `self.0` is a live `av_calloc` block we own.
        unsafe { ffi::av_free(self.0.cast::<c_void>()) }
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn channel_zeroed_storage_reads_as_c_zeroed_storage() {
    const LEN: usize = 8;
    let rust = alloc_array_zeroed::<Channel>(LEN).unwrap();
    let c = CChannels::zeroed(LEN);
    for (i, ch) in rust.as_slice().iter().enumerate() {
        assert_eq!(ch.into_raw(), c.get(i));
        assert_eq!(c_name(ch.into_raw()), c_name(c.get(i)));
    }
    // A zero-filled `AVChannelCustom`'s id is `AV_CHAN_FRONT_LEFT`.
    assert!(rust.as_slice().iter().all(|&ch| ch == Channel::FRONT_LEFT));
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn channel_writes_through_each_side_agree_after_every_step() {
    let values = values();
    let len = values.len();
    let mut rust = alloc_array_zeroed::<Channel>(len).unwrap();
    let mut c = CChannels::zeroed(len);
    for (step, &v) in values.iter().enumerate() {
        // Each step writes slot `step` and rotates an earlier one.
        let back = (step * 7) % len;
        let w = values[(step + 3) % len];
        rust.as_mut_slice()[step] = Channel::from_raw(v);
        rust.as_mut_slice()[back] = w.into();
        c.set(step, v);
        c.set(back, w);
        for i in 0..len {
            let ri = rust.as_slice()[i];
            assert_eq!(ri.into_raw(), c.get(i), "step {step} slot {i}");
            assert_eq!(c_name(ri.into_raw()), c_name(c.get(i)));
        }
    }
    // A clone is `av_memdup`'s copy of the same encodings.
    let copy = rust.clone();
    drop(rust);
    for i in 0..len {
        assert_eq!(copy.as_slice()[i].into_raw(), c.get(i));
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn channel_values_c_writes_are_read_back_unchanged() {
    let values = values();
    let mut c = CChannels::zeroed(values.len());
    for (i, &v) in values.iter().enumerate() {
        c.set(i, v);
    }
    // The path a field accessor over C-written storage takes: adopt the C
    // block and read it as `Channel`s.
    let (ptr, len) = (c.0, c.1);
    core::mem::forget(c);
    // SAFETY: `ptr` is a live `av_calloc` block of `len` initialised
    // `ffi::AVChannel`, whose ownership `forget` gave up; `Channel` is
    // layout-compatible with it, and `av_free` releases it.
    let adopted = unsafe { HeapVec::<Channel>::from_raw_parts(ptr.cast(), len) }.unwrap();
    for (i, &v) in values.iter().enumerate() {
        assert_eq!(adopted.as_slice()[i], Channel::from_raw(v));
        assert_eq!(c_name(adopted.as_slice()[i].into_raw()), c_name(v));
    }
}

/// [`MatrixEncoding`] against C's `AVMatrixEncoding`, as libswresample's
/// `matrix_encoding` option publishes it: its default, its range, and the
/// value behind each named constant.
///
/// [`MatrixEncoding`]: ffmpeg::libavutil::channel_layout::MatrixEncoding
mod matrix_encoding {
    use core::ffi::{CStr, c_int, c_void};

    use ffmpeg::libavutil::channel_layout::MatrixEncoding;
    use ffmpeg::libavutil::ffi;
    use ffmpeg::libswresample::ffi as swr;

    /// C's option `name` (in `unit` for a named constant) on
    /// libswresample's class, as `(default, min, max)`.
    fn swr_option(name: &CStr, unit: Option<&CStr>) -> (i64, f64, f64) {
        // SAFETY: `swr_get_class` takes nothing and returns C's static class.
        let mut class = unsafe { swr::swr_get_class() };
        let fake_obj = (&raw mut class).cast::<c_void>();
        let unit = unit.map_or(core::ptr::null(), CStr::as_ptr);
        // SAFETY: with `AV_OPT_SEARCH_FAKE_OBJ`, `av_opt_find` takes a
        // pointer to a `const AVClass *` as the object, which `fake_obj` is
        // for the whole call; `name` and `unit` are NUL-terminated or NULL.
        let opt = unsafe {
            ffi::av_opt_find(
                fake_obj,
                name.as_ptr(),
                unit,
                0,
                ffi::AV_OPT_SEARCH_FAKE_OBJ as c_int,
            )
        };
        assert!(!opt.is_null(), "{name:?} not found");
        // SAFETY: `opt` points into libswresample's static option table;
        // both the option and its named constants are integer-valued, so
        // `default_val.i64` is the active union member.
        unsafe { ((*opt).default_val.i64_, (*opt).min, (*opt).max) }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn option_default_and_range_span_the_named_values() {
        let (default, min, max) = swr_option(c"matrix_encoding", None);
        let default = MatrixEncoding::from_raw(ffi::AVMatrixEncoding(default as u32));
        assert_eq!(default, MatrixEncoding::NONE);
        assert_eq!(default, MatrixEncoding::default());
        assert_eq!(default.index(), Some(0));
        assert_eq!((min, max), (0.0, (MatrixEncoding::COUNT - 1) as f64));
        for v in min as u32..=max as u32 {
            let e = MatrixEncoding::from_raw(ffi::AVMatrixEncoding(v));
            assert_eq!(e.index(), Some(v as usize));
            assert!(!format!("{e:?}").starts_with("MatrixEncoding("), "{v}");
        }
        let past = MatrixEncoding::from_raw(ffi::AVMatrixEncoding(max as u32 + 1));
        assert_eq!(past.index(), None);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn named_constants_match_c_s_values_and_names() {
        let named = [
            (c"none", MatrixEncoding::NONE),
            (c"dolby", MatrixEncoding::DOLBY),
            (c"dplii", MatrixEncoding::DPLII),
        ];
        for (name, rust) in named {
            let (value, _, _) = swr_option(name, Some(c"matrix_encoding"));
            let c = MatrixEncoding::from_raw(ffi::AVMatrixEncoding(value as u32));
            assert_eq!(c, rust, "{name:?}");
            assert_eq!(i64::from(rust.into_raw().0), value);
            assert_eq!(format!("{c:?}"), name.to_str().unwrap().to_uppercase());
        }
    }
}

/// [`ChannelOrder`] against the C enum, as C observes it: the order C's
/// layout constructors write into `AVChannelLayout.order`, the orders
/// `av_channel_layout_check` accepts, and C-allocated storage holding the
/// values (formats wave 0, batch 1).
mod channel_order {
    use core::ffi::{CStr, c_int, c_uint, c_void};
    use core::mem::{MaybeUninit, size_of};

    use ffmpeg::libavutil::channel_layout::ChannelOrder;
    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::mem::{HeapVec, alloc_array_zeroed};

    use super::parser_inputs::skipped_under_bsan;

    /// Every named constant beside its C enumerator.
    const NAMED: [(ChannelOrder, ffi::AVChannelOrder); 4] = [
        (
            ChannelOrder::UNSPEC,
            ffi::AVChannelOrder::AV_CHANNEL_ORDER_UNSPEC,
        ),
        (
            ChannelOrder::NATIVE,
            ffi::AVChannelOrder::AV_CHANNEL_ORDER_NATIVE,
        ),
        (
            ChannelOrder::CUSTOM,
            ffi::AVChannelOrder::AV_CHANNEL_ORDER_CUSTOM,
        ),
        (
            ChannelOrder::AMBISONIC,
            ffi::AVChannelOrder::AV_CHANNEL_ORDER_AMBISONIC,
        ),
    ];

    /// Values C may store though it names none of them,
    /// `FF_CHANNEL_ORDER_NB` included.
    const UNNAMED: [c_uint; 6] = [4, 5, 1 << 16, 1 << 31, c_uint::MAX - 1, c_uint::MAX];

    /// Every order the tests compare, named first.
    fn values() -> Vec<ffi::AVChannelOrder> {
        NAMED
            .iter()
            .map(|&(_, c)| c)
            .chain(UNNAMED.map(ffi::AVChannelOrder))
            .collect()
    }

    /// A C channel layout, uninitialised by `av_channel_layout_uninit` on drop.
    struct CLayout(ffi::AVChannelLayout);

    impl CLayout {
        /// The zeroed layout, as `av_channel_layout_uninit` leaves one.
        fn zeroed() -> Self {
            // SAFETY: `AVChannelLayout` is plain C data (integers, a union of
            // an integer and a pointer, and a pointer), for which all-zero is
            // a valid value: the state `av_channel_layout_uninit` leaves.
            Self(unsafe { MaybeUninit::zeroed().assume_init() })
        }

        /// The layout `av_channel_layout_from_string` parses from `s`.
        fn from_string(s: &CStr) -> Self {
            let mut l = Self::zeroed();
            // SAFETY: `l.0` is a writable, zeroed layout; `s` is
            // NUL-terminated.
            let ret = unsafe { ffi::av_channel_layout_from_string(&raw mut l.0, s.as_ptr()) };
            assert_eq!(ret, 0, "{s:?}");
            l
        }

        /// The order C stored, as Rust reads it.
        fn order(&self) -> ChannelOrder {
            ChannelOrder::from_raw(self.0.order)
        }

        /// `av_channel_layout_check`.
        fn check(&self) -> c_int {
            // SAFETY: `self.0` is a live layout; for `CUSTOM` the tests leave
            // the map either NULL or C-allocated with `nb_channels` entries.
            unsafe { ffi::av_channel_layout_check(&raw const self.0) }
        }

        /// A layout of `nb_channels` channels and `mask`, ordered `order`.
        fn with(order: ffi::AVChannelOrder, nb_channels: c_int, mask: u64) -> Self {
            let mut l = Self::zeroed();
            l.0.order = order;
            l.0.nb_channels = nb_channels;
            l.0.u.mask = mask;
            l
        }
    }

    impl Drop for CLayout {
        fn drop(&mut self) {
            // SAFETY: `self.0` is a live layout, and every `CUSTOM` one here
            // holds a map C allocated (or NULL), which uninit frees.
            unsafe { ffi::av_channel_layout_uninit(&raw mut self.0) }
        }
    }

    /// `av_channel_layout_check` for a layout whose order is `order` and whose
    /// mask or map is valid for that order, so only the order decides.
    fn c_accepts(order: ffi::AVChannelOrder) -> bool {
        let l = if order == ffi::AVChannelOrder::AV_CHANNEL_ORDER_CUSTOM {
            let mut l = CLayout::zeroed();
            // SAFETY: `l.0` is a writable, zeroed layout holding no map.
            let ret = unsafe { ffi::av_channel_layout_custom_init(&raw mut l.0, 2) };
            assert_eq!(ret, 0);
            // Give every channel a known id: `AV_CHAN_NONE` is rejected.
            for i in 0..2 {
                // SAFETY: `custom_init` allocated a map of 2 entries.
                unsafe { (*l.0.u.map.add(i)).id = ffi::AVChannel(i as c_int) };
            }
            l
        } else if order == ffi::AVChannelOrder::AV_CHANNEL_ORDER_AMBISONIC {
            CLayout::with(order, 4, 0)
        } else {
            CLayout::with(order, 2, 0b11)
        };
        l.check() != 0
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn constants_are_the_c_enumerators() {
        for (order, c) in NAMED {
            assert_eq!(order.into_raw(), c);
            assert_eq!(ChannelOrder::from_raw(c), order);
            assert!(c_accepts(order.into_raw()), "{order:?}");
        }
        // Unnamed values fall to `av_channel_layout_check`'s default arm.
        for raw in UNNAMED {
            let order = ChannelOrder::from_raw(ffi::AVChannelOrder(raw));
            assert!(NAMED.iter().all(|&(n, _)| n != order));
            assert!(!c_accepts(order.into_raw()), "raw {raw}");
            assert_eq!(format!("{order:?}"), format!("ChannelOrder({raw})"));
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn debug_names_exactly_the_orders_c_accepts() {
        for c in values() {
            let order = ChannelOrder::from(c);
            let named = !format!("{order:?}").starts_with("ChannelOrder(");
            assert_eq!(named, c_accepts(c), "{c:?}");
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn orders_c_constructors_write_read_as_the_constants() {
        for (s, want) in [
            (c"2C", ChannelOrder::UNSPEC),
            (c"5 channels", ChannelOrder::UNSPEC),
            (c"stereo", ChannelOrder::NATIVE),
            (c"FL+FR+LFE", ChannelOrder::NATIVE),
            (c"FR+FL", ChannelOrder::CUSTOM),
            (c"FL@left+FR", ChannelOrder::CUSTOM),
            (c"ambisonic 1", ChannelOrder::AMBISONIC),
            (c"ambisonic 2+stereo", ChannelOrder::AMBISONIC),
            (c"ambisonic 1+FR@x", ChannelOrder::CUSTOM),
        ] {
            if skipped_under_bsan(s) {
                continue;
            }
            let l = CLayout::from_string(s);
            assert_eq!(l.order(), want, "{s:?}");
            assert_eq!(l.order().into_raw(), l.0.order);
            assert_eq!(l.check(), 1, "{s:?}");
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn writes_through_each_side_agree_after_every_step() {
        // One C layout and one Rust value, stepped together: C's constructors
        // and `av_channel_layout_uninit` write the order, and Rust writes it
        // back into the C struct through `into_raw`.
        let mut c = CLayout::zeroed();
        let mut rust = ChannelOrder::default();
        assert_eq!(c.order(), rust);

        // SAFETY: `c.0` is a writable layout holding no map.
        let ret = unsafe { ffi::av_channel_layout_from_mask(&raw mut c.0, 0b111) };
        assert_eq!(ret, 0);
        rust = ChannelOrder::NATIVE;
        assert_eq!(c.order(), rust);
        assert_eq!(c.check(), 1);

        // Rust rewrites the order of the C layout: the mask now marks the
        // non-diegetic channels of a first-order ambisonic layout.
        c.0.order = ChannelOrder::AMBISONIC.into();
        c.0.nb_channels = 4 + 3;
        rust = ChannelOrder::AMBISONIC;
        assert_eq!(c.order(), rust);
        assert_eq!(c.check(), 1);

        // An unnamed order written from Rust is one C rejects.
        let unnamed = ChannelOrder::from_raw(ffi::AVChannelOrder(1 << 20));
        c.0.order = unnamed.into_raw();
        assert_eq!(c.order(), unnamed);
        assert_eq!(c.check(), 0);

        // SAFETY: the order is not `CUSTOM`, so uninit only zeroes the struct.
        unsafe { ffi::av_channel_layout_uninit(&raw mut c.0) };
        rust = ChannelOrder::default();
        assert_eq!(c.order(), rust);
        assert_eq!(c.order(), ChannelOrder::UNSPEC);

        // SAFETY: `c.0` is a writable, zeroed layout holding no map.
        let ret = unsafe { ffi::av_channel_layout_custom_init(&raw mut c.0, 3) };
        assert_eq!(ret, 0);
        rust = ChannelOrder::CUSTOM;
        assert_eq!(c.order(), rust);
        // SAFETY: `custom_init` allocated the map, which uninit frees.
        unsafe { ffi::av_channel_layout_uninit(&raw mut c.0) };
        assert_eq!(c.order(), ChannelOrder::default());
    }

    /// A C block of `ffi::AVChannelOrder`, freed with `av_free` on drop.
    struct COrders(*mut ffi::AVChannelOrder, usize);

    impl COrders {
        fn zeroed(len: usize) -> Self {
            // SAFETY: `av_calloc` takes its sizes by value.
            let ptr = unsafe { ffi::av_calloc(len, size_of::<ffi::AVChannelOrder>()) };
            assert!(!ptr.is_null());
            Self(ptr.cast(), len)
        }

        fn get(&self, i: usize) -> ffi::AVChannelOrder {
            assert!(i < self.1);
            // SAFETY: `i` is in bounds of the live, initialised block.
            unsafe { self.0.add(i).read() }
        }

        fn set(&mut self, i: usize, v: ffi::AVChannelOrder) {
            assert!(i < self.1);
            // SAFETY: `i` is in bounds of the live block, which we own.
            unsafe { self.0.add(i).write(v) }
        }
    }

    impl Drop for COrders {
        fn drop(&mut self) {
            // SAFETY: `self.0` is a live `av_calloc` block we own.
            unsafe { ffi::av_free(self.0.cast::<c_void>()) }
        }
    }

    /// The C values a Rust array holds, read back through its C-typed pointer.
    fn rust_as_c(a: &HeapVec<ChannelOrder>) -> Vec<ffi::AVChannelOrder> {
        let ptr = a.as_ptr().cast::<ffi::AVChannelOrder>();
        (0..a.len())
            // SAFETY: `ChannelOrder` is `#[repr(transparent)]` over
            // `ffi::AVChannelOrder`, and `i` is in bounds of the live,
            // initialised array `a` borrows.
            .map(|i| unsafe { ptr.add(i).read() })
            .collect()
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn arrays_agree_with_c_blocks_after_every_step() {
        let values = values();
        let len = values.len();
        let mut rust = alloc_array_zeroed::<ChannelOrder>(len).unwrap();
        let mut c = COrders::zeroed(len);
        assert!(rust.as_slice().iter().all(|&o| o == ChannelOrder::UNSPEC));
        for (step, &v) in values.iter().enumerate() {
            // Each step writes slot `step` and rotates an earlier one.
            let back = (step * 7) % len;
            let w = values[(step + 3) % len];
            rust.as_mut_slice()[step] = ChannelOrder::from_raw(v);
            rust.as_mut_slice()[back] = w.into();
            c.set(step, v);
            c.set(back, w);
            for i in 0..len {
                assert_eq!(
                    rust.as_slice()[i].into_raw(),
                    c.get(i),
                    "step {step} slot {i}"
                );
                assert_eq!(
                    c_accepts(rust.as_slice()[i].into_raw()),
                    c_accepts(c.get(i))
                );
            }
        }
        // A clone is `av_memdup`'s copy of the same encodings.
        let copy = rust.clone();
        assert_eq!(
            rust_as_c(&copy),
            (0..len).map(|i| c.get(i)).collect::<Vec<_>>()
        );
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn values_c_writes_are_read_back_unchanged() {
        let values = values();
        let mut c = COrders::zeroed(values.len());
        for (i, &v) in values.iter().enumerate() {
            c.set(i, v);
        }
        // The path a field accessor over a C array of `enum AVChannelOrder`
        // takes: adopt the C block and read it as `ChannelOrder`s.
        let (ptr, len) = (c.0, c.1);
        core::mem::forget(c);
        // SAFETY: `ptr` is a live `av_calloc` block of `len` initialised
        // `ffi::AVChannelOrder`, whose ownership `forget` gave up;
        // `ChannelOrder` is layout-compatible with it, and `av_free` releases
        // it.
        let adopted = unsafe { HeapVec::<ChannelOrder>::from_raw_parts(ptr.cast(), len) };
        let adopted = adopted.unwrap();
        for (i, &v) in values.iter().enumerate() {
            assert_eq!(adopted.as_slice()[i], ChannelOrder::from_raw(v));
            assert_eq!(c_accepts(adopted.as_slice()[i].into_raw()), c_accepts(v));
        }
    }
}

/// [`ChannelCustom`] against C's `AVChannelCustom`, as the elements of a
/// custom-order layout's map: the safe getters against what C's parser
/// stores and C's readers (`av_channel_layout_channel_from_index`) return,
/// and twin maps written field by field, one by C-layout writes
/// (`av_strlcpy` for names) and one through the safe setters, compared
/// byte for byte and as C's readers (`av_channel_layout_describe`,
/// `_check`, `_index_from_string`, `_channel_from_string`) observe them after
/// every step, then copied (`av_channel_layout_copy`) and freed
/// (`av_channel_layout_uninit`), a Rust-built map included.
///
/// [`ChannelCustom`]: ffmpeg::libavutil::channel_layout::ChannelCustom
mod channel_custom {
    use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
    use core::ptr::{self, NonNull};

    use ffibox::{CSlice, CSliceMut};
    use ffmpeg::libavutil::channel_layout::{Channel, ChannelCustom, ChannelCustomRef};
    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::mem::alloc_cell_array_zeroed;

    /// A C channel layout, uninitialised by C (its map freed) on drop.
    struct Layout(ffi::AVChannelLayout);

    impl Layout {
        fn zeroed() -> ffi::AVChannelLayout {
            ffi::AVChannelLayout {
                order: ffi::AVChannelOrder::AV_CHANNEL_ORDER_UNSPEC,
                nb_channels: 0,
                u: ffi::AVChannelLayout__bindgen_ty_1 { mask: 0 },
                opaque: ptr::null_mut(),
            }
        }

        /// `av_channel_layout_custom_init`: `n` channels, each `UNKNOWN`
        /// and unnamed.
        fn custom(n: c_int) -> Self {
            let mut l = Self::zeroed();
            // SAFETY: `l` is a zeroed layout C may overwrite.
            assert_eq!(unsafe { ffi::av_channel_layout_custom_init(&mut l, n) }, 0);
            assert_eq!(l.order, ffi::AVChannelOrder::AV_CHANNEL_ORDER_CUSTOM);
            Self(l)
        }

        /// `av_channel_layout_from_string`.
        fn parse(s: &CStr) -> Self {
            let mut l = Self::zeroed();
            // SAFETY: `l` is a zeroed layout C may overwrite; `s` is a C
            // string.
            let r = unsafe { ffi::av_channel_layout_from_string(&mut l, s.as_ptr()) };
            assert_eq!(r, 0, "{s:?}");
            Self(l)
        }

        /// A custom layout over a map Rust allocated (`av_calloc`) and
        /// filled, which C frees.
        fn adopt(map: ffmpeg::libavutil::mem::HeapVec<ChannelCustom>) -> Self {
            let (p, n) = map.into_raw_parts();
            let mut l = Self::zeroed();
            l.order = ffi::AVChannelOrder::AV_CHANNEL_ORDER_CUSTOM;
            l.nb_channels = n as c_int;
            l.u.map = p.cast();
            Self(l)
        }

        fn len(&self) -> usize {
            self.0.nb_channels as usize
        }

        fn custom_map(&self) -> *mut ffi::AVChannelCustom {
            assert_eq!(self.0.order, ffi::AVChannelOrder::AV_CHANNEL_ORDER_CUSTOM);
            // SAFETY: a custom-order layout's union holds `map`.
            unsafe { self.0.u.map }
        }

        /// The map's elements as safe handles: the test's one adoption of
        /// C-owned `AVChannelCustom` storage.
        fn handles(&self) -> CSlice<'_, ChannelCustom> {
            // SAFETY: a custom layout's `map` is a live array of
            // `nb_channels` initialised elements, freed only when `self`
            // drops; `ChannelCustom` is transparent over `AVChannelCustom`.
            unsafe { CSlice::from_raw_parts(NonNull::new(self.custom_map()).unwrap().cast(), self.len()) }
        }

        fn handles_mut(&mut self) -> CSliceMut<'_, ChannelCustom> {
            // SAFETY: as for `handles`, borrowed exclusively through
            // `&mut self`.
            unsafe { CSliceMut::from_raw_parts(NonNull::new(self.custom_map()).unwrap().cast(), self.len()) }
        }

        /// Element `i` as C stores it, by value.
        fn raw(&self, i: usize) -> ffi::AVChannelCustom {
            assert!(i < self.len());
            // SAFETY: `i` is in bounds of the live map.
            unsafe { self.custom_map().add(i).read() }
        }

        /// A C-layout pointer to element `i`, for C-side writes.
        fn raw_mut(&mut self, i: usize) -> *mut ffi::AVChannelCustom {
            assert!(i < self.len());
            // SAFETY: `i` is in bounds of the live map.
            unsafe { self.custom_map().add(i) }
        }

        fn describe(&self) -> String {
            let mut buf = [0 as c_char; 512];
            // SAFETY: `self.0` is a valid layout and `buf` holds 512 bytes.
            let r = unsafe { ffi::av_channel_layout_describe(&self.0, buf.as_mut_ptr(), buf.len()) };
            assert!(r > 0 && (r as usize) < buf.len(), "{r}");
            // SAFETY: C NUL-terminated `buf`.
            unsafe { CStr::from_ptr(buf.as_ptr()) }.to_string_lossy().into_owned()
        }

        fn copy(&self) -> Self {
            let mut l = Self::zeroed();
            // SAFETY: `l` is zeroed, `self.0` valid.
            assert_eq!(unsafe { ffi::av_channel_layout_copy(&mut l, &self.0) }, 0);
            Self(l)
        }
    }

    impl Drop for Layout {
        fn drop(&mut self) {
            // SAFETY: `self.0` is a valid layout whose map, if any, is an
            // `av_malloc`-family block C may free.
            unsafe { ffi::av_channel_layout_uninit(&mut self.0) }
        }
    }

    /// Names C's lookups are asked for: present, absent, prefixed by a
    /// channel, truncated and overlong.
    const PROBES: [&CStr; 15] = [
        c"front",
        c"BAD@front",
        c"@front",
        c"FL@front",
        c"FR@front",
        c"@right",
        c"@fr",
        c"@x",
        c"USR63@x",
        c"@0123456789abcde",
        c"@0123456789abcdef",
        c"@whole",
        c"@",
        c"FL",
        c"LFE",
    ];

    /// Everything C's readers report about `l`.
    #[derive(Debug, PartialEq)]
    struct Observed {
        describe: String,
        check: c_int,
        index: Vec<c_int>,
        channel: Vec<c_int>,
        at: Vec<c_int>,
    }

    fn observe(l: &Layout) -> Observed {
        let index = PROBES
            .iter()
            // SAFETY: `l.0` is a valid layout and `p` a C string.
            .map(|p| unsafe { ffi::av_channel_layout_index_from_string(&l.0, p.as_ptr()) })
            .collect();
        let channel = PROBES
            .iter()
            // SAFETY: as above.
            .map(|p| unsafe { ffi::av_channel_layout_channel_from_string(&l.0, p.as_ptr()) }.0)
            .collect();
        let at = (0..=l.len() as c_uint)
            // SAFETY: `l.0` is a valid layout; C bounds-checks the index.
            .map(|i| unsafe { ffi::av_channel_layout_channel_from_index(&l.0, i) }.0)
            .collect();
        // SAFETY: `l.0` is a valid layout.
        let check = unsafe { ffi::av_channel_layout_check(&l.0) };
        Observed {
            describe: l.describe(),
            check,
            index,
            channel,
            at,
        }
    }

    fn bytes(r: ffi::AVChannelCustom) -> (c_int, [c_char; 16], *mut c_void) {
        (r.id.0, r.name, r.opaque)
    }

    /// The safe getters on every element of `l` against C's own reads.
    fn assert_getters_read_c(l: &Layout, step: &str) {
        for (i, h) in l.handles().iter().enumerate() {
            let raw = l.raw(i);
            // SAFETY: valid layout, index in range.
            let c_id = unsafe { ffi::av_channel_layout_channel_from_index(&l.0, i as c_uint) };
            assert_eq!(h.id(), Channel::from_raw(c_id), "{step} [{i}]");
            assert_eq!(h.id().into_raw(), raw.id, "{step} [{i}]");
            // C writes every name NUL-terminated within the field.
            // SAFETY: the field holds a NUL.
            let c_name = unsafe { CStr::from_ptr(raw.name.as_ptr()) };
            assert_eq!(h.name(), c_name, "{step} [{i}]");
            assert_eq!(h.opaque(), raw.opaque, "{step} [{i}]");
            assert_eq!(bytes(h.get().into_raw()), bytes(raw), "{step} [{i}]");
        }
    }

    /// One write to element `i`, by C and through the safe setters.
    #[derive(Clone, Copy)]
    enum Op<'a> {
        Id(Channel),
        /// `set_name`; C writes only names that fit.
        Name(&'a CStr),
        /// `set_name_truncated`; C writes with `av_strlcpy`.
        NameTruncated(&'a CStr),
        Opaque(*mut c_void),
        Whole(ChannelCustom),
    }

    fn c_write(l: &mut Layout, i: usize, op: Op<'_>) {
        let p = l.raw_mut(i);
        // SAFETY: `p` addresses a live element of `l`'s map, which nothing
        // else reaches meanwhile; `av_strlcpy` writes at most the field's 16
        // bytes, and `name` is a C string.
        unsafe {
            match op {
                Op::Id(ch) => (*p).id = ch.into_raw(),
                Op::Name(name) => {
                    if name.to_bytes().len() < 16 {
                        ffi::av_strlcpy((*p).name.as_mut_ptr(), name.as_ptr(), 16);
                    }
                }
                Op::NameTruncated(name) => {
                    ffi::av_strlcpy((*p).name.as_mut_ptr(), name.as_ptr(), 16);
                }
                Op::Opaque(o) => (*p).opaque = o,
                Op::Whole(v) => *p = v.into_raw(),
            }
        }
    }

    fn rust_write(l: &mut Layout, i: usize, op: Op<'_>) {
        let mut handles = l.handles_mut();
        let mut m = handles.get_mut(i).unwrap();
        match op {
            Op::Id(ch) => m.set_id(ch),
            Op::Name(name) => {
                let fits = name.to_bytes().len() < 16;
                assert_eq!(m.set_name(name).is_ok(), fits);
            }
            Op::NameTruncated(name) => {
                let whole = m.set_name_truncated(name);
                assert_eq!(whole, name.to_bytes().len() < 16);
            }
            Op::Opaque(o) => m.set_opaque(o),
            Op::Whole(v) => m.set(v),
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn getters_read_what_c_parses() {
        let layouts = [
            c"FL@front+FR@right+LFE",
            c"FL+FR@0123456789abcdefghij+USR63@x+AMBI4@amb",
            c"FL@a+FL@a+FL@b",
            c"USR0@\xc3\xa9+BL",
            c"FC@0123456789abcde",
        ];
        for s in layouts {
            let l = Layout::parse(s);
            assert_eq!(l.0.order, ffi::AVChannelOrder::AV_CHANNEL_ORDER_CUSTOM, "{s:?}");
            assert_getters_read_c(&l, &format!("{s:?}"));
            for h in l.handles().iter() {
                assert!(h.opaque().is_null());
                assert!(h.name().as_bytes().len() <= 15);
            }
        }
        // C truncates parsed names to the field, as `set_name_truncated` does.
        let l = Layout::parse(c"FL+FR@0123456789abcdefghij");
        assert_eq!(l.handles().get(1).unwrap().name(), c"0123456789abcde");
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn twin_maps_written_by_c_and_the_setters_read_alike() {
        let mut cookies = [0u8; 4];
        let base = cookies.as_mut_ptr();
        let cookie = |i: usize| -> *mut c_void { base.wrapping_add(i).cast() };
        let mut whole = ChannelCustom::new(Channel::SIDE_LEFT);
        assert_eq!(whole.as_mut().set_name(c"whole"), Ok(()));
        whole.as_mut().set_opaque(cookie(3));
        let steps: Vec<(usize, Op<'_>)> = vec![
            (0, Op::Id(Channel::FRONT_LEFT)),
            (1, Op::Id(Channel::FRONT_RIGHT)),
            (2, Op::Id(Channel::from_raw(ffi::AVChannel(63)))),
            (3, Op::Id(Channel::LOW_FREQUENCY)),
            (0, Op::Name(c"front")),
            (1, Op::Name(c"right")),
            (2, Op::Name(c"0123456789abcdef")),
            (2, Op::NameTruncated(c"0123456789abcdef")),
            (3, Op::Name(c"x")),
            (2, Op::Name(c"0123456789abcde")),
            (0, Op::NameTruncated(c"fr")),
            (1, Op::Name(c"")),
            (0, Op::Name(c"x")),
            (3, Op::NameTruncated(c"0123456789abcdefghijklmnop")),
            (0, Op::Opaque(cookie(0))),
            (2, Op::Opaque(cookie(2))),
            (1, Op::Whole(whole)),
            (0, Op::Opaque(ptr::null_mut())),
            (3, Op::Id(Channel::AMBISONIC_BASE)),
            (3, Op::Whole(ChannelCustom::default())),
            // Unnamed again, in native order: C describes the map as the
            // native layout it equals.
            (0, Op::Name(c"")),
            (1, Op::Whole(ChannelCustom::new(Channel::FRONT_RIGHT))),
            (2, Op::NameTruncated(c"")),
            (2, Op::Id(Channel::FRONT_CENTER)),
            (3, Op::Id(Channel::LOW_FREQUENCY)),
        ];
        let mut c_side = Layout::custom(4);
        let mut rust_side = Layout::custom(4);
        assert_eq!(observe(&c_side), observe(&rust_side));
        for (n, &(i, op)) in steps.iter().enumerate() {
            c_write(&mut c_side, i, op);
            rust_write(&mut rust_side, i, op);
            let step = format!("step {n}");
            for e in 0..4 {
                assert_eq!(bytes(c_side.raw(e)), bytes(rust_side.raw(e)), "{step} [{e}]");
            }
            assert_eq!(observe(&c_side), observe(&rust_side), "{step}");
            assert_getters_read_c(&c_side, &step);
            assert_getters_read_c(&rust_side, &step);
            if n == 19 {
                // Not vacuous: C sees the names the setters wrote.
                let seen = observe(&rust_side);
                assert_eq!(seen.describe, "4 channels (FL@x+SL@whole+USR63@0123456789abcde+UNK)");
                assert_eq!(seen.index[2], -22, "`@front` is gone");
                assert_eq!(seen.index[9], 2);
                assert_eq!(seen.index[1], -22, "`BAD@` is no channel");
            }
        }
        assert_eq!(observe(&rust_side).describe, "3.1");

        // Copies: the map is duplicated byte for byte, the user pointers by
        // address.
        let (c_copy, rust_copy) = (c_side.copy(), rust_side.copy());
        assert_ne!(rust_copy.custom_map(), rust_side.custom_map());
        assert_eq!(observe(&c_copy), observe(&rust_copy));
        for (a, b) in rust_side.handles().iter().zip(rust_copy.handles().iter()) {
            assert_eq!(a.get(), b.get());
        }
        assert!(rust_copy.handles().get(1).unwrap().opaque().is_null());
        assert_eq!(rust_copy.handles().get(2).unwrap().opaque(), cookie(2));
        // Dropping a source leaves its copy intact.
        drop(rust_side);
        drop(c_side);
        assert_getters_read_c(&rust_copy, "copy after the source is freed");
        assert_eq!(observe(&c_copy), observe(&rust_copy));
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn a_map_built_in_rust_is_read_and_freed_by_c() {
        let reference = Layout::parse(c"FL@a+FR@bb+LFE@0123456789abcde+USR42");
        for _ in 0..3 {
            let mut map = alloc_cell_array_zeroed::<ChannelCustom>(reference.len()).unwrap();
            {
                let mut handles = map.as_handles_mut();
                for (i, src) in reference.handles().iter().enumerate() {
                    let mut m = handles.get_mut(i).unwrap();
                    m.set_id(src.id());
                    assert_eq!(m.set_name(src.name().as_c_str()), Ok(()));
                }
            }
            let ours = Layout::adopt(map);
            assert_eq!(observe(&ours), observe(&reference));
            // Equal as C reads them, not byte for byte: C's parser reuses one
            // `AVChannelCustom` for every channel and `av_strlcpy`s each name
            // into it, so a parsed name keeps the previous one's bytes after
            // its NUL, which no C reader looks at.
            for (a, b) in ours.handles().iter().zip(reference.handles().iter()) {
                assert_eq!(a.get(), b.get());
            }
            // `ours` drops here: `av_channel_layout_uninit` frees the
            // `av_calloc` block Rust allocated.
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn values_moved_in_and_out_of_c_maps_read_alike() {
        let src = Layout::parse(c"BL@left+BR@right+USR7@seven");
        let values: Vec<ChannelCustom> = src.handles().iter().map(|h| h.get()).collect();
        let mut dst = Layout::custom(3);
        for (i, v) in values.iter().enumerate() {
            // A value read from one C map, set into another, reads as C's.
            let from_raw = ChannelCustom::from_raw(src.raw(i));
            assert_eq!(*v, from_raw);
            dst.handles_mut().get_mut(i).unwrap().set(*v);
        }
        assert_eq!(observe(&dst), observe(&src));
        let read: Vec<ChannelCustomRef<'_>> = dst.handles().iter().collect();
        assert_eq!(read.len(), 3);
        assert_eq!(read[2].name(), c"seven");
    }
}

/// `Channel::write_name`, `name`, `write_description`, `description` and
/// `from_name` against `av_channel_name`, `av_channel_description` and
/// `av_channel_from_string`: every channel id around the named, Ambisonic
/// and user ranges and the `int` extremes, written into every buffer size
/// from empty to roomy on both sides and compared byte for byte, the names
/// fed back, and C's lenient spellings, in repeated sequences.
mod channel_names {
    use core::ffi::{CStr, c_char, c_int};
    use std::ffi::CString;

    use ffmpeg::libavutil::channel_layout::Channel;
    use ffmpeg::libavutil::ffi;

    type Writer = unsafe extern "C" fn(*mut c_char, usize, ffi::AVChannel) -> c_int;
    type RustWriter = fn(Channel, &mut [u8]) -> Result<usize, i32>;

    fn ids() -> Vec<c_int> {
        (-4..70)
            .chain(1000..1030)
            .chain(2040..2060)
            .chain([
                0x7fff,
                0x10000,
                i32::MAX - 1,
                i32::MAX,
                i32::MIN,
                i32::MIN + 1,
            ])
            .collect()
    }

    fn ch(id: c_int) -> Channel {
        Channel::from_raw(ffi::AVChannel(id))
    }

    /// C's writer into a buffer of `size` bytes prefilled with 0xAA: its
    /// return and the whole buffer.
    fn c_write(write: Writer, id: c_int, size: usize) -> (c_int, Vec<u8>) {
        let mut buf = vec![0xAA; size];
        let p = if size == 0 {
            core::ptr::null_mut()
        } else {
            buf.as_mut_ptr().cast()
        };
        // SAFETY: `p` is NULL with size 0, a count-only request, or valid
        // for `size` bytes of writes; C keeps no pointer.
        let ret = unsafe { write(p, size, ffi::AVChannel(id)) };
        (ret, buf)
    }

    /// Rust's writer into the same prefilled buffer.
    fn rust_write(f: RustWriter, id: c_int, size: usize) -> (c_int, Vec<u8>) {
        let mut buf = vec![0xAA; size];
        let ret = match f(ch(id), &mut buf) {
            Ok(n) => c_int::try_from(n).unwrap(),
            Err(e) => e,
        };
        (ret, buf)
    }

    fn c_string(write: Writer, id: c_int) -> CString {
        let (need, _) = c_write(write, id, 0);
        let (_, buf) = c_write(write, id, need as usize);
        CStr::from_bytes_until_nul(&buf).unwrap().to_owned()
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn writers_match_c_for_every_id_and_buffer_size() {
        let writers: [(Writer, RustWriter); 2] = [
            (ffi::av_channel_name, Channel::write_name),
            (ffi::av_channel_description, Channel::write_description),
        ];
        for (c, rust) in writers {
            for id in ids() {
                for size in 0..26 {
                    assert_eq!(
                        rust_write(rust, id, size),
                        c_write(c, id, size),
                        "id {id}, size {size}"
                    );
                }
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn whole_strings_match_c() {
        let mut longest = (0, 0);
        for id in ids() {
            let name = ch(id).name();
            assert_eq!(name, c_string(ffi::av_channel_name, id), "id {id}");
            let desc = ch(id).description();
            assert_eq!(desc, c_string(ffi::av_channel_description, id), "id {id}");
            longest.0 = longest.0.max(name.as_bytes_with_nul().len());
            longest.1 = longest.1.max(desc.as_bytes_with_nul().len());
        }
        // The buffer sizes the docs promise suffice.
        assert!(longest.0 <= 16 && longest.1 <= 24, "{longest:?}");
        assert_eq!(longest, (15, 22));
    }

    fn c_from(name: &CStr) -> c_int {
        // SAFETY: `name` is a live NUL-terminated string, only read.
        unsafe { ffi::av_channel_from_string(name.as_ptr()) }.0
    }

    /// Both lookups of `name`; C's `NONE` (-1) is Rust's `None`.
    fn lookups(name: &CStr) -> (c_int, c_int) {
        (
            Channel::from_name(name).map_or(-1, |c| c.into_raw().0),
            c_from(name),
        )
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn names_map_back_on_both_sides() {
        for id in ids() {
            let name = ch(id).name();
            let (rust, c) = lookups(&name);
            assert_eq!(rust, c, "{name:?}");
            // Every name maps back but NONE's, and a negative user id's,
            // which C refuses.
            if id >= 0 {
                assert_eq!(rust, id, "{name:?}");
            } else {
                assert_eq!(
                    rust,
                    if name.as_bytes() == b"UNK" || name.as_bytes() == b"UNSD" {
                        id
                    } else {
                        -1
                    }
                );
            }
        }
    }

    fn candidates() -> Vec<CString> {
        [
            "",
            "FL",
            "fl",
            "FL ",
            " FL",
            "LFE2",
            "NONE",
            "UNK",
            "UNSD",
            "AMBI",
            "AMBI0",
            "AMBI1023",
            "AMBI1024",
            "AMBI-1",
            "AMBI0x10",
            "AMBI010",
            "AMBI7x",
            "AMBI 5",
            "AMBI4294967296",
            "AMBI99999999999999999999",
            "USR",
            "USR0",
            "USR5",
            "USR-5",
            "USR0x10",
            "USR010",
            "USR5x",
            "USR 5",
            "USR2147483647",
            "USR2147483648",
            "USR4294967297",
            "USR99999999999999999999",
            "USR+7",
            "usr5",
            "AMBIENT",
            "XYZ",
        ]
        .into_iter()
        .map(|s| CString::new(s).unwrap())
        .collect()
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn from_name_matches_c_for_every_candidate() {
        for name in candidates() {
            let (rust, c) = lookups(&name);
            assert_eq!(rust, c, "{name:?}");
        }
        // C's lenient corners, as Rust reports them.
        assert_eq!(Channel::from_name(c"AMBI"), Channel::ambisonic(0));
        assert_eq!(Channel::from_name(c"USR"), Some(Channel::FRONT_LEFT));
        assert_eq!(Channel::from_name(c"NONE"), None);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn repeated_interleaved_calls_match_c() {
        let names = candidates();
        let ids = ids();
        for round in 0..3 {
            for (k, id) in ids.iter().enumerate().rev().step_by(round + 1) {
                let size = k % 20;
                assert_eq!(
                    rust_write(Channel::write_name, *id, size),
                    c_write(ffi::av_channel_name, *id, size),
                    "round {round}, id {id}, size {size}"
                );
                assert_eq!(
                    rust_write(Channel::write_description, *id, size),
                    c_write(ffi::av_channel_description, *id, size),
                );
                let name = &names[k % names.len()];
                assert_eq!(lookups(name).0, lookups(name).1, "round {round}, {name:?}");
            }
        }
    }
}

mod name_appenders {
    use core::ffi::{c_int, c_uint};
    use core::mem::MaybeUninit;
    use core::ptr::{self, addr_of};

    use ffmpeg::libavutil::bprint::{BPrint, BPrintRef};
    use ffmpeg::libavutil::channel_layout::Channel;
    use ffmpeg::libavutil::ffi;

    type CAppender = unsafe extern "C" fn(*mut ffi::AVBPrint, ffi::AVChannel);
    type RustAppender = fn(Channel, ffmpeg::libavutil::bprint::BPrintMut<'_>);

    const APPENDERS: [(CAppender, RustAppender); 2] = [
        (ffi::av_channel_name_bprint, Channel::append_name),
        (
            ffi::av_channel_description_bprint,
            Channel::append_description,
        ),
    ];

    fn ids() -> Vec<c_int> {
        (-4..70)
            .chain(1020..1030)
            .chain(2040..2052)
            .chain([0x10000, i32::MAX, i32::MIN])
            .collect()
    }

    /// A C-side print buffer on its own heap storage, which never moves.
    struct CBuf(*mut ffi::AVBPrint, bool);

    impl CBuf {
        fn new(buffer: Option<&mut [u8]>) -> Self {
            let b: *mut ffi::AVBPrint =
                Box::into_raw(Box::new(MaybeUninit::<ffi::AVBPrint>::uninit())).cast();
            let owned = buffer.is_none();
            // SAFETY: `b` is writable storage for one `AVBPrint`; a caller
            // buffer is writable for its length and outlives every use of
            // `b` in these tests.
            unsafe {
                match buffer {
                    Some(data) => ffi::av_bprint_init_for_buffer(
                        b,
                        data.as_mut_ptr().cast(),
                        data.len() as c_uint,
                    ),
                    None => ffi::av_bprint_init(b, 0, c_uint::MAX),
                }
            };
            Self(b, owned)
        }

        /// `(len, stored bytes, complete)`.
        fn state(&self) -> (u32, Vec<u8>, bool) {
            // SAFETY: an initialised buffer, whose string holds
            // `min(len, size - 1)` bytes before its NUL.
            unsafe {
                let len = addr_of!((*self.0).len).read();
                let size = addr_of!((*self.0).size).read();
                let stored = len.min(size.saturating_sub(1)) as usize;
                let s = addr_of!((*self.0).str_).read().cast::<u8>();
                let bytes = core::slice::from_raw_parts(s, stored).to_vec();
                (len, bytes, ffi::av_bprint_is_complete(self.0) != 0)
            }
        }
    }

    impl Drop for CBuf {
        fn drop(&mut self) {
            if self.1 {
                // SAFETY: an owned, initialised buffer, whose string C frees.
                unsafe { ffi::av_bprint_finalize(self.0, ptr::null_mut()) };
            }
            // SAFETY: the storage `Box` that `new` leaked.
            drop(unsafe { Box::from_raw(self.0.cast::<MaybeUninit<ffi::AVBPrint>>()) });
        }
    }

    fn rust_state(b: BPrintRef<'_>) -> (u32, Vec<u8>, bool) {
        (b.len(), b.as_bytes().to_vec(), b.is_complete())
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn appending_sequences_match_c_after_every_step() {
        for (i, chunk) in ids().chunks(16).enumerate() {
            let mut rust = BPrint::new(0, BPrint::SIZE_UNLIMITED).unwrap();
            let c = CBuf::new(None);
            for (j, &id) in chunk.iter().enumerate() {
                let (c_append, rust_append) = APPENDERS[(i + j) % 2];
                rust_append(Channel::from_raw(ffi::AVChannel(id)), rust.as_mut());
                // SAFETY: an initialised buffer; any channel value.
                unsafe { c_append(c.0, ffi::AVChannel(id)) };
                assert_eq!(rust_state(rust.as_ref()), c.state(), "{id}");
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn appending_to_small_caller_buffers_truncates_like_c() {
        for size in [1, 2, 5, 16] {
            for &id in &[0, 3, 1024, 2047, -1, i32::MIN, 100] {
                let (mut rust_data, mut c_data) = (vec![0xAAu8; size], vec![0xAAu8; size]);
                {
                    let mut rust = BPrint::for_buffer(&mut rust_data).unwrap();
                    let c = CBuf::new(Some(&mut c_data));
                    for (c_append, rust_append) in APPENDERS {
                        rust_append(Channel::from_raw(ffi::AVChannel(id)), rust.as_mut());
                        // SAFETY: an initialised buffer on `c_data`.
                        unsafe { c_append(c.0, ffi::AVChannel(id)) };
                        assert_eq!(rust_state(rust.as_ref()), c.state(), "{id} {size}");
                    }
                }
                assert_eq!(rust_data, c_data, "{id} {size}");
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn appended_names_are_the_channel_names() {
        for id in ids() {
            let ch = Channel::from_raw(ffi::AVChannel(id));
            let mut b = BPrint::new(0, BPrint::SIZE_UNLIMITED).unwrap();
            ch.append_name(b.as_mut());
            assert_eq!(b.as_ref().as_bytes(), ch.name().as_bytes(), "{id}");
            b.as_mut().clear();
            ch.append_description(b.as_mut());
            assert_eq!(b.as_ref().as_bytes(), ch.description().as_bytes(), "{id}");
        }
    }
}

/// `ChannelLayout` against C's own handling of `AVChannelLayout`: each C
/// initialiser through the Rust constructors against the same call on a C
/// layout, the getters against the raw fields, copies against
/// `av_channel_layout_copy` over every kind of destination, and sequences of
/// every setter against the C writes they stand for (the documented `{0}`
/// plus field writes, `av_channel_layout_from_mask`,
/// `av_channel_layout_from_string`, `av_channel_layout_retype`), compared
/// after every step through C's own readers on both layouts.
mod layout {
    use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
    use core::mem::size_of;
    use core::ptr;
    use std::ffi::CString;

    use ffmpeg::libavutil::channel_layout::{
        Channel, ChannelCustom, ChannelLayout, ChannelLayoutRef, ChannelLayoutVal, ChannelOrder,
        LayoutDetails,
    };
    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::mem::{HeapVec, alloc_cell_array_zeroed};

    use super::parser_inputs::{CHANNEL_BITS, LAYOUT_MASKS, STANDARD_LAYOUTS, allowed, under_bsan};

    const EINVAL: c_int = -22;

    /// A layout C owns, uninitialised by C on drop.
    struct CLayout(ffi::AVChannelLayout);

    impl CLayout {
        fn zeroed() -> Self {
            Self(ffi::AVChannelLayout {
                order: ffi::AVChannelOrder::AV_CHANNEL_ORDER_UNSPEC,
                nb_channels: 0,
                u: ffi::AVChannelLayout__bindgen_ty_1 { mask: 0 },
                opaque: ptr::null_mut(),
            })
        }

        /// C's `av_channel_layout_uninit` keeping the user pointer, as
        /// `av_channel_layout_retype` does before it rebuilds a layout.
        fn uninit_keeping_opaque(&mut self) -> *mut c_void {
            let opaque = self.0.opaque;
            // SAFETY: `self.0` is a valid layout C may uninitialise.
            unsafe { ffi::av_channel_layout_uninit(&mut self.0) };
            opaque
        }
    }

    impl Drop for CLayout {
        fn drop(&mut self) {
            // SAFETY: `self.0` is a valid layout whose map, if any, C owns.
            unsafe { ffi::av_channel_layout_uninit(&mut self.0) }
        }
    }

    /// The C layout a Rust owner holds, for C's readers.
    fn rust_ptr(l: &ChannelLayoutVal) -> *const ffi::AVChannelLayout {
        const { assert!(size_of::<ChannelLayoutVal>() == size_of::<ffi::AVChannelLayout>()) };
        // `CVal` is `#[repr(C)]` with the value first and a zero-sized
        // policy, and `ChannelLayout` is transparent over the C struct.
        ptr::from_ref(l).cast()
    }

    /// One map element as C observes it: id, name up to its NUL, user
    /// pointer.
    type Elem = (c_int, Vec<u8>, usize);

    fn c_elem(e: &ffi::AVChannelCustom) -> Elem {
        let bytes: Vec<u8> = e
            .name
            .iter()
            .map(|&c| c as u8)
            .take_while(|&b| b != 0)
            .collect();
        (e.id.0, bytes, e.opaque as usize)
    }

    /// The raw fields of a layout: order, count, the mask (outside custom
    /// order), the map's elements (in it), the user pointer.
    #[derive(Debug, PartialEq)]
    struct Fields {
        order: c_uint,
        nb_channels: c_int,
        mask: Option<u64>,
        map: Option<Vec<Elem>>,
        opaque: usize,
    }

    fn c_fields(p: *const ffi::AVChannelLayout) -> Fields {
        // SAFETY: `p` addresses a live, valid layout.
        let l = unsafe { p.read() };
        let custom = l.order == ffi::AVChannelOrder::AV_CHANNEL_ORDER_CUSTOM;
        let unspec = l.order == ffi::AVChannelOrder::AV_CHANNEL_ORDER_UNSPEC;
        let map = custom.then(|| {
            // SAFETY: a custom layout's `u` holds the map.
            let map = unsafe { l.u.map };
            (0..l.nb_channels.max(0) as usize)
                // SAFETY: a valid custom layout's map holds `nb_channels`
                // elements.
                .map(|i| c_elem(unsafe { &*map.add(i) }))
                .collect()
        });
        // SAFETY: outside custom order `u` holds the mask; in custom order
        // its 8 bytes are the map pointer's, read as plain bits and unused.
        let mask = unsafe { l.u.mask };
        Fields {
            order: l.order.0,
            nb_channels: l.nb_channels,
            mask: (!custom && !unspec).then_some(mask),
            map,
            opaque: l.opaque as usize,
        }
    }

    /// The same fields read through the safe getters.
    fn rust_fields(l: ChannelLayoutRef<'_>) -> Fields {
        let (mask, map) = match l.details() {
            LayoutDetails::Unspecified => (None, None),
            LayoutDetails::Native(m) | LayoutDetails::Ambisonic(m) => (Some(m), None),
            LayoutDetails::Custom(map) => {
                let elems = map
                    .iter()
                    .map(|c| {
                        (
                            c.id().into_raw().0,
                            c.name().as_bytes().to_vec(),
                            c.opaque() as usize,
                        )
                    })
                    .collect();
                (None, Some(elems))
            }
            LayoutDetails::Unknown(order) => panic!("{order:?}"),
        };
        Fields {
            order: l.order().into_raw().0,
            nb_channels: l.channel_count(),
            mask,
            map,
            opaque: l.opaque() as usize,
        }
    }

    /// Everything C's readers report about a layout, beside its fields.
    #[derive(Debug, PartialEq)]
    struct Observed {
        fields: Fields,
        describe: Result<String, c_int>,
        check: c_int,
        at: Vec<c_int>,
        index: Vec<c_int>,
        ambisonic_order: c_int,
    }

    fn observe(p: *const ffi::AVChannelLayout) -> Observed {
        // SAFETY: `p` addresses a live, valid layout every call below only
        // reads; NULL with size 0 asks `describe` for the length only, and
        // `buf` holds the `need` bytes it asks for.
        unsafe {
            let need = ffi::av_channel_layout_describe(p, ptr::null_mut(), 0);
            let describe = if need < 0 {
                Err(need)
            } else {
                let mut buf = vec![0 as c_char; need as usize];
                assert_eq!(
                    ffi::av_channel_layout_describe(p, buf.as_mut_ptr(), buf.len()),
                    need
                );
                Ok(CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned())
            };
            let n = (*p).nb_channels.clamp(0, 70) as c_uint;
            let at = (0..=n)
                .map(|i| ffi::av_channel_layout_channel_from_index(p, i).0)
                .collect();
            let index = [0, 1, 3, 40, 63, 64, 0x400, 0x401, -1]
                .map(|ch| ffi::av_channel_layout_index_from_channel(p, ffi::AVChannel(ch)))
                .into();
            Observed {
                fields: c_fields(p),
                describe,
                check: ffi::av_channel_layout_check(p),
                at,
                index,
                ambisonic_order: ffi::av_channel_layout_ambisonic_order(p),
            }
        }
    }

    /// Both twins agree as C reads them, and the getters read the Rust one's
    /// fields.
    fn assert_twins(r: &ChannelLayoutVal, c: &CLayout, step: &str) {
        let (ro, co) = (observe(rust_ptr(r)), observe(&c.0));
        assert_eq!(ro, co, "{step}");
        assert_eq!(rust_fields(r.as_ref()), ro.fields, "{step}");
        // SAFETY: both are live, valid layouts C only reads.
        let cmp = unsafe { ffi::av_channel_layout_compare(rust_ptr(r), &c.0) };
        if ro.check == 1 {
            assert_eq!(cmp, 0, "{step}");
        }
    }

    /// Descriptions both sides parse (or refuse) alike.
    const DESCRIPTIONS: [&CStr; 39] = [
        c"mono",
        c"stereo",
        c"5.1(side)",
        c"7.1.4",
        c"22.2",
        c"downmix",
        c"FL+FR+LFE",
        c"FR+FL",
        c"FL@front+FR@0123456789abcdefghij+USR63@x+AMBI4@amb",
        c"FL@a+FL@a+FL@b",
        c"USR0@\xc3\xa9+BL",
        c"0x3",
        c"0xffffffffffffffff",
        c"-1",
        c"4c",
        c"9c",
        c"4C",
        c"4 channels",
        c"2 channels (FL+FR)",
        c"2 channels (FL+FR)x",
        c"3 channels (FL+FR)",
        c"ambisonic 0",
        c"ambisonic 2+stereo",
        c"ambisonic 1+FL@x+FR",
        c"ambisonic 1+AMBI0",
        c"ambisonic 46339",
        c"ambisonic 46340",
        // The extras at and one below the `int` bound: C refuses the first,
        // and runs out of memory mapping the second.
        c"ambisonic 46339+88047C",
        c"ambisonic 46339+88046C",
        c"ambisonic 1+3C",
        c"FL@",
        c"",
        c"FL+NOPE",
        // Orders C narrows from `long` to `int` modulo 2^32 (1 and 2), and
        // the largest it refuses by its own range check; `INT_MAX` itself,
        // which C overflows on, is the wrapper's refusal (`ub_safe`).
        c"ambisonic 4294967297",
        c"ambisonic 0x100000002+FL",
        c"ambisonic 2147483646",
        // A byte other than `+` after the order, a mask `strtoull` cannot
        // hold (`errno` set), and a channel list missing its closing
        // parenthesis.
        c"ambisonic 1x",
        c"18446744073709551616",
        c"2 channels (FL+FR",
    ];

    fn c_parse(s: &CStr) -> (c_int, CLayout) {
        let mut c = CLayout::zeroed();
        // SAFETY: a zeroed layout C may initialise; `s` is a C string.
        let ret = unsafe { ffi::av_channel_layout_from_string(&mut c.0, s.as_ptr()) };
        if ret < 0 {
            // C leaves the layout uninitialised: start the twin afresh.
            drop(c);
            c = CLayout::zeroed();
        }
        (ret, c)
    }

    /// What `c_parse("ambisonic <order>[+<mask>]")` returns, written as C's
    /// parser writes it (`channel_layout.c:340-368`, the native-extra path)
    /// without calling it: the ambisonic forms are parsed through glibc end
    /// pointers, which BSan cannot be given (`parser_inputs`), so under BSan
    /// the C twin of an ambisonic step is built by hand; the parse itself is
    /// compared under the other instruments.
    fn c_ambisonic(order: u32, mask: u64) -> (c_int, CLayout) {
        let mut c = CLayout::zeroed();
        let n = i64::from(order) + 1;
        let extra = i64::from(mask.count_ones());
        if n > i64::from(c_int::MAX) / n || extra >= i64::from(c_int::MAX) - n * n {
            return (EINVAL, c);
        }
        c.0.order = ffi::AVChannelOrder::AV_CHANNEL_ORDER_AMBISONIC;
        c.0.nb_channels = c_int::try_from(n * n + extra).unwrap();
        c.0.u.mask = mask;
        (0, c)
    }

    fn result<T>(r: &Result<T, c_int>) -> c_int {
        r.as_ref().map_or_else(|&e| e, |_| 0)
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn initialisers_match_c_for_every_input() {
        for mask in [0, 1, 0x3, 0x60f, 1 << 63, u64::MAX] {
            let r = ChannelLayout::from_mask(mask);
            let mut c = CLayout::zeroed();
            // SAFETY: a zeroed layout C may initialise.
            let ret = unsafe { ffi::av_channel_layout_from_mask(&mut c.0, mask) };
            assert_eq!(result(&r), ret, "{mask:#x}");
            if let Ok(r) = r {
                assert_twins(&r, &c, &format!("mask {mask:#x}"));
            }
        }
        for n in [i32::MIN, -1, 0, 1, 2, 3, 64, 300, i32::MAX] {
            let r = ChannelLayout::custom(n);
            let mut c = CLayout::zeroed();
            // SAFETY: a zeroed layout C may initialise.
            let ret = unsafe { ffi::av_channel_layout_custom_init(&mut c.0, n) };
            assert_eq!(result(&r), ret, "custom {n}");
            if let Ok(r) = r {
                assert_twins(&r, &c, &format!("custom {n}"));
            }
        }
        for n in (-5..=80).chain([i32::MIN, i32::MAX]) {
            let r = ChannelLayout::default_for(n);
            let mut c = CLayout::zeroed();
            // SAFETY: a zeroed layout C may initialise.
            unsafe { ffi::av_channel_layout_default(&mut c.0, n) };
            assert_twins(&r, &c, &format!("default {n}"));
        }
        let mut parsed = 0;
        for s in allowed(&DESCRIPTIONS) {
            let r = ChannelLayout::parse(s);
            let (ret, c) = c_parse(s);
            assert_eq!(result(&r), ret, "{s:?}");
            if let Ok(r) = r {
                assert_twins(&r, &c, &format!("{s:?}"));
                parsed += 1;
            }
        }
        assert!(parsed >= if under_bsan() { 10 } else { 20 }, "{parsed}");
        let zero = ChannelLayout::unspecified(0);
        assert_twins(&zero, &CLayout::zeroed(), "zeroed");
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn copies_match_c_copy_over_every_destination() {
        let sources: Vec<&CStr> = allowed(&DESCRIPTIONS)
            .filter(|s| c_parse(s).0 == 0)
            .collect();
        for &src in &sources {
            let (rs, (_, cs)) = (ChannelLayout::parse(src).unwrap(), c_parse(src));
            // Into a fresh layout.
            let rc = rs.as_ref().try_clone().unwrap();
            let mut cc = CLayout::zeroed();
            // SAFETY: both are valid layouts; the destination is zeroed.
            assert_eq!(unsafe { ffi::av_channel_layout_copy(&mut cc.0, &cs.0) }, 0);
            assert_twins(&rc, &cc, &format!("clone of {src:?}"));
            // Over a destination of every kind, which C uninitialises first.
            for dst in allowed(&[c"FL@a+FR@b+LFE", c"stereo", c"ambisonic 1", c"3C"]) {
                let (mut rd, (_, mut cd)) = (ChannelLayout::parse(dst).unwrap(), c_parse(dst));
                assert_eq!(rd.as_mut().copy_from(rs.as_ref()), Ok(()));
                // SAFETY: both are valid, distinct layouts.
                assert_eq!(unsafe { ffi::av_channel_layout_copy(&mut cd.0, &cs.0) }, 0);
                assert_twins(&rd, &cd, &format!("{src:?} over {dst:?}"));
            }
        }
    }

    /// One state change, applied to the Rust layout through the safe API
    /// and to the C layout through the C writes it stands for.
    #[derive(Clone, Copy, Debug)]
    enum Op {
        Opaque(usize),
        Unspecified(c_int),
        Native(u64),
        Ambisonic(u32, u64),
        CustomMap(&'static [(c_int, &'static CStr)]),
        TakeCustomMap,
        Edit(usize, c_int, &'static CStr),
        Parse(&'static CStr),
        Take,
        Clear,
        CopyFromOther,
    }

    /// `C` writes `op`; returns what the step reports, to compare.
    fn c_apply(c: &mut CLayout, other: &CLayout, op: Op) -> Option<Vec<Elem>> {
        match op {
            Op::Opaque(p) => c.0.opaque = ptr::without_provenance_mut(p),
            Op::Unspecified(n) => {
                let opaque = c.uninit_keeping_opaque();
                c.0.order = ffi::AVChannelOrder::AV_CHANNEL_ORDER_UNSPEC;
                c.0.nb_channels = n;
                c.0.opaque = opaque;
            }
            Op::Native(mask) => {
                let opaque = c.uninit_keeping_opaque();
                // SAFETY: an uninitialised layout C may initialise.
                let ret = unsafe { ffi::av_channel_layout_from_mask(&mut c.0, mask) };
                assert_eq!(ret, 0);
                c.0.opaque = opaque;
            }
            Op::Ambisonic(order, mask) => {
                let s = if mask == 0 {
                    format!("ambisonic {order}")
                } else {
                    format!("ambisonic {order}+{mask:#x}")
                };
                let (ret, parsed) = if under_bsan() {
                    c_ambisonic(order, mask)
                } else {
                    c_parse(&CString::new(s).unwrap())
                };
                if ret == 0 {
                    let opaque = c.uninit_keeping_opaque();
                    // Move the parsed layout in.
                    c.0 = parsed.0;
                    core::mem::forget(parsed);
                    c.0.opaque = opaque;
                }
                return Some(vec![(ret, vec![], 0)]);
            }
            Op::CustomMap(chans) => {
                let opaque = c.uninit_keeping_opaque();
                let n = chans.len() as c_int;
                // SAFETY: an uninitialised layout C may initialise.
                let ret = unsafe { ffi::av_channel_layout_custom_init(&mut c.0, n) };
                assert_eq!(ret, 0);
                for (i, &(id, name)) in chans.iter().enumerate() {
                    // SAFETY: `i` is in bounds of the map C allocated;
                    // `av_strlcpy` writes at most the field's 16 bytes.
                    unsafe {
                        let e = c.0.u.map.add(i);
                        (*e).id = ffi::AVChannel(id);
                        ffi::av_strlcpy((*e).name.as_mut_ptr(), name.as_ptr(), 16);
                    }
                }
                c.0.opaque = opaque;
            }
            Op::TakeCustomMap => {
                if c.0.order != ffi::AVChannelOrder::AV_CHANNEL_ORDER_CUSTOM {
                    return Some(vec![]);
                }
                let taken = c_fields(&c.0).map;
                // SAFETY: a valid layout; C keeps the count and the user
                // pointer and frees the map, lossily when it named channels.
                let ret = unsafe {
                    ffi::av_channel_layout_retype(
                        &mut c.0,
                        ffi::AVChannelOrder::AV_CHANNEL_ORDER_UNSPEC,
                        0,
                    )
                };
                assert!(ret >= 0, "{ret}");
                return taken;
            }
            Op::Edit(i, id, name) => {
                if c.0.order == ffi::AVChannelOrder::AV_CHANNEL_ORDER_CUSTOM {
                    let i = i % c.0.nb_channels as usize;
                    // SAFETY: `i` is in bounds of the custom map.
                    unsafe {
                        let e = c.0.u.map.add(i);
                        (*e).id = ffi::AVChannel(id);
                        ffi::av_strlcpy((*e).name.as_mut_ptr(), name.as_ptr(), 16);
                    }
                }
            }
            Op::Parse(s) => {
                let (ret, parsed) = c_parse(s);
                assert_eq!(ret, 0);
                *c = parsed;
            }
            Op::Take => {
                let old = core::mem::replace(c, CLayout::zeroed());
                return c_fields(&old.0)
                    .map
                    .or(Some(vec![(old.0.nb_channels, vec![], 0)]));
            }
            Op::Clear => {
                // SAFETY: a valid layout.
                unsafe { ffi::av_channel_layout_uninit(&mut c.0) }
            }
            Op::CopyFromOther => {
                // SAFETY: valid, distinct layouts.
                let ret = unsafe { ffi::av_channel_layout_copy(&mut c.0, &other.0) };
                assert_eq!(ret, 0);
            }
        }
        None
    }

    /// The safe API applies `op`.
    fn rust_apply(r: &mut ChannelLayoutVal, other: &ChannelLayoutVal, op: Op) -> Option<Vec<Elem>> {
        let elems = |map: &HeapVec<ChannelCustom>| -> Vec<Elem> {
            map.as_handles()
                .iter()
                .map(|c| {
                    (
                        c.id().into_raw().0,
                        c.name().as_bytes().to_vec(),
                        c.opaque() as usize,
                    )
                })
                .collect()
        };
        match op {
            Op::Opaque(p) => r.as_mut().set_opaque(ptr::without_provenance_mut(p)),
            Op::Unspecified(n) => r.as_mut().set_unspecified(n),
            Op::Native(mask) => r.as_mut().set_native(mask),
            Op::Ambisonic(order, mask) => {
                let ret = r
                    .as_mut()
                    .set_ambisonic(order, mask)
                    .map_or_else(|e| e, |()| 0);
                return Some(vec![(ret, vec![], 0)]);
            }
            Op::CustomMap(chans) => {
                let mut map = alloc_cell_array_zeroed::<ChannelCustom>(chans.len()).unwrap();
                for (mut m, &(id, name)) in map.as_handles_mut().iter_mut().zip(chans) {
                    m.set_id(Channel::from_raw(ffi::AVChannel(id)));
                    m.set_name_truncated(name);
                }
                r.as_mut().set_custom_map(map).unwrap();
            }
            Op::TakeCustomMap => {
                return Some(
                    r.as_mut()
                        .take_custom_map()
                        .map_or_else(Vec::new, |m| elems(&m)),
                );
            }
            Op::Edit(i, id, name) => {
                if let Some(mut map) = r.as_mut().custom_map_mut() {
                    let i = i % map.len();
                    let mut m = map.get_mut(i).unwrap();
                    m.set_id(Channel::from_raw(ffi::AVChannel(id)));
                    m.set_name_truncated(name);
                }
            }
            Op::Parse(s) => r.as_mut().set(ChannelLayout::parse(s).unwrap()),
            Op::Take => {
                let old = r.as_mut().take();
                return match old.as_ref().details() {
                    LayoutDetails::Custom(_) => rust_fields(old.as_ref()).map,
                    _ => Some(vec![(old.as_ref().channel_count(), vec![], 0)]),
                };
            }
            Op::Clear => r.as_mut().clear(),
            Op::CopyFromOther => r.as_mut().copy_from(other.as_ref()).unwrap(),
        }
        None
    }

    const MAP_A: &[(c_int, &CStr)] = &[(0, c"front"), (1, c"right"), (3, c"")];
    const MAP_B: &[(c_int, &CStr)] = &[(63, c"0123456789abcdefghij"), (0x400, c"amb")];
    const MAP_C: &[(c_int, &CStr)] = &[(0, c""), (1, c""), (2, c""), (3, c"")];

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn setter_sequences_match_c_writes_after_every_step() {
        use Op::*;
        let steps = [
            Opaque(0x10),
            Native(0x3),
            CustomMap(MAP_A),
            Edit(1, 5, c"bl"),
            Edit(4, 0x401, c"0123456789abcdefgh"),
            TakeCustomMap,
            Unspecified(7),
            TakeCustomMap,
            Ambisonic(2, 0x3),
            Ambisonic(46339, 0),
            Ambisonic(46340, 0x1),
            Ambisonic(u32::MAX, 0),
            CustomMap(MAP_B),
            CopyFromOther,
            Opaque(0x20),
            CustomMap(MAP_C),
            TakeCustomMap,
            Parse(c"FL@x+FR@y"),
            Take,
            Native(u64::MAX),
            // The same custom map as `ambisonic 1+FL@x+FR`, which C parses
            // through glibc end pointers BSan cannot be given.
            Parse(if under_bsan() {
                c"AMBI0+AMBI1+AMBI2+AMBI3+FL@x+FR"
            } else {
                c"ambisonic 1+FL@x+FR"
            }),
            Edit(0, 0x401, c"z"),
            Take,
            CustomMap(MAP_B),
            Unspecified(-3),
            Clear,
            CustomMap(MAP_A),
            Clear,
        ];
        let mut r = ChannelLayout::unspecified(0);
        let mut c = CLayout::zeroed();
        let r_other = ChannelLayout::parse(c"FL@o+LFE@p").unwrap();
        let (_, c_other) = c_parse(c"FL@o+LFE@p");
        assert_twins(&r, &c, "start");
        for (n, op) in steps.into_iter().enumerate() {
            let step = format!("step {n} {op:?}");
            let rr = rust_apply(&mut r, &r_other, op);
            let cr = c_apply(&mut c, &c_other, op);
            assert_eq!(rr, cr, "{step}");
            assert_twins(&r, &c, &step);
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn maps_built_in_rust_read_like_c_parsed_ones() {
        for s in [
            c"FL@front+FR@right+LFE@lfe",
            c"USR63@x+AMBI4@amb+BL@b",
            c"FC@0123456789abcde",
        ] {
            let (_, reference) = c_parse(s);
            let Some(elems) = c_fields(&reference.0).map else {
                panic!("{s:?}")
            };
            let mut map = alloc_cell_array_zeroed::<ChannelCustom>(elems.len()).unwrap();
            for (mut m, (id, name, _)) in map.as_handles_mut().iter_mut().zip(&elems) {
                m.set_id(Channel::from_raw(ffi::AVChannel(*id)));
                assert_eq!(m.set_name(&CString::new(name.clone()).unwrap()), Ok(()));
            }
            let ours = ChannelLayout::from_custom_map(map).unwrap();
            assert_twins(&ours, &reference, &format!("{s:?}"));
            assert_eq!(ours.as_ref().order(), ChannelOrder::CUSTOM);
            // `ours` drops here: `av_channel_layout_uninit` frees the block
            // Rust allocated.
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn native_and_ambisonic_setters_match_cs_initialisers() {
        for mask in [1, 0x3, 0x60f, 1 << 63, u64::MAX] {
            let mut r = ChannelLayout::parse(c"FL@a+FR").unwrap();
            let mut c = c_parse(c"FL@a+FR").1;
            rust_apply(&mut r, &ChannelLayout::unspecified(0), Op::Native(mask));
            c_apply(&mut c, &CLayout::zeroed(), Op::Native(mask));
            assert_twins(&r, &c, &format!("native {mask:#x}"));
        }
        // An empty mask C refuses; the setter writes it as C users may.
        let mut r = ChannelLayout::from_mask(0x3).unwrap();
        r.as_mut().set_native(0);
        let mut c = CLayout::zeroed();
        c.0.order = ffi::AVChannelOrder::AV_CHANNEL_ORDER_NATIVE;
        assert_twins(&r, &c, "empty native");
        for order in [
            0,
            1,
            2,
            5,
            15,
            46338,
            46339,
            46340,
            46341,
            1 << 31,
            u32::MAX,
        ] {
            for mask in [0, 0x3, 0x60f, u64::MAX] {
                let mut r = ChannelLayout::unspecified(0);
                let mut c = CLayout::zeroed();
                let step = format!("ambisonic {order}+{mask:#x}");
                let other = ChannelLayout::unspecified(0);
                assert_eq!(
                    rust_apply(&mut r, &other, Op::Ambisonic(order, mask)),
                    c_apply(&mut c, &CLayout::zeroed(), Op::Ambisonic(order, mask)),
                    "{step}"
                );
                assert_twins(&r, &c, &step);
            }
        }
        assert_eq!(
            EINVAL,
            ChannelLayout::unspecified(0)
                .as_mut()
                .set_ambisonic(46340, 0)
                .unwrap_err()
        );
    }

    /// The entries of a `crustify_AV_CH*` table shim, by name, read until
    /// the NULL name past the last one.
    fn c_table<T>(mut entry: impl FnMut(usize, &mut *const c_char) -> T) -> Vec<(String, T)> {
        let mut out = Vec::new();
        for i in 0.. {
            let mut name: *const c_char = ptr::null();
            let value = entry(i, &mut name);
            if name.is_null() {
                break;
            }
            // SAFETY: the shim stored a static NUL-terminated literal.
            let name = unsafe { CStr::from_ptr(name) }.to_str().unwrap().to_owned();
            out.push((name, value));
        }
        out
    }

    /// `ChannelLayout::mask` is every `AV_CH_<name>` macro, and C's native
    /// bit of every id below 64.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn channel_bits_match_the_c_av_ch_macros() {
        // SAFETY: `name` is a valid slot for the shim's output.
        let table = c_table(|i, name| unsafe { ffi::crustify_AV_CH(i, name) });
        assert_eq!(table.len(), CHANNEL_BITS.len());
        for ((c_name, c_mask), (name, ch)) in table.iter().zip(CHANNEL_BITS) {
            assert_eq!(c_name, name);
            assert_eq!(ch.mask(), Some(*c_mask), "{name}");
            assert_eq!(c_mask.count_ones(), 1, "{name}");
        }
        for id in 0..64 {
            let ch = Channel::from_raw(ffi::AVChannel(id));
            let bit = ch.mask().unwrap();
            let l = ChannelLayout::from_mask(bit).unwrap();
            // SAFETY: a valid native layout C only reads.
            let (at, index) = unsafe {
                (
                    ffi::av_channel_layout_channel_from_index(rust_ptr(&l), 0),
                    ffi::av_channel_layout_index_from_channel(rust_ptr(&l), ch.into_raw()),
                )
            };
            assert_eq!((at.0, index), (id, 0), "id {id}");
        }
    }

    /// `masks::*` are the `AV_CH_LAYOUT_<name>` macros, which C's
    /// `av_channel_layout_from_mask` accepts as the standard layouts.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn layout_masks_match_the_c_av_ch_layout_macros() {
        // SAFETY: `name` is a valid slot for the shim's output.
        let table = c_table(|i, name| unsafe { ffi::crustify_AV_CH_LAYOUT(i, name) });
        assert_eq!(table.len(), LAYOUT_MASKS.len());
        for ((c_name, c_mask), (name, mask)) in table.iter().zip(LAYOUT_MASKS) {
            assert_eq!(c_name, name);
            assert_eq!(mask, *c_mask, "{name}");
            let r = ChannelLayout::from_mask(mask).unwrap();
            let mut c = CLayout::zeroed();
            // SAFETY: a zeroed layout C may initialise.
            let ret = unsafe { ffi::av_channel_layout_from_mask(&mut c.0, *c_mask) };
            assert_eq!(ret, 0);
            assert_twins(&r, &c, name);
            assert_eq!(observe(rust_ptr(&r)).check, 1, "{name}");
        }
    }

    /// `ChannelLayout::<NAME>` is each `AV_CHANNEL_LAYOUT_<NAME>` compound
    /// literal, as C's readers see it, and describes as the standard name
    /// C parses back into the same layout.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn standard_layouts_match_the_c_av_channel_layout_macros() {
        let table = c_table(|i, name| {
            let mut c = CLayout::zeroed();
            // SAFETY: `name` and `c.0` are valid slots for the shim's output.
            let found = unsafe { ffi::crustify_AV_CHANNEL_LAYOUT(i, name, &mut c.0) };
            assert_eq!(found, c_int::from(!name.is_null()));
            c
        });
        assert_eq!(table.len(), STANDARD_LAYOUTS.len());
        for (name, layout) in STANDARD_LAYOUTS {
            let (_, c) = table
                .iter()
                .find(|(c_name, _)| c_name == name)
                .unwrap_or_else(|| panic!("{name} is not a C macro"));
            let r = ChannelLayoutVal::from(layout);
            assert_twins(&r, c, name);
            let observed = observe(rust_ptr(&r));
            assert_eq!(observed.check, 1, "{name}");
            let described = CString::new(observed.describe.unwrap()).unwrap();
            let (parsed, (ret, c_parsed)) = (ChannelLayout::parse(&described), c_parse(&described));
            assert_eq!(ret, 0, "{name} as {described:?}");
            assert_twins(
                &parsed.unwrap(),
                &c_parsed,
                &format!("{name} as {described:?}"),
            );
            // SAFETY: valid layouts C only reads.
            let same = unsafe { ffi::av_channel_layout_compare(rust_ptr(&r), &c_parsed.0) };
            assert_eq!(same, 0, "{name} as {described:?}");
        }
        // The C header's table is the Rust constants, name for name.
        let mut c_names: Vec<&str> = table.iter().map(|(n, _)| n.as_str()).collect();
        let mut r_names: Vec<&str> = STANDARD_LAYOUTS.iter().map(|(n, _)| *n).collect();
        c_names.sort_unstable();
        r_names.sort_unstable();
        assert_eq!(c_names, r_names);
    }

    /// `ChannelLayout::native(m)` is `AV_CHANNEL_LAYOUT_MASK(popcount(m), m)`,
    /// the empty mask included, and `av_channel_layout_from_mask` otherwise.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn native_matches_the_c_av_channel_layout_mask_macro() {
        for mask in [0, 1, 0x3, 0x60f, 1 << 63, u64::MAX, 0x8000_0000_0000_0001] {
            let r = ChannelLayoutVal::from(ChannelLayout::native(mask));
            let mut c = CLayout::zeroed();
            let nb = mask.count_ones() as c_int;
            // SAFETY: `c.0` is a valid slot for the macro's value.
            unsafe { ffi::crustify_AV_CHANNEL_LAYOUT_MASK(nb, mask, &mut c.0) };
            assert_twins(&r, &c, &format!("mask {mask:#x}"));
            if mask != 0 {
                let mut init = CLayout::zeroed();
                // SAFETY: a zeroed layout C may initialise.
                let ret = unsafe { ffi::av_channel_layout_from_mask(&mut init.0, mask) };
                assert_eq!(ret, 0);
                assert_twins(&r, &init, &format!("from_mask {mask:#x}"));
            }
        }
    }

    /// `ChannelLayoutMut::retype` and its siblings, `ChannelLayoutRef::subset`
    /// and `standard_layouts` against `av_channel_layout_retype`,
    /// `av_channel_layout_subset` and `av_channel_layout_standard` (formats
    /// wave-3 batch 2).
    mod order_changes {
        use ffmpeg::libavutil::channel_layout::{Conversion, masks, standard_layouts};

        use super::*;

        const LOSSLESS: c_int = ffi::AV_CHANNEL_LAYOUT_RETYPE_FLAG_LOSSLESS as c_int;
        const CANONICAL: c_int = ffi::AV_CHANNEL_LAYOUT_RETYPE_FLAG_CANONICAL as c_int;

        /// The orders C knows, and one it does not.
        const ORDERS: [c_uint; 5] = [0, 1, 2, 3, 7];

        /// Masks to intersect with: none, all, single high and low bits, a
        /// standard layout, and bits around the native/user boundary.
        const MASKS: [u64; 9] = [
            0,
            u64::MAX,
            0x3,
            masks::L5POINT1,
            1 << 62,
            1 << 63,
            0x8000_0000_0000_0001,
            0x0000_0100_0000_0108,
            0xdead_beef_0bad_f00d,
        ];

        const UNNAMED_NATIVE: &[(c_int, &CStr)] = &[(0, c""), (1, c""), (3, c""), (62, c"")];
        const UNNAMED_DESCENDING: &[(c_int, &CStr)] = &[(1, c""), (0, c"")];
        const REPEATED: &[(c_int, &CStr)] = &[(2, c""), (2, c"")];
        const UNKNOWN: &[(c_int, &CStr)] = &[(0x300, c""), (0x300, c""), (0x300, c"")];
        const UNKNOWN_NAMED: &[(c_int, &CStr)] = &[(0x300, c""), (0x300, c"x")];
        const UNUSED: &[(c_int, &CStr)] = &[(0x200, c"")];
        const USER: &[(c_int, &CStr)] = &[(63, c""), (40, c"")];
        const AMBI_FIRST: &[(c_int, &CStr)] = &[
            (0x400, c""),
            (0x401, c""),
            (0x402, c""),
            (0x403, c""),
            (0, c""),
            (1, c""),
        ];
        const AMBI_NAMED: &[(c_int, &CStr)] =
            &[(0x400, c"w"), (0x401, c""), (0x402, c""), (0x403, c"")];
        const AMBI_ZERO_ONLY: &[(c_int, &CStr)] = &[(0x400, c"")];
        const AMBI_INCOMPLETE: &[(c_int, &CStr)] = &[(0x400, c""), (0x401, c"")];
        const AMBI_SWAPPED: &[(c_int, &CStr)] =
            &[(0x401, c""), (0x400, c""), (0x402, c""), (0x403, c"")];
        const AMBI_AFTER_NATIVE: &[(c_int, &CStr)] = &[(0, c""), (0x400, c"")];
        const AMBI_EXTRA_DESCENDING: &[(c_int, &CStr)] = &[(0x400, c""), (3, c""), (1, c"")];
        const AMBI_EXTRA_USER: &[(c_int, &CStr)] = &[(0x400, c""), (63, c"")];
        const WITH_NONE: &[(c_int, &CStr)] = &[(0, c""), (-1, c"")];

        /// Setter sequences building each starting layout on both twins.
        pub(super) fn starts() -> Vec<Vec<Op>> {
            use Op::*;
            let maps = [
                MAP_A,
                MAP_B,
                MAP_C,
                UNNAMED_NATIVE,
                UNNAMED_DESCENDING,
                REPEATED,
                UNKNOWN,
                UNKNOWN_NAMED,
                UNUSED,
                USER,
                AMBI_FIRST,
                AMBI_NAMED,
                AMBI_ZERO_ONLY,
                AMBI_INCOMPLETE,
                AMBI_SWAPPED,
                AMBI_AFTER_NATIVE,
                AMBI_EXTRA_DESCENDING,
                AMBI_EXTRA_USER,
                WITH_NONE,
            ];
            let mut starts: Vec<Vec<Op>> = vec![
                vec![],
                vec![Unspecified(3)],
                vec![Unspecified(-2)],
                vec![Native(0x3)],
                vec![Native(masks::L22POINT2)],
                vec![Native(u64::MAX)],
                vec![Ambisonic(0, 0)],
                vec![Ambisonic(1, 0)],
                vec![Ambisonic(2, 0x3)],
                vec![Ambisonic(0, 1 << 63)],
                vec![Parse(c"FL@x+FR@y")],
                vec![Parse(c"5.1(side)")],
                vec![CustomMap(MAP_A), TakeCustomMap],
            ];
            starts.extend(maps.into_iter().map(|m| vec![CustomMap(m)]));
            // The user pointer rides along every conversion.
            for s in &mut starts {
                s.push(Opaque(0x50));
            }
            starts
        }

        /// Twins built by `ops`.
        pub(super) fn twins(ops: &[Op]) -> (ChannelLayoutVal, CLayout) {
            let (r_other, c_other) = (ChannelLayout::unspecified(0), CLayout::zeroed());
            let mut r = ChannelLayout::unspecified(0);
            let mut c = CLayout::zeroed();
            for &op in ops {
                assert_eq!(
                    rust_apply(&mut r, &r_other, op),
                    c_apply(&mut c, &c_other, op),
                    "{op:?}"
                );
            }
            assert_twins(&r, &c, &format!("{ops:?}"));
            (r, c)
        }

        pub(super) fn c_retype(c: &mut CLayout, order: c_uint, flags: c_int) -> c_int {
            // SAFETY: a valid layout C may convert in place.
            unsafe { ffi::av_channel_layout_retype(&mut c.0, ffi::AVChannelOrder(order), flags) }
        }

        /// The safe call standing for `flags`, its result as C reports it.
        pub(super) fn rust_retype(r: &mut ChannelLayoutVal, order: c_uint, flags: c_int) -> c_int {
            let order = ChannelOrder::from_raw(ffi::AVChannelOrder(order));
            let done = |res: Result<(), c_int>| res.map_or_else(|e| e, |()| 0);
            if flags & CANONICAL != 0 {
                done(r.as_mut().canonicalize())
            } else if flags & LOSSLESS != 0 {
                done(r.as_mut().retype_lossless(order))
            } else {
                match r.as_mut().retype(order) {
                    Ok(Conversion::Lossless) => 0,
                    Ok(Conversion::Lossy) => 1,
                    Err(e) => e,
                }
            }
        }

        fn assert_subsets(r: &ChannelLayoutVal, c: &CLayout, step: &str) {
            for mask in MASKS {
                // SAFETY: a valid layout C only reads.
                let want = unsafe { ffi::av_channel_layout_subset(&c.0, mask) };
                assert_eq!(r.as_ref().subset(mask), want, "{step}: {mask:#x}");
            }
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn retype_matches_c_for_every_start_order_and_flag() {
            let mut outcomes = [0usize; 4];
            for ops in starts() {
                for order in ORDERS {
                    for flags in [0, LOSSLESS, CANONICAL, LOSSLESS | CANONICAL] {
                        let step = format!("{ops:?} -> {order} flags {flags}");
                        let (mut r, mut c) = twins(&ops);
                        let want = c_retype(&mut c, order, flags);
                        assert_eq!(rust_retype(&mut r, order, flags), want, "{step}");
                        assert_twins(&r, &c, &step);
                        assert_subsets(&r, &c, &step);
                        outcomes[match want {
                            0 => 0,
                            1 => 1,
                            -38 => 2,
                            _ => 3,
                        }] += 1;
                    }
                }
            }
            // Lossless, lossy, ENOSYS and EINVAL outcomes are all reached.
            assert!(outcomes.iter().all(|&n| n > 0), "{outcomes:?}");
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn retype_sequences_match_c_after_every_step() {
            use Op::*;
            /// One step: a conversion (order, flags) or a setter.
            #[derive(Clone, Copy, Debug)]
            enum Step {
                Retype(c_uint, c_int),
                Set(Op),
            }
            use Step::*;
            let steps = [
                Set(Native(masks::L5POINT1)),
                Set(Opaque(0x60)),
                Retype(2, 0),
                Set(Edit(1, 0x300, c"")),
                Retype(1, 0),
                Retype(0, CANONICAL),
                Set(Edit(1, 1, c"")),
                Retype(1, LOSSLESS),
                Retype(1, CANONICAL),
                Retype(3, 0),
                Retype(2, 0),
                Set(Edit(0, 0, c"named")),
                Retype(1, LOSSLESS),
                Retype(0, CANONICAL),
                Retype(1, 0),
                Retype(0, LOSSLESS),
                Retype(0, 0),
                Retype(2, 0),
                Retype(0, CANONICAL),
                Retype(3, LOSSLESS),
                Set(CustomMap(AMBI_FIRST)),
                Retype(3, LOSSLESS),
                Retype(1, 0),
                Retype(2, LOSSLESS),
                Set(Edit(2, 0x402, c"y")),
                Retype(0, CANONICAL),
                Retype(3, 0),
                Retype(7, 0),
                Retype(2, CANONICAL),
                Set(Clear),
                Retype(2, 0),
                Set(Unspecified(2)),
                Retype(2, 0),
                Retype(0, CANONICAL),
                Retype(1, 0),
            ];
            let (r_other, c_other) = (ChannelLayout::unspecified(0), CLayout::zeroed());
            let mut r = ChannelLayout::unspecified(0);
            let mut c = CLayout::zeroed();
            for (n, s) in steps.into_iter().enumerate() {
                let step = format!("step {n} {s:?}");
                match s {
                    Retype(order, flags) => {
                        let want = c_retype(&mut c, order, flags);
                        assert_eq!(rust_retype(&mut r, order, flags), want, "{step}");
                    }
                    Set(op) => assert_eq!(
                        rust_apply(&mut r, &r_other, op),
                        c_apply(&mut c, &c_other, op),
                        "{step}"
                    ),
                }
                assert_twins(&r, &c, &step);
                assert_subsets(&r, &c, &step);
            }
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn subset_matches_c_on_every_start_and_standard_layout() {
            for ops in starts() {
                let (r, c) = twins(&ops);
                assert_subsets(&r, &c, &format!("{ops:?}"));
            }
            for (name, layout) in STANDARD_LAYOUTS {
                let r = ChannelLayoutVal::from(layout);
                let mut c = CLayout::zeroed();
                // SAFETY: both are valid, distinct layouts.
                let ret = unsafe { ffi::av_channel_layout_copy(&mut c.0, rust_ptr(&r)) };
                assert_eq!(ret, 0);
                assert_subsets(&r, &c, name);
            }
            // Every single-bit mask against a map holding each native id once.
            const ALL_NATIVE: [(c_int, &CStr); 63] = {
                let mut all = [(0, c""); 63];
                let mut i = 0;
                while i < 63 {
                    all[i].0 = i as c_int;
                    i += 1;
                }
                all
            };
            let (r, c) = twins(&[Op::CustomMap(&ALL_NATIVE), Op::Edit(5, 0x300, c"")]);
            for bit in 0..64 {
                // SAFETY: a valid layout C only reads.
                let want = unsafe { ffi::av_channel_layout_subset(&c.0, 1 << bit) };
                assert_eq!(r.as_ref().subset(1 << bit), want, "bit {bit}");
            }
        }

        /// C's own walk of its table, from a NULL state until it returns NULL.
        fn c_standard() -> Vec<*const ffi::AVChannelLayout> {
            let mut state: *mut c_void = ptr::null_mut();
            let mut out = vec![];
            loop {
                // SAFETY: `state` is a writable slot C started from NULL.
                let l = unsafe { ffi::av_channel_layout_standard(&mut state) };
                if l.is_null() {
                    break;
                }
                out.push(l);
            }
            // Past the end C keeps returning NULL and leaves the state.
            let end = state;
            for _ in 0..3 {
                // SAFETY: as above.
                assert!(unsafe { ffi::av_channel_layout_standard(&mut state) }.is_null());
                assert_eq!(state, end);
            }
            out
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn standard_layouts_are_cs_table_in_order() {
            let c = c_standard();
            let mut it = standard_layouts();
            let rust: Vec<_> = it.by_ref().collect();
            assert_eq!(rust.len(), c.len());
            assert_eq!(rust.len(), 40);
            for _ in 0..3 {
                assert!(it.next().is_none());
            }
            for (i, (&r, &p)) in rust.iter().zip(&c).enumerate() {
                assert_eq!(rust_fields(r), c_fields(p), "entry {i}");
                // A deep copy reads to C as the entry it was copied from.
                let owned = r.try_clone().unwrap();
                assert_eq!(observe(rust_ptr(&owned)), observe(p), "entry {i}");
                // SAFETY: two valid layouts C only reads.
                let cmp = unsafe { ffi::av_channel_layout_compare(rust_ptr(&owned), p) };
                assert_eq!(cmp, 0);
            }
            // Each entry is the native layout one of the header's
            // `AV_CHANNEL_LAYOUT_*` initialisers builds.
            for r in &rust {
                let LayoutDetails::Native(mask) = r.details() else {
                    panic!("{r:?}")
                };
                assert!(STANDARD_LAYOUTS.iter().any(|(_, l)| {
                    matches!(l.as_ref().details(), LayoutDetails::Native(m) if m == mask)
                }));
            }
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn standard_layouts_resume_where_cs_state_would() {
            // A walk interrupted after `k` entries and resumed from a copy
            // matches C resumed from the state it left after `k` calls.
            let c = c_standard();
            for k in [0, 1, 17, 39, 40] {
                let mut it = standard_layouts();
                for _ in 0..k {
                    it.next();
                }
                let resumed: Vec<_> = it.map(rust_fields).collect();
                let want: Vec<_> = c[k..].iter().map(|&p| c_fields(p)).collect();
                assert_eq!(resumed, want, "after {k}");
            }
        }
    }

    /// The readers of `ChannelLayoutRef` (`is_valid`, `ambisonic_order`,
    /// `channel`, `index_of`, `index_by_name`, `channel_by_name`,
    /// `write_description`, `description`, `append_description`) and `==`
    /// against `av_channel_layout_check`, `_ambisonic_order`,
    /// `_channel_from_index`, `_index_from_channel`, `_index_from_string`,
    /// `_channel_from_string`, `_describe`, `_describe_bprint` and
    /// `_compare` on twin layouts: every start state, every standard layout,
    /// every buffer size around the description's, every pair of starts, and
    /// sequences of conversions and setters compared after every step
    /// (formats wave-3 batch 1).
    mod readers {
        use core::mem::MaybeUninit;

        use ffmpeg::libavutil::bprint::BPrint;
        use ffmpeg::libavutil::channel_layout::masks;

        use super::order_changes::{c_retype, rust_retype, starts, twins};
        use super::*;

        /// Channel ids at every class: `NONE`, native, user, the Ambisonic
        /// bounds, the markers and the `int` extremes.
        const IDS: [c_int; 17] = [
            -1,
            0,
            1,
            3,
            40,
            62,
            63,
            64,
            0x200,
            0x300,
            0x400,
            0x403,
            0x404,
            0x7ff,
            0x800,
            c_int::MAX,
            c_int::MIN,
        ];

        /// Lookups of every shape, the start maps' names included.
        const NAMES: [&CStr; 24] = [
            c"",
            c"@",
            c"FL",
            c"FR",
            c"LFE",
            c"USR40",
            c"USR63",
            c"AMBI0",
            c"AMBI4",
            c"UNK",
            c"FL@",
            c"@x",
            c"FR@y",
            c"FL@y",
            c"@front",
            c"FL@front",
            c"FR@front",
            c"XX@front",
            c"@right",
            c"@amb",
            c"AMBI4@amb",
            c"@0123456789abcde",
            c"USR63@0123456789abcde",
            c"nope",
        ];

        /// Everything the readers report about one layout, in the Rust
        /// API's terms.
        #[derive(Debug, PartialEq)]
        struct Reads {
            valid: bool,
            ambisonic_order: Option<u32>,
            at: Vec<Option<c_int>>,
            index: Vec<Option<usize>>,
            by_name: Vec<(Option<usize>, Option<c_int>)>,
            description: Result<Vec<u8>, c_int>,
            /// `(result, buffer)` written into each size around the
            /// description's.
            sized: Vec<(Result<usize, c_int>, Vec<u8>)>,
            /// A print buffer holding `"x:"` with the description appended.
            appended: (Result<(), c_int>, Vec<u8>),
        }

        /// Indices around the layout's channels and `unsigned`'s extremes.
        fn indices(nb_channels: c_int) -> Vec<c_uint> {
            let n = nb_channels.clamp(0, 70) as c_uint;
            (0..=n + 1)
                .chain([c_int::MAX as c_uint, c_int::MAX as c_uint + 1, c_uint::MAX])
                .collect()
        }

        /// Buffer sizes from empty to past the description's `need`.
        fn sizes(need: usize) -> Vec<usize> {
            let mut s: Vec<usize> = (0..need.min(6)).collect();
            s.extend([need.saturating_sub(1), need, need + 3]);
            s
        }

        fn none_to_option(ch: ffi::AVChannel) -> Option<c_int> {
            (ch != ffi::AVChannel::AV_CHAN_NONE).then_some(ch.0)
        }

        fn rust_reads(l: ChannelLayoutRef<'_>) -> Reads {
            let need = l.write_description(&mut []).unwrap_or(0);
            let sized = sizes(need)
                .into_iter()
                .map(|len| {
                    let mut buf = vec![0xAAu8; len];
                    (l.write_description(&mut buf), buf)
                })
                .collect();
            let mut b = BPrint::new(0, BPrint::SIZE_UNLIMITED).unwrap();
            b.as_mut().append_bytes(b"x:");
            let res = l.append_description(b.as_mut());
            Reads {
                valid: l.is_valid(),
                ambisonic_order: l.ambisonic_order(),
                at: indices(l.channel_count())
                    .into_iter()
                    .map(|i| l.channel(i as usize).map(|ch| ch.into_raw().0))
                    .collect(),
                index: IDS
                    .map(|id| l.index_of(Channel::from_raw(ffi::AVChannel(id))))
                    .into(),
                by_name: NAMES
                    .map(|s| {
                        let ch = l.channel_by_name(s).map(|ch| ch.into_raw().0);
                        (l.index_by_name(s), ch)
                    })
                    .into(),
                description: l.description().map(CString::into_bytes),
                sized,
                appended: (res, b.as_ref().as_bytes().to_vec()),
            }
        }

        /// What C's readers report, mapped onto the Rust API's terms: a
        /// negative index or order is `None`, as is `AV_CHAN_NONE`.
        fn c_reads(p: *const ffi::AVChannelLayout) -> Reads {
            // SAFETY: `p` addresses a live, valid layout every call below
            // only reads; each buffer is valid for the size passed (NULL
            // with size 0 asks for the length only); the print buffer is
            // initialised before use and finalised once.
            unsafe {
                let nb = (*p).nb_channels;
                let need = ffi::av_channel_layout_describe(p, ptr::null_mut(), 0);
                let description = if need < 0 {
                    Err(need)
                } else {
                    let mut buf = vec![0 as c_char; need as usize];
                    let ret = ffi::av_channel_layout_describe(p, buf.as_mut_ptr(), buf.len());
                    assert_eq!(ret, need);
                    Ok(CStr::from_ptr(buf.as_ptr()).to_bytes().to_vec())
                };
                let sized = sizes(usize::try_from(need).unwrap_or(0))
                    .into_iter()
                    .map(|len| {
                        let mut buf = vec![0xAAu8; len];
                        let ptr = if len == 0 {
                            ptr::null_mut()
                        } else {
                            buf.as_mut_ptr().cast()
                        };
                        let ret = ffi::av_channel_layout_describe(p, ptr, len);
                        (usize::try_from(ret).map_err(|_| ret), buf)
                    })
                    .collect();
                let mut bp = MaybeUninit::<ffi::AVBPrint>::uninit();
                ffi::av_bprint_init(bp.as_mut_ptr(), 0, c_uint::MAX);
                ffi::av_bprintf(bp.as_mut_ptr(), c"x:".as_ptr());
                let ret = ffi::av_channel_layout_describe_bprint(p, bp.as_mut_ptr());
                let appended = CStr::from_ptr((*bp.as_ptr()).str_).to_bytes().to_vec();
                ffi::av_bprint_finalize(bp.as_mut_ptr(), ptr::null_mut());
                Reads {
                    valid: ffi::av_channel_layout_check(p) == 1,
                    ambisonic_order: u32::try_from(ffi::av_channel_layout_ambisonic_order(p)).ok(),
                    at: indices(nb)
                        .into_iter()
                        .map(|i| none_to_option(ffi::av_channel_layout_channel_from_index(p, i)))
                        .collect(),
                    index: IDS
                        .map(|id| {
                            let ret =
                                ffi::av_channel_layout_index_from_channel(p, ffi::AVChannel(id));
                            usize::try_from(ret).ok()
                        })
                        .into(),
                    by_name: NAMES
                        .map(|s| {
                            let at = ffi::av_channel_layout_index_from_string(p, s.as_ptr());
                            let ch = ffi::av_channel_layout_channel_from_string(p, s.as_ptr());
                            (usize::try_from(at).ok(), none_to_option(ch))
                        })
                        .into(),
                    description,
                    sized,
                    appended: (if ret < 0 { Err(ret) } else { Ok(()) }, appended),
                }
            }
        }

        /// The twins read alike through the safe readers and C's.
        fn assert_reads(r: &ChannelLayoutVal, c: &CLayout, step: &str) {
            let (rr, cr) = (rust_reads(r.as_ref()), c_reads(&c.0));
            assert_eq!(rr, cr, "{step}");
            // The Rust twin read through C, too, is the C twin.
            assert_eq!(c_reads(rust_ptr(r)), cr, "{step}");
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn readers_match_c_on_every_start() {
            let mut described = 0;
            for ops in starts() {
                let step = format!("{ops:?}");
                let (r, c) = twins(&ops);
                assert_reads(&r, &c, &step);
                described += usize::from(r.as_ref().description().is_ok());
            }
            assert!(described > 20, "{described}");
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn readers_match_c_on_every_standard_layout_and_description() {
            for (name, layout) in STANDARD_LAYOUTS {
                let r = ChannelLayoutVal::from(layout);
                let mut c = CLayout::zeroed();
                // SAFETY: both are valid, distinct layouts.
                let ret = unsafe { ffi::av_channel_layout_copy(&mut c.0, rust_ptr(&r)) };
                assert_eq!(ret, 0);
                assert_reads(&r, &c, name);
            }
            for s in allowed(&DESCRIPTIONS) {
                let (ret, c) = c_parse(s);
                if ret == 0 {
                    assert_reads(&ChannelLayout::parse(s).unwrap(), &c, &format!("{s:?}"));
                }
            }
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn equality_matches_c_compare_for_every_pair() {
            let all: Vec<_> = starts().iter().map(|ops| twins(ops)).collect();
            let mut equal = 0;
            for (i, (ra, ca)) in all.iter().enumerate() {
                for (j, (rb, cb)) in all.iter().enumerate() {
                    // SAFETY: two valid layouts C only reads.
                    let want = unsafe { ffi::av_channel_layout_compare(&ca.0, &cb.0) };
                    assert!(want == 0 || want == 1, "{want}");
                    let eq = ra.as_ref() == rb.as_ref();
                    assert_eq!(eq, want == 0, "{i} {j}");
                    // SAFETY: as above, a Rust-held layout against a C one.
                    let mixed = unsafe { ffi::av_channel_layout_compare(rust_ptr(ra), &cb.0) };
                    assert_eq!(mixed, want, "{i} {j}");
                    equal += usize::from(eq && i != j);
                }
            }
            assert!(equal > 0);
            // The bare values compare as their handles do.
            let entry = |i: usize| STANDARD_LAYOUTS.into_iter().nth(i).unwrap();
            for i in 0..STANDARD_LAYOUTS.len() {
                for j in 0..STANDARD_LAYOUTS.len() {
                    let ((na, a), (nb, b)) = (entry(i), entry(j));
                    let eq = a == b;
                    let (ra, rb) = (ChannelLayoutVal::from(a), ChannelLayoutVal::from(b));
                    let (pa, pb) = (rust_ptr(&ra), rust_ptr(&rb));
                    // SAFETY: two valid layouts C only reads.
                    let want = unsafe { ffi::av_channel_layout_compare(pa, pb) };
                    assert_eq!(eq, want == 0, "{na} {nb}");
                }
            }
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn readers_match_c_after_every_step_of_a_sequence() {
            use Op::*;
            /// One step: a conversion (order, flags) or a setter.
            #[derive(Clone, Copy, Debug)]
            enum Step {
                Retype(c_uint, c_int),
                Set(Op),
            }
            use Step::*;
            const LOSSLESS: c_int = ffi::AV_CHANNEL_LAYOUT_RETYPE_FLAG_LOSSLESS as c_int;
            const CANONICAL: c_int = ffi::AV_CHANNEL_LAYOUT_RETYPE_FLAG_CANONICAL as c_int;
            let steps = [
                Set(Native(masks::L7POINT1)),
                Retype(2, 0),
                Set(Edit(2, 0x300, c"centre")),
                Set(Edit(5, 40, c"")),
                Retype(0, CANONICAL),
                Set(Edit(2, 2, c"")),
                Retype(0, CANONICAL),
                Retype(3, 0),
                Set(Ambisonic(2, 0x3)),
                Retype(2, 0),
                Set(Edit(9, 0x401, c"w")),
                Retype(3, LOSSLESS),
                Set(Edit(9, 1, c"")),
                Retype(3, LOSSLESS),
                Retype(2, 0),
                Set(Edit(0, -1, c"")),
                Retype(1, 0),
                Set(Parse(c"FL@x+FR@y")),
                Set(Edit(1, 0, c"x")),
                Retype(1, 0),
                Set(Unspecified(5)),
                Retype(2, 0),
                Set(Opaque(0x70)),
                Set(CustomMap(MAP_B)),
                Set(Take),
                Set(Ambisonic(46339, 1 << 63)),
                Set(Native(u64::MAX)),
                Retype(2, 0),
                Set(Clear),
            ];
            let (r_other, c_other) = (ChannelLayout::unspecified(0), CLayout::zeroed());
            let mut r = ChannelLayout::unspecified(0);
            let mut c = CLayout::zeroed();
            assert_reads(&r, &c, "start");
            for (n, s) in steps.into_iter().enumerate() {
                let step = format!("step {n} {s:?}");
                match s {
                    Retype(order, flags) => {
                        let want = c_retype(&mut c, order, flags);
                        assert_eq!(rust_retype(&mut r, order, flags), want, "{step}");
                    }
                    Set(op) => assert_eq!(
                        rust_apply(&mut r, &r_other, op),
                        c_apply(&mut c, &c_other, op),
                        "{step}"
                    ),
                }
                assert_twins(&r, &c, &step);
                assert_reads(&r, &c, &step);
            }
        }
    }
}
