//! UB tests for `libavutil/imgutils.h`'s safe size check and plane copies:
//! exactly sized heap planes (so a row past either end is an ASan report),
//! bottom-up and overlapping-row layouts, empty and degenerate planes,
//! oversized arguments, the uncacheable copy's SSE4 path with aligned,
//! misaligned and short planes (on which C itself faults), and copies on
//! several threads at once.

use std::thread;

use ffmpeg::libavutil::imgutils::{
    PlaneCopyError, check_size, copy_plane, copy_plane_from_uncacheable,
};

/// A heap buffer of exactly `len` bytes, filled from `seed`.
fn plane(len: usize, seed: u8) -> Vec<u8> {
    let mut v = Vec::with_capacity(len);
    v.extend((0..len).map(|i| (i as u8).wrapping_mul(13).wrapping_add(seed)));
    v.shrink_to_fit();
    v
}

/// The bytes a plane of `h` rows of `w` bytes, `linesize` apart, spans.
fn span(linesize: isize, w: usize, h: usize) -> usize {
    if h == 0 {
        0
    } else {
        (h - 1) * linesize.unsigned_abs() + w
    }
}

/// A heap allocation ending exactly at the plane's end, where the plane of
/// `len` bytes starts `offset` past a 64-byte boundary: the buffer and the
/// plane's start. The allocator's placement is found by trial; should it keep
/// moving, the plane is placed in slack (still sound, less sharp for ASan).
fn placed(len: usize, offset: usize, seed: u8) -> (Box<[u8]>, usize) {
    let fill = |pad: usize| {
        let mut v = vec![0u8; pad + len].into_boxed_slice();
        for (i, b) in v[pad..].iter_mut().enumerate() {
            *b = (i as u8).wrapping_mul(7).wrapping_add(seed);
        }
        v
    };
    let mut pad = offset;
    for _ in 0..64 {
        let v = fill(pad);
        let misplaced = (v.as_ptr() as usize + pad) % 64;
        if misplaced == offset % 64 {
            return (v, pad);
        }
        pad = (pad + 64 + offset % 64 - misplaced) % 64;
    }
    let v = fill(128 + offset);
    let start = v.as_ptr().align_offset(64) + offset;
    (v, start)
}

/// Row `r` of a plane laid out as `copy_plane` documents.
fn row(buf: &[u8], linesize: isize, w: usize, h: usize, r: usize) -> &[u8] {
    let origin = if linesize < 0 {
        (h - 1) * linesize.unsigned_abs()
    } else {
        0
    };
    let at = origin.wrapping_add_signed(r as isize * linesize);
    &buf[at..at + w]
}

#[test]
fn check_size_accepts_and_rejects_edge_dimensions() {
    assert_eq!(check_size(1, 1), Ok(()));
    assert_eq!(check_size(1920, 1080), Ok(()));
    for (w, h) in [
        (0, 1),
        (1, 0),
        (0, 0),
        (u32::MAX, 1),
        (1, u32::MAX),
        (i32::MAX as u32 + 1, 1),
        (65536, 65536),
    ] {
        assert_eq!(check_size(w, h), Err(-22), "{w}x{h}");
    }
}

#[test]
fn exact_planes_copy_in_every_orientation() {
    for (dl, sl, w, h) in [
        (8i32, 8i32, 8usize, 5usize),
        (12, 9, 7, 6),
        (-12, 9, 7, 6),
        (12, -9, 7, 6),
        (-9, -12, 9, 3),
        (5, 5, 5, 1),
        (1, 1, 1, 64),
    ] {
        let (dlen, slen) = (span(dl as isize, w, h), span(sl as isize, w, h));
        let src = plane(slen, 1);
        let mut dst = plane(dlen, 200);
        copy_plane(&mut dst, dl, &src, sl, w, h).unwrap();
        for r in 0..h {
            assert_eq!(
                row(&dst, dl as isize, w, h, r),
                row(&src, sl as isize, w, h, r)
            );
        }
        let mut dst = plane(dlen, 200);
        copy_plane_from_uncacheable(&mut dst, dl as isize, &src, sl as isize, w, h).unwrap();
        for r in 0..h {
            assert_eq!(
                row(&dst, dl as isize, w, h, r),
                row(&src, sl as isize, w, h, r)
            );
        }
    }
}

