//! UB tests for `libavutil/pixelutils.h`'s [`SadFn::new`], through the safe
//! API only: every block size and alignment C is asked for, the functions it
//! returns called at the edges of their blocks, and selection from several
//! threads at once.

use std::thread;

use ffmpeg::libavutil::pixelutils::{SadAlignment, SadError, SadFn};

const ALIGNMENTS: [SadAlignment; 3] = [
    SadAlignment::Unaligned,
    SadAlignment::First,
    SadAlignment::Both,
];

/// Block sizes C has functions for, those around them, and the extremes.
fn sizes() -> impl Iterator<Item = u32> {
    (0..=9).chain([
        31,
        32,
        63,
        64,
        255,
        256,
        i32::MAX as u32,
        i32::MAX as u32 + 1,
        u32::MAX,
    ])
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn every_size_selects_or_refuses() {
    for a in ALIGNMENTS {
        for bits in sizes() {
            match SadFn::new(bits, a) {
                Some(f) => {
                    assert!((1..=5).contains(&bits));
                    assert_eq!((f.size(), f.alignment()), (1 << bits, a));
                }
                None => assert!(!(1..=5).contains(&bits), "{bits}"),
            }
        }
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn selected_functions_read_only_their_blocks() {
    for a in ALIGNMENTS {
        for bits in 1..=5 {
            let f = SadFn::new(bits, a).unwrap();
            let n = f.size();
            // Exactly the bytes a block needs, on a heap block of its own,
            // at an address a multiple of 64 or not.
            for stride in [n as isize, -(n as isize), 0, 3 * n as isize] {
                let need = (n - 1) * stride.unsigned_abs() + n;
                let (x, y) = (vec![3u8; need], vec![5u8; need]);
                match f.call(&x, stride, &y, stride) {
                    Ok(sum) => assert_eq!(sum, (2 * n * n) as u32),
                    Err(e) => assert!(matches!(
                        e,
                        SadError::FirstMisaligned | SadError::SecondMisaligned
                    )),
                }
                assert_eq!(
                    f.call(&x[..need - 1], stride, &y, stride),
                    Err(SadError::FirstTooShort)
                );
            }
        }
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn selection_from_threads_at_once() {
    let expect: Vec<Option<usize>> = sizes()
        .map(|b| SadFn::new(b, SadAlignment::Both).map(|f| f.size()))
        .collect();
    thread::scope(|s| {
        for t in 0..8 {
            let expect = &expect;
            s.spawn(move || {
                let a = ALIGNMENTS[t % 3];
                let got: Vec<Option<usize>> = sizes()
                    .map(|b| SadFn::new(b, a).map(|f| f.size()))
                    .collect();
                assert_eq!(&got, expect);
                let f = SadFn::new(4, SadAlignment::Unaligned).unwrap();
                let block = vec![t as u8; 16 * 16];
                assert_eq!(f.call(&block, 16, &block, 16), Ok(0));
            });
        }
    });
}
