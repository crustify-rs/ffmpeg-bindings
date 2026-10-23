//! Equivalence of `libavutil/pixelutils.h`'s safe API with the C library:
//! [`SadFn::new`] against `av_pixelutils_get_sad_fn`, by what each selection
//! is (a function or NULL) and by the sums the functions compute.
#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "test inputs are chosen within range, and the casts mirror the C conversions under comparison"
)]

use core::ffi::c_int;

use ffmpeg_bindings::libavutil::libavutil::ffi;
use ffmpeg_bindings::libavutil::libavutil::pixelutils::{SadAlignment, SadFn};

const ALIGNMENTS: [(SadAlignment, c_int); 3] = [
    (SadAlignment::Unaligned, 0),
    (SadAlignment::First, 1),
    (SadAlignment::Both, 2),
];

/// A 64-byte aligned buffer, so every block row meets every alignment.
#[repr(C, align(64))]
struct Aligned([u8; 4096]);

fn noise(seed: u32) -> Box<Aligned> {
    let mut x = seed | 1;
    Box::new(Aligned(core::array::from_fn(|_| {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        x as u8
    })))
}

/// `av_pixelutils_get_sad_fn(bits, bits, aligned, NULL)`.
fn c_sad_fn(
    bits: c_int,
    aligned: c_int,
) -> Option<unsafe extern "C" fn(*const u8, isize, *const u8, isize) -> c_int> {
    // SAFETY: integers and a NULL log context, which C only reads without
    // pixelutils.
    unsafe { ffi::av_pixelutils_get_sad_fn(bits, bits, aligned, core::ptr::null_mut()) }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn selection_matches_c_for_every_size() {
    for (a, c) in ALIGNMENTS {
        for bits in 0..=8 {
            let rust = SadFn::new(bits as u32, a);
            assert_eq!(rust.is_some(), c_sad_fn(bits, c).is_some(), "{bits} {a:?}");
            if let Some(f) = rust {
                assert_eq!((f.size(), f.alignment()), (1 << bits, a));
            }
        }
        for bits in [c_int::MAX as u32, c_int::MAX as u32 + 1, u32::MAX] {
            assert!(SadFn::new(bits, a).is_none());
        }
        assert!(c_sad_fn(c_int::MAX, c).is_none());
        // C rejects a non-square size, which `SadFn::new` cannot ask for.
        // SAFETY: as in `c_sad_fn`.
        let rect = unsafe { ffi::av_pixelutils_get_sad_fn(3, 4, c, core::ptr::null_mut()) };
        assert!(rect.is_none());
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn selected_functions_sum_as_c_s_do() {
    let (a, b) = (noise(41), noise(43));
    for (al, c) in ALIGNMENTS {
        for bits in 1..=5 {
            let rust = SadFn::new(bits as u32, al).unwrap();
            let c_fn = c_sad_fn(bits, c).unwrap();
            let n = 1usize << bits;
            // A sequence of block pairs at aligned rows, strides positive,
            // negative and overlapping.
            for (step, (s1, s2)) in [(64isize, 64isize), (128, 32), (-64, 96), (32, -32), (0, 64)]
                .into_iter()
                .enumerate()
            {
                let need = |s: isize| (n - 1) * s.unsigned_abs() + n;
                let (o1, o2) = (64 * step, 128 * step);
                let (l1, l2) = (&a.0[o1..o1 + need(s1)], &b.0[o2..o2 + need(s2)]);
                let sum = rust.call(l1, s1, l2, s2).unwrap();
                let row0 = |s: isize| if s < 0 { (n - 1) * s.unsigned_abs() } else { 0 };
                // SAFETY: both blocks lie inside the slices from their row 0,
                // whose rows are 64-byte aligned (aligned buffers, offsets
                // and strides multiples of 32 at least the block size or 0).
                let (b1, b2) = (unsafe { l1.as_ptr().add(row0(s1)) }, unsafe {
                    l2.as_ptr().add(row0(s2))
                });
                // SAFETY: as above.
                let c_sum = unsafe { c_fn(b1, s1, b2, s2) };
                assert_eq!(sum, c_sum as u32, "{bits} {al:?} {s1} {s2}");
            }
        }
    }
}
