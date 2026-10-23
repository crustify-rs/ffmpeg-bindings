//! Shared by the `channel_layout` suites of `equiv` and `ub_safe`, which
//! include it with `#[path]`: the header's macro constants beside their Rust
//! spellings, and which descriptions `av_channel_layout_from_string` can be
//! given under BorrowSanitizer.
//!
//! BSan reports a false "access an allocation that has been freed" when C
//! reads through an end pointer that uninstrumented glibc `strtol` /
//! `strtoull` wrote into an uninitialised local (`channel_layout.c:342`
//! `*endptr`, `:417` `*end`): the store does not update the local's shadow,
//! which keeps the provenance an earlier frame left in that stack slot, a
//! freed `av_strdup` block when an earlier `av_channel_layout_from_string`
//! call put its `chlist` there. Which inputs trip it therefore depends on the
//! caller's frames, not on the input alone, so every input whose parse reaches
//! those lines is skipped under BSan; see
//! `crustify/reviews/20261005/libavutil/formats/wave-2/batch-0/misc/bsan-stale-stack-shadow-under-glibc-strtol-end-pointers`.
//!
//! `av_channel_from_string`'s `USR<n>` path initialises its `endptr` to the
//! string before `strtol` overwrites it, so its shadow stays live: channel
//! lists are safe, and this predicate mirrors `av_channel_layout_from_string`
//! only as far as needed to tell whether C stops before `strtol`.
#![allow(dead_code)]

use core::ffi::CStr;
use std::ffi::CString;

use ffmpeg_bindings::libavutil::libavutil::channel_layout::{Channel, ChannelLayout, masks};

/// `channel_layout.c`'s `channel_layout_map` names, which
/// `av_channel_layout_from_string` matches before parsing anything.
const STANDARD_NAMES: [&str; 40] = [
    "mono",
    "stereo",
    "2.1",
    "3.0",
    "3.0(back)",
    "4.0",
    "quad",
    "quad(side)",
    "3.1",
    "5.0",
    "5.0(side)",
    "4.1",
    "5.1",
    "5.1(side)",
    "6.0",
    "6.0(front)",
    "3.1.2",
    "hexagonal",
    "6.1",
    "6.1(back)",
    "6.1(front)",
    "7.0",
    "7.0(front)",
    "7.1",
    "7.1(wide)",
    "7.1(wide-side)",
    "5.1.2",
    "5.1.2(back)",
    "octagonal",
    "cube",
    "5.1.4",
    "7.1.2",
    "7.1.4",
    "7.2.3",
    "9.1.4",
    "9.1.6",
    "hexadecagonal",
    "binaural",
    "downmix",
    "22.2",
];

/// Whether this run is under BSan.
pub fn under_bsan() -> bool {
    std::env::var("CRUSTIFY_FFMPEG_VARIANT").as_deref() == Ok("bsan")
}

/// Whether `av_channel_layout_from_string(description)` returns before its
/// `strtol` / `strtoull` calls: a standard layout name, or a channel list
/// `parse_channel_list` accepts, plain (`FL@left+FR`) or inside
/// `<n> channels (...)`. Anything else, every `ambisonic` form included, is
/// parsed through glibc end pointers, or is too irregular (quotes, escapes,
/// blanks, empty keys) to be sure it is not.
pub fn parsed_before_strtol(description: &CStr) -> bool {
    let Ok(s) = description.to_str() else {
        return false;
    };
    if STANDARD_NAMES.contains(&s) {
        return true;
    }
    if s.starts_with("ambisonic ") {
        return false;
    }
    channel_list_accepted(channel_list_of(s))
}

/// The list `av_sscanf(str, "%d channels (%[^)]", ...)` cuts out of `s`, or
/// `s` itself when the pattern does not match whole.
fn channel_list_of(s: &str) -> &str {
    let t = s.trim_start_matches([' ', '\t', '\n', '\x0b', '\x0c', '\r']);
    let digits = t.trim_start_matches(['+', '-']);
    let n = digits.bytes().take_while(u8::is_ascii_digit).count();
    if n == 0 {
        return s;
    }
    let Some(rest) = digits[n..].strip_prefix(" channels (") else {
        return s;
    };
    let inner = rest.split(')').next().unwrap_or("");
    if inner.is_empty() { s } else { inner }
}

/// Whether `parse_channel_list` accepts `list`: at least one `+`-separated
/// token, each a channel abbreviation `av_channel_from_string` knows, with
/// an optional `@name`.
fn channel_list_accepted(list: &str) -> bool {
    !list.is_empty()
        && list.split('+').all(|token| {
            let key = token.split_once('@').map_or(token, |(key, _)| key);
            !key.is_empty()
                && key.bytes().all(|b| b.is_ascii_alphanumeric())
                && !token.contains(['\\', '\'', '"', ' ', '\t'])
                && CString::new(key).is_ok_and(|key| Channel::from_name(&key).is_some())
        })
}