#[test]
fn one_byte_short_planes_are_refused_untouched() {
    for (dl, sl, w, h) in [(8i32, 8i32, 8usize, 5usize), (-12, 9, 7, 6), (12, -9, 7, 6)] {
        let (dlen, slen) = (span(dl as isize, w, h), span(sl as isize, w, h));
        let src = plane(slen, 1);
        let mut short = plane(dlen - 1, 9);
        let before = short.clone();
        assert_eq!(
            copy_plane(&mut short, dl, &src, sl, w, h),
            Err(PlaneCopyError::DestinationTooShort)
        );
        assert_eq!(
            copy_plane_from_uncacheable(&mut short, dl as isize, &src, sl as isize, w, h),
            Err(PlaneCopyError::DestinationTooShort)
        );
        assert_eq!(short, before);
        let mut dst = plane(dlen, 9);
        assert_eq!(
            copy_plane(&mut dst, dl, &src[..slen - 1], sl, w, h),
            Err(PlaneCopyError::SourceTooShort)
        );
        assert_eq!(
            copy_plane_from_uncacheable(&mut dst, dl as isize, &src[..slen - 1], sl as isize, w, h),
            Err(PlaneCopyError::SourceTooShort)
        );
    }
}

#[test]
fn rows_wider_than_a_linesize_are_refused_instead_of_aborting() {
    let src = plane(64, 1);
    let mut dst = plane(64, 2);
    for h in [0, 1, 3] {
        assert_eq!(
            copy_plane(&mut dst, 4, &src, 8, 5, h),
            Err(PlaneCopyError::RowTooWide)
        );
        assert_eq!(
            copy_plane(&mut dst, 8, &src, -4, 5, h),
            Err(PlaneCopyError::RowTooWide)
        );
        assert_eq!(
            copy_plane(&mut dst, 0, &src, 0, 1, h),
            Err(PlaneCopyError::RowTooWide)
        );
        assert_eq!(
            copy_plane_from_uncacheable(&mut dst, 4, &src, 8, 5, h),
            Err(PlaneCopyError::RowTooWide)
        );
    }
}

#[test]
fn empty_and_degenerate_planes_copy_nothing() {
    let mut empty: Vec<u8> = Vec::new();
    assert_eq!(copy_plane(&mut empty, 0, &[], 0, 0, 0), Ok(()));
    assert_eq!(copy_plane(&mut empty, 0, &[], 0, 0, 1000), Ok(()));
    assert_eq!(copy_plane(&mut empty, 16, &[], -16, 0, 1), Ok(()));
    assert_eq!(copy_plane(&mut empty, 16, &[], 16, 8, 0), Ok(()));
    assert_eq!(
        copy_plane(&mut empty, i32::MIN, &[], i32::MAX, 0, 1),
        Ok(())
    );
    assert_eq!(
        copy_plane_from_uncacheable(&mut empty, 0, &[], 0, 0, 0),
        Ok(())
    );
    // Zero-width rows still lie `linesize` apart inside the slice.
    assert_eq!(
        copy_plane_from_uncacheable(&mut empty, 64, &[], 64, 0, 7),
        Err(PlaneCopyError::DestinationTooShort)
    );
    assert_eq!(
        copy_plane(&mut empty, 64, &[], 64, 0, 7),
        Err(PlaneCopyError::DestinationTooShort)
    );
    assert_eq!(
        copy_plane_from_uncacheable(&mut empty, 128, &[], 128, 64, 0),
        Ok(())
    );
    assert_eq!(
        copy_plane_from_uncacheable(&mut empty, isize::MAX, &[], isize::MAX, 0, 1),
        Ok(())
    );
    // Zero-width rows of linesize-0 planes all lie at the start.
    let src = plane(16, 3);
    let mut dst = plane(16, 4);
    assert_eq!(copy_plane(&mut dst, 0, &src, 0, 0, 9), Ok(()));
    assert_eq!(copy_plane(&mut dst, 16, &src, 0, 0, 1), Ok(()));
    assert_eq!(dst, plane(16, 4));
}

