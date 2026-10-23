//! Equivalence of `libavutil/bswap.h`'s safe byte swaps with the raw C
//! functions, and with Rust's `swap_bytes`.

use ffmpeg_bindings::libavutil::libavutil::bswap::{swap16, swap32, swap64};
use ffmpeg_bindings::libavutil::libavutil::ffi;

/// Every single-bit and single-byte pattern of a 64-bit word, its
/// complements, and a few mixed words.
fn words() -> impl Iterator<Item = u64> {
    let bits = (0..64).map(|b| 1u64 << b);
    let bytes = (0..8).flat_map(|i| [0x01u64, 0x7f, 0x80, 0xff].map(|v| v << (8 * i)));
    let mixed = [0, u64::MAX, 0x0123_4567_89ab_cdef, 0xdead_beef_cafe_f00d];
    bits.chain(bytes).chain(mixed).flat_map(|w| [w, !w])
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn swaps_match_c_and_swap_bytes() {
    for w in words() {
        for x in [w as u16, (w >> 16) as u16, (w >> 48) as u16] {
            // SAFETY: `av_bswap16` takes and returns an integer by value.
            let c = unsafe { ffi::av_bswap16(x) };
            assert_eq!(swap16(x), c, "{x:#x}");
            assert_eq!(c, x.swap_bytes());
        }
        for x in [w as u32, (w >> 32) as u32] {
            // SAFETY: `av_bswap32` takes and returns an integer by value.
            let c = unsafe { ffi::av_bswap32(x) };
            assert_eq!(swap32(x), c, "{x:#x}");
            assert_eq!(c, x.swap_bytes());
        }
        // SAFETY: `av_bswap64` takes and returns an integer by value.
        let c = unsafe { ffi::av_bswap64(w) };
        assert_eq!(swap64(w), c, "{w:#x}");
        assert_eq!(c, w.swap_bytes());
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn round_trips_through_either_side_agree() {
    for w in words() {
        // Rust then C, and C then Rust, each restore the word.
        // SAFETY: as above.
        assert_eq!(unsafe { ffi::av_bswap64(swap64(w)) }, w);
        // SAFETY: as above.
        assert_eq!(swap64(unsafe { ffi::av_bswap64(w) }), w);
        let x = w as u32;
        // SAFETY: as above.
        assert_eq!(swap32(unsafe { ffi::av_bswap32(x) }), x);
        let y = w as u16;
        // SAFETY: as above.
        assert_eq!(swap16(unsafe { ffi::av_bswap16(y) }), y);
    }
}
