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
