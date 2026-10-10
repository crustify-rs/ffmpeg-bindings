//! Equivalence of `libavutil/channel_layout.h`'s [`Channel`] with the C enum,
//! as C observes it: through `av_channel_name`, `av_channel_description` and
//! `av_channel_from_string`, and through C-allocated storage holding the
//! values.

use core::ffi::{CStr, c_int, c_void};
use core::mem::size_of;

use ffmpeg::libavutil::channel_layout::Channel;
use ffmpeg::libavutil::ffi;
use ffmpeg::libavutil::mem::{HeapVec, alloc_array_zeroed};

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
    fn debug_names_exactly_the_orders_c_accepts() {
        for c in values() {
            let order = ChannelOrder::from(c);
            let named = !format!("{order:?}").starts_with("ChannelOrder(");
            assert_eq!(named, c_accepts(c), "{c:?}");
        }
    }

    #[test]
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
            let l = CLayout::from_string(s);
            assert_eq!(l.order(), want, "{s:?}");
            assert_eq!(l.order().into_raw(), l.0.order);
            assert_eq!(l.check(), 1, "{s:?}");
        }
    }

    #[test]
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