/// Whether `description` must not be given to `av_channel_layout_from_string`
/// in this run: under BSan, every input parsed through glibc end pointers.
pub fn skipped_under_bsan(description: &CStr) -> bool {
    under_bsan() && !parsed_before_strtol(description)
}

/// `descriptions` without the ones [`skipped_under_bsan`].
pub fn allowed<'a>(descriptions: &'a [&'a CStr]) -> impl Iterator<Item = &'a CStr> + 'a {
    descriptions
        .iter()
        .copied()
        .filter(|s| !skipped_under_bsan(s))
}

/// Every `AV_CH_<name>` channel bit beside its [`Channel`], in header order.
pub const CHANNEL_BITS: [(&str, Channel); 36] = [
    ("FRONT_LEFT", Channel::FRONT_LEFT),
    ("FRONT_RIGHT", Channel::FRONT_RIGHT),
    ("FRONT_CENTER", Channel::FRONT_CENTER),
    ("LOW_FREQUENCY", Channel::LOW_FREQUENCY),
    ("BACK_LEFT", Channel::BACK_LEFT),
    ("BACK_RIGHT", Channel::BACK_RIGHT),
    ("FRONT_LEFT_OF_CENTER", Channel::FRONT_LEFT_OF_CENTER),
    ("FRONT_RIGHT_OF_CENTER", Channel::FRONT_RIGHT_OF_CENTER),
    ("BACK_CENTER", Channel::BACK_CENTER),
    ("SIDE_LEFT", Channel::SIDE_LEFT),
    ("SIDE_RIGHT", Channel::SIDE_RIGHT),
    ("TOP_CENTER", Channel::TOP_CENTER),
    ("TOP_FRONT_LEFT", Channel::TOP_FRONT_LEFT),
    ("TOP_FRONT_CENTER", Channel::TOP_FRONT_CENTER),
    ("TOP_FRONT_RIGHT", Channel::TOP_FRONT_RIGHT),
    ("TOP_BACK_LEFT", Channel::TOP_BACK_LEFT),
    ("TOP_BACK_CENTER", Channel::TOP_BACK_CENTER),
    ("TOP_BACK_RIGHT", Channel::TOP_BACK_RIGHT),
    ("STEREO_LEFT", Channel::STEREO_LEFT),
    ("STEREO_RIGHT", Channel::STEREO_RIGHT),
    ("WIDE_LEFT", Channel::WIDE_LEFT),
    ("WIDE_RIGHT", Channel::WIDE_RIGHT),
    ("SURROUND_DIRECT_LEFT", Channel::SURROUND_DIRECT_LEFT),
    ("SURROUND_DIRECT_RIGHT", Channel::SURROUND_DIRECT_RIGHT),
    ("LOW_FREQUENCY_2", Channel::LOW_FREQUENCY_2),
    ("TOP_SIDE_LEFT", Channel::TOP_SIDE_LEFT),
    ("TOP_SIDE_RIGHT", Channel::TOP_SIDE_RIGHT),
    ("BOTTOM_FRONT_CENTER", Channel::BOTTOM_FRONT_CENTER),
    ("BOTTOM_FRONT_LEFT", Channel::BOTTOM_FRONT_LEFT),
    ("BOTTOM_FRONT_RIGHT", Channel::BOTTOM_FRONT_RIGHT),
    ("SIDE_SURROUND_LEFT", Channel::SIDE_SURROUND_LEFT),
    ("SIDE_SURROUND_RIGHT", Channel::SIDE_SURROUND_RIGHT),
    ("TOP_SURROUND_LEFT", Channel::TOP_SURROUND_LEFT),
    ("TOP_SURROUND_RIGHT", Channel::TOP_SURROUND_RIGHT),
    ("BINAURAL_LEFT", Channel::BINAURAL_LEFT),
    ("BINAURAL_RIGHT", Channel::BINAURAL_RIGHT),
];