#[test]
fn oversized_arguments_are_refused() {
    let mut dst = plane(64, 0);
    let src = plane(64, 1);
    assert_eq!(
        copy_plane(&mut dst, 8, &src, 8, i32::MAX as usize + 1, 1),
        Err(PlaneCopyError::TooLarge)
    );
    assert_eq!(
        copy_plane(&mut dst, 8, &src, 8, 1, usize::MAX),
        Err(PlaneCopyError::TooLarge)
    );
    assert_eq!(
        copy_plane(&mut dst, i32::MAX, &src, i32::MAX, 1, 2),
        Err(PlaneCopyError::DestinationTooShort)
    );
    assert_eq!(
        copy_plane(&mut dst, i32::MIN, &src, i32::MIN, 1, 2),
        Err(PlaneCopyError::DestinationTooShort)
    );
    for (dl, sl) in [(isize::MIN, 8), (8, isize::MIN)] {
        assert_eq!(
            copy_plane_from_uncacheable(&mut dst, dl, &src, sl, 1, 1),
            Err(PlaneCopyError::TooLarge)
        );
    }
    assert_eq!(
        copy_plane_from_uncacheable(&mut dst, 8, &src, 8, 1, usize::MAX),
        Err(PlaneCopyError::TooLarge)
    );
    assert_eq!(
        copy_plane_from_uncacheable(&mut dst, isize::MAX, &src, isize::MAX, usize::MAX, 1),
        Err(PlaneCopyError::TooLarge)
    );
    assert_eq!(
        copy_plane_from_uncacheable(&mut dst, isize::MAX, &src, -isize::MAX, 2, 2),
        Err(PlaneCopyError::DestinationTooShort)
    );
}

/// Planes on which C's uncacheable copy takes its SSE4 path (both linesizes
/// cover the 64-byte-rounded width): aligned ones, and misaligned rows,
/// pointers and short last rows, which fault or overrun in C.
#[test]
fn uncacheable_fast_path_planes_never_fault_or_overrun() {
    for (dl, sl, w, h, doff, soff, dshort, sshort) in [
        (
            64isize, 64isize, 10usize, 4usize, 0usize, 0usize, false, false,
        ),
        (128, 64, 64, 3, 0, 16, false, false),
        (192, 128, 100, 5, 32, 0, false, false),
        (72, 64, 10, 4, 0, 0, false, false),
        (64, 80, 10, 4, 0, 0, false, false),
        (64, 64, 10, 4, 8, 0, false, false),
        (64, 64, 64, 4, 0, 1, false, false),
        (64, 64, 10, 4, 0, 0, true, false),
        (64, 64, 10, 4, 0, 0, false, true),
        (96, 96, 33, 2, 3, 5, true, true),
    ] {
        let wide = w.next_multiple_of(64);
        let dlen = if dshort {
            span(dl, w, h)
        } else {
            span(dl, wide, h)
        };
        let slen = if sshort {
            span(sl, w, h)
        } else {
            span(sl, wide, h)
        };
        let (src, s0) = placed(slen, soff, 1);
        let (mut dst, d0) = placed(dlen, doff, 99);
        let before = dst.clone();
        copy_plane_from_uncacheable(&mut dst[d0..], dl, &src[s0..], sl, w, h).unwrap();
        for r in 0..h {
            let (d, s) = (d0 + r * dl as usize, s0 + r * sl as usize);
            assert_eq!(
                dst[d..d + w],
                src[s..s + w],
                "{dl} {sl} {w} {h} {doff} {soff} row {r}"
            );
            // Past the width, a row keeps its bytes or takes the source's
            // (the SSE4 path copies whole 64-byte blocks); between rows,
            // nothing changes.
            let end = (d + dl as usize).min(dst.len());
            for x in d + w..end {
                let copied = x - d < wide && s + (x - d) < src.len() && dst[x] == src[s + x - d];
                assert!(dst[x] == before[x] || copied, "byte {x}");
            }
        }
    }
}

#[test]
fn planes_copy_on_many_threads() {
    let src = plane(span(48, 40, 32), 5);
    thread::scope(|s| {
        for t in 0..8 {
            let src = &src;
            s.spawn(move || {
                let dl = if t % 2 == 0 { 48 } else { -48 };
                let mut dst = plane(span(48, 40, 32), t);
                copy_plane(&mut dst, dl, src, 48, 40, 32).unwrap();
                let mut uc = plane(span(64, 40, 32), t);
                copy_plane_from_uncacheable(&mut uc, 64, src, 48, 40, 32).unwrap();
                for r in 0..32 {
                    assert_eq!(row(&dst, dl as isize, 40, 32, r), row(src, 48, 40, 32, r));
                    assert_eq!(row(&uc, 64, 40, 32, r), row(src, 48, 40, 32, r));
                }
                assert_eq!(check_size(40, 32), Ok(()));
                assert_eq!(check_size(0, 32), Err(-22));
            });
        }
    });
}
