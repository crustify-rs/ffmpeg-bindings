//! UB tests of `libavutil/bswap.h`'s safe byte swaps: every bit position and
//! the extremes, on this thread and others.
#![expect(
    clippy::cast_possible_truncation,
    reason = "test inputs are chosen within range, and the casts mirror the C conversions under comparison"
)]

use ffmpeg_bindings::libavutil::libavutil::bswap::{swap16, swap32, swap64};

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn every_bit_moves_to_its_mirrored_byte() {
    for b in 0..64u32 {
        let w = 1u64 << b;
        let mirrored = 1u64 << ((7 - b / 8) * 8 + b % 8);
        assert_eq!(swap64(w), mirrored);
        assert_eq!(swap32(w as u32), (w as u32).swap_bytes());
        assert_eq!(swap16(w as u16), (w as u16).swap_bytes());
    }
    assert_eq!(
        (swap16(u16::MAX), swap32(u32::MAX), swap64(u64::MAX)),
        (u16::MAX, u32::MAX, u64::MAX)
    );
    assert_eq!((swap16(0), swap32(0), swap64(0)), (0, 0, 0));
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn swaps_run_concurrently() {
    let workers: Vec<_> = (0..4u64)
        .map(|t| {
            std::thread::spawn(move || {
                (0..10_000u64).fold(0u64, |acc, i| {
                    let w = i.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ t;
                    acc ^ swap64(swap64(w))
                        ^ u64::from(swap32(w as u32))
                        ^ u64::from(swap16(w as u16))
                })
            })
        })
        .collect();
    for w in workers {
        w.join().unwrap();
    }
}