/// Every `AV_CH_LAYOUT_<name>` beside its [`masks`] constant, in header
/// order, the deprecated aliases left out.
pub const LAYOUT_MASKS: [(&str, u64); 40] = [
    ("MONO", masks::MONO),
    ("STEREO", masks::STEREO),
    ("2POINT1", masks::L2POINT1),
    ("2_1", masks::L2_1),
    ("SURROUND", masks::SURROUND),
    ("3POINT1", masks::L3POINT1),
    ("4POINT0", masks::L4POINT0),
    ("4POINT1", masks::L4POINT1),
    ("2_2", masks::L2_2),
    ("QUAD", masks::QUAD),
    ("5POINT0", masks::L5POINT0),
    ("5POINT1", masks::L5POINT1),
    ("5POINT0_BACK", masks::L5POINT0_BACK),
    ("5POINT1_BACK", masks::L5POINT1_BACK),
    ("6POINT0", masks::L6POINT0),
    ("6POINT0_FRONT", masks::L6POINT0_FRONT),
    ("HEXAGONAL", masks::HEXAGONAL),
    ("3POINT1POINT2", masks::L3POINT1POINT2),
    ("6POINT1", masks::L6POINT1),
    ("6POINT1_BACK", masks::L6POINT1_BACK),
    ("6POINT1_FRONT", masks::L6POINT1_FRONT),
    ("7POINT0", masks::L7POINT0),
    ("7POINT0_FRONT", masks::L7POINT0_FRONT),
    ("7POINT1", masks::L7POINT1),
    ("7POINT1_WIDE", masks::L7POINT1_WIDE),
    ("7POINT1_WIDE_BACK", masks::L7POINT1_WIDE_BACK),
    ("5POINT1POINT2", masks::L5POINT1POINT2),
    ("5POINT1POINT2_BACK", masks::L5POINT1POINT2_BACK),
    ("OCTAGONAL", masks::OCTAGONAL),
    ("CUBE", masks::CUBE),
    ("5POINT1POINT4", masks::L5POINT1POINT4),
    ("7POINT1POINT2", masks::L7POINT1POINT2),
    ("7POINT1POINT4", masks::L7POINT1POINT4),
    ("7POINT2POINT3", masks::L7POINT2POINT3),
    ("9POINT1POINT4", masks::L9POINT1POINT4),
    ("9POINT1POINT6", masks::L9POINT1POINT6),
    ("HEXADECAGONAL", masks::HEXADECAGONAL),
    ("BINAURAL", masks::BINAURAL),
    ("STEREO_DOWNMIX", masks::STEREO_DOWNMIX),
    ("22POINT2", masks::L22POINT2),
];

/// Every `AV_CHANNEL_LAYOUT_<name>` beside its [`ChannelLayout`] constant,
/// in header order, the deprecated aliases left out.
pub const STANDARD_LAYOUTS: [(&str, ChannelLayout); 41] = [
    ("MONO", ChannelLayout::MONO),
    ("STEREO", ChannelLayout::STEREO),
    ("2POINT1", ChannelLayout::L2POINT1),
    ("2_1", ChannelLayout::L2_1),
    ("SURROUND", ChannelLayout::SURROUND),
    ("3POINT1", ChannelLayout::L3POINT1),
    ("4POINT0", ChannelLayout::L4POINT0),
    ("4POINT1", ChannelLayout::L4POINT1),
    ("2_2", ChannelLayout::L2_2),
    ("QUAD", ChannelLayout::QUAD),
    ("5POINT0", ChannelLayout::L5POINT0),
    ("5POINT1", ChannelLayout::L5POINT1),
    ("5POINT0_BACK", ChannelLayout::L5POINT0_BACK),
    ("5POINT1_BACK", ChannelLayout::L5POINT1_BACK),
    ("6POINT0", ChannelLayout::L6POINT0),
    ("6POINT0_FRONT", ChannelLayout::L6POINT0_FRONT),
    ("HEXAGONAL", ChannelLayout::HEXAGONAL),
    ("3POINT1POINT2", ChannelLayout::L3POINT1POINT2),
    ("6POINT1", ChannelLayout::L6POINT1),
    ("6POINT1_BACK", ChannelLayout::L6POINT1_BACK),
    ("6POINT1_FRONT", ChannelLayout::L6POINT1_FRONT),
    ("7POINT0", ChannelLayout::L7POINT0),
    ("7POINT0_FRONT", ChannelLayout::L7POINT0_FRONT),
    ("7POINT1", ChannelLayout::L7POINT1),
    ("7POINT1_WIDE", ChannelLayout::L7POINT1_WIDE),
    ("7POINT1_WIDE_BACK", ChannelLayout::L7POINT1_WIDE_BACK),
    ("5POINT1POINT2", ChannelLayout::L5POINT1POINT2),
    ("5POINT1POINT2_BACK", ChannelLayout::L5POINT1POINT2_BACK),
    ("OCTAGONAL", ChannelLayout::OCTAGONAL),
    ("CUBE", ChannelLayout::CUBE),
    ("5POINT1POINT4", ChannelLayout::L5POINT1POINT4),
    ("7POINT1POINT2", ChannelLayout::L7POINT1POINT2),
    ("7POINT1POINT4", ChannelLayout::L7POINT1POINT4),
    ("7POINT2POINT3", ChannelLayout::L7POINT2POINT3),
    ("9POINT1POINT4", ChannelLayout::L9POINT1POINT4),
    ("9POINT1POINT6", ChannelLayout::L9POINT1POINT6),
    ("HEXADECAGONAL", ChannelLayout::HEXADECAGONAL),
    ("BINAURAL", ChannelLayout::BINAURAL),
    ("STEREO_DOWNMIX", ChannelLayout::STEREO_DOWNMIX),
    ("22POINT2", ChannelLayout::L22POINT2),
    (
        "AMBISONIC_FIRST_ORDER",
        ChannelLayout::AMBISONIC_FIRST_ORDER,
    ),
];
