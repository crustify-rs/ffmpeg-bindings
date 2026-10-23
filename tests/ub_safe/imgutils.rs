//! UB tests for `libavutil/imgutils.h`'s safe size check and plane copies:
//! exactly sized heap planes (so a row past either end is an `ASan` report),
//! bottom-up and overlapping-row layouts, empty and degenerate planes,
//! oversized arguments, the uncacheable copy's SSE4 path with aligned,
//! misaligned and short planes (on which C itself faults), and copies on
//! several threads at once.
#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "test inputs are chosen within range, and the casts mirror the C conversions under comparison"
)]
#![expect(
    clippy::too_many_lines,
    reason = "each test is one scenario, its helpers and steps kept where they are used"
)]

use std::thread;

use ffmpeg_bindings::libavutil::libavutil::imgutils::{
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
/// moving, the plane is placed in slack (still sound, less sharp for `ASan`).
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
#[cfg_attr(miri, ignore = "calls C")]
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
#[cfg_attr(miri, ignore = "calls C")]
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
#[cfg_attr(miri, ignore = "calls C")]
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
#[cfg_attr(miri, ignore = "calls C")]
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

/// UB tests for the image layout, allocation, copy and fill wrappers: every
/// pixel format's planes in exactly sized heap allocations (so a row past
/// either end of a plane is an `ASan` report), one byte short, bottom-up and
/// misaligned; the inputs on which C overflows `int`, divides by zero,
/// indexes past its arrays or reads past its descriptor table (each refused
/// before C, so no `UBSan` report); and images allocated, filled, copied and
/// dropped on several threads.
mod images {
    use std::thread;

    use ffmpeg_bindings::libavutil::libavutil::ffi::AVPixelFormat;
    use ffmpeg_bindings::libavutil::libavutil::imgutils::{
        Image, ImageCopyError, PlaneCopyError, Planes, PlanesMut, buffer_size,
        buffer_size_for_linesizes, check_fill_black, check_fill_color, check_sar,
        check_size_max_pixels, copy, copy_from_mut, copy_from_uncacheable, copy_to_buffer,
        fill_arrays, fill_arrays_mut, fill_black, fill_color, fill_linesizes, fill_plane_sizes,
        fill_pointers, fill_pointers_mut, linesize,
    };
    use ffmpeg_bindings::libavutil::libavutil::pixfmt::{ColorRange, PixelFormat};
    use ffmpeg_bindings::libavutil::libavutil::rational::Rational;

    use super::{placed, plane, span};

    const EINVAL: i32 = -22;

    /// Every pixel format these bindings know, by value.
    fn formats() -> impl Iterator<Item = PixelFormat> {
        (0..AVPixelFormat::AV_PIX_FMT_NB.0).map(|raw| PixelFormat::from_raw(AVPixelFormat(raw)))
    }

    /// The formats an every-format sweep runs over: all of them, or, under
    /// `BorrowSanitizer` (whose cost per retag and allocation turns such a
    /// sweep into hours), one of each layout class the image routines
    /// distinguish: packed and planar YUV, chroma subsampled both ways,
    /// semi-planar, RGB in both byte orders, a palette, a bitstream, float,
    /// 16-bit and alpha planes, Bayer, XYZ and a hardware format.
    fn sweep_formats() -> Vec<PixelFormat> {
        if std::env::var("CRUSTIFY_FFMPEG_VARIANT").as_deref() == Ok("bsan") {
            vec![
                PixelFormat::YUV420P,
                PixelFormat::YUYV422,
                PixelFormat::RGB24,
                PixelFormat::YUV410P,
                PixelFormat::GRAY8,
                PixelFormat::MONOWHITE,
                PixelFormat::PAL8,
                PixelFormat::UYVY422,
                PixelFormat::NV12,
                PixelFormat::RGB48BE,
                PixelFormat::YUVA444P16LE,
                PixelFormat::GBRAPF32LE,
                PixelFormat::BAYER_RGGB16BE,
                PixelFormat::XYZ12LE,
                PixelFormat::P010LE,
                PixelFormat::VUYA,
                PixelFormat::GBRP,
                PixelFormat::VAAPI,
            ]
        } else {
            formats().collect()
        }
    }

    /// Each plane's bytes a `w x h` copy of `format` touches, `(row width,
    /// rows)`, as the wrappers document; `None` past the format's planes.
    fn extents(format: PixelFormat, w: i32, h: i32) -> [Option<(usize, usize)>; 4] {
        let (_, v) = format.chroma_subsampling().unwrap();
        let planes = format.plane_count().unwrap();
        core::array::from_fn(|i| {
            let width = linesize(format, w, i).ok()? as usize;
            let rows = if i == 1 || i == 2 {
                (h as usize).div_ceil(1 << v)
            } else {
                h as usize
            };
            (i < planes && width > 0).then_some((width, rows))
        })
    }

    /// Exactly sized heap planes, rows `pad` bytes wider than their extent
    /// apart (negated for `bottom_up`), for a `w x h` image of `format`; the
    /// palette of a palettized format in plane 1.
    fn exact_planes(
        format: PixelFormat,
        w: i32,
        h: i32,
        pad: usize,
        bottom_up: bool,
        seed: u8,
    ) -> ([Vec<u8>; 4], [isize; 4]) {
        let ext = extents(format, w, h);
        let mut linesizes = [0isize; 4];
        let bufs = core::array::from_fn(|i| match ext[i] {
            Some((width, rows)) => {
                let ls = (width + pad) as isize;
                linesizes[i] = if bottom_up { -ls } else { ls };
                plane(span(ls, width, rows), seed.wrapping_add(i as u8))
            }
            None if i == 1 && format == PixelFormat::PAL8 => plane(1024, seed),
            None => Vec::new(),
        });
        (bufs, linesizes)
    }

    fn view(bufs: &[Vec<u8>; 4], linesizes: [isize; 4]) -> Planes<'_> {
        Planes {
            data: bufs.each_ref().map(|b| &b[..]),
            linesizes,
        }
    }

    fn view_mut(bufs: &mut [Vec<u8>; 4], linesizes: [isize; 4]) -> PlanesMut<'_> {
        PlanesMut {
            data: bufs.each_mut().map(|b| &mut b[..]),
            linesizes,
        }
    }

    /// Whether the wrappers can copy `format` at all (C copies nothing of
    /// hardware formats).
    fn copyable(format: PixelFormat) -> bool {
        format.plane_count().is_some_and(|n| n > 0)
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn exact_planes_of_every_format_copy_and_fill() {
        for format in sweep_formats().into_iter().filter(|&f| copyable(f)) {
            for (w, h) in [(1, 1), (7, 5), (16, 3)] {
                for (pad, bottom_up) in [(0, false), (3, false), (0, true), (5, true)] {
                    let (src, sl) = exact_planes(format, w, h, pad, bottom_up, 1);
                    let (mut dst, dl) = exact_planes(format, w, h, pad, !bottom_up, 77);
                    let what = format!("{format:?} {w}x{h} {pad} {bottom_up}");
                    copy(&mut view_mut(&mut dst, dl), view(&src, sl), format, w, h)
                        .unwrap_or_else(|e| panic!("{what}: {e}"));
                    copy_from_uncacheable(
                        &mut view_mut(&mut dst, dl),
                        view(&src, sl),
                        format,
                        w,
                        h,
                    )
                    .unwrap_or_else(|e| panic!("{what}: {e}"));
                    let (mut other, ol) = exact_planes(format, w, h, pad, bottom_up, 9);
                    copy_from_mut(
                        &mut view_mut(&mut other, ol),
                        &view_mut(&mut dst, dl),
                        format,
                        w,
                        h,
                    )
                    .unwrap_or_else(|e| panic!("{what}: {e}"));
                    let _ = fill_color(&mut view_mut(&mut dst, dl), format, [!0; 4], w, h);
                    let _ = fill_black(&mut view_mut(&mut dst, dl), format, ColorRange::MPEG, w, h);
                    for align in [1, 8, 64] {
                        let size = buffer_size(format, w, h, align).unwrap();
                        let mut out = plane(size, 3);
                        let n = copy_to_buffer(&mut out, view(&src, sl), format, w, h, align);
                        assert_eq!(n, Ok(size), "{what} {align}");
                        let mut short = plane(size - 1, 3);
                        let before = short.clone();
                        assert_eq!(
                            copy_to_buffer(&mut short, view(&src, sl), format, w, h, align),
                            Err(EINVAL)
                        );
                        assert_eq!(short, before);
                    }
                }
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn one_byte_short_planes_are_refused_untouched() {
        for format in [
            PixelFormat::YUV420P,
            PixelFormat::NV12,
            PixelFormat::RGB24,
            PixelFormat::PAL8,
            PixelFormat::YUVA444P16LE,
            PixelFormat::MONOBLACK,
        ] {
            let (w, h) = (9, 7);
            for i in 0..4 {
                let (src, sl) = exact_planes(format, w, h, 2, i % 2 == 1, 1);
                let (mut dst, dl) = exact_planes(format, w, h, 2, false, 50);
                if dst[i].is_empty() {
                    continue;
                }
                dst[i].pop();
                dst[i].shrink_to_fit();
                let before = dst.clone();
                let err =
                    copy(&mut view_mut(&mut dst, dl), view(&src, sl), format, w, h).unwrap_err();
                assert_eq!(
                    err,
                    ImageCopyError::Plane {
                        index: i,
                        error: PlaneCopyError::DestinationTooShort
                    },
                    "{format:?} {i}"
                );
                let err = copy_from_uncacheable(
                    &mut view_mut(&mut dst, dl),
                    view(&src, sl),
                    format,
                    w,
                    h,
                );
                assert!(err.is_err());
                let palette = format == PixelFormat::PAL8 && i == 1;
                if !palette {
                    assert_eq!(
                        fill_color(&mut view_mut(&mut dst, dl), format, [0; 4], w, h),
                        Err(EINVAL)
                    );
                    assert_eq!(
                        fill_black(&mut view_mut(&mut dst, dl), format, ColorRange::JPEG, w, h),
                        Err(EINVAL)
                    );
                }
                assert_eq!(dst, before, "{format:?} {i}");
                // A short source plane.
                let (mut src, sl) = (src, sl);
                let (mut dst, dl) = exact_planes(format, w, h, 2, false, 50);
                src[i].pop();
                src[i].shrink_to_fit();
                let before = dst.clone();
                let err =
                    copy(&mut view_mut(&mut dst, dl), view(&src, sl), format, w, h).unwrap_err();
                assert_eq!(
                    err,
                    ImageCopyError::Plane {
                        index: i,
                        error: PlaneCopyError::SourceTooShort
                    }
                );
                assert_eq!(dst, before);
                let mut out = vec![0u8; buffer_size(format, w, h, 1).unwrap()];
                assert_eq!(
                    copy_to_buffer(&mut out, view(&src, sl), format, w, h, 1),
                    Err(EINVAL)
                );
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn rows_wider_than_a_linesize_are_refused_instead_of_aborting() {
        let format = PixelFormat::GRAY8;
        let src = plane(64, 1);
        let mut dst = plane(64, 2);
        let narrow = Planes {
            data: [&src, &[], &[], &[]],
            linesizes: [4, 0, 0, 0],
        };
        let mut planes = PlanesMut {
            data: [&mut dst, &mut [], &mut [], &mut []],
            linesizes: [8, 0, 0, 0],
        };
        let wide = ImageCopyError::Plane {
            index: 0,
            error: PlaneCopyError::RowTooWide,
        };
        assert_eq!(copy(&mut planes, narrow, format, 6, 3), Err(wide));
        assert_eq!(copy(&mut planes, narrow, format, 6, 0), Err(wide));
        assert_eq!(
            copy_from_uncacheable(&mut planes, narrow, format, 6, 3),
            Err(wide)
        );
        // Fills take overlapping rows inside the plane.
        assert_eq!(fill_color(&mut planes, format, [9; 4], 16, 3), Ok(()));
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn inputs_c_mishandles_are_refused_before_c() {
        let i32_max = i32::MAX;
        // `int` overflow rounding a bitstream row up to bytes.
        for (f, w) in [
            (PixelFormat::MONOBLACK, i32_max),
            (PixelFormat::MONOWHITE, i32_max - 6),
            (PixelFormat::BGR4, 536_870_911),
            (PixelFormat::RGB4, 536_870_911),
        ] {
            assert_eq!(linesize(f, w, 0), Err(EINVAL));
            assert_eq!(fill_linesizes(f, w), Err(EINVAL));
            assert_eq!(check_size_max_pixels(w as u32, 1, i64::MAX, f), Err(EINVAL));
            assert_eq!(check_fill_color(f, [0; 4], w), Err(EINVAL));
            assert_eq!(check_fill_black(f, ColorRange::MPEG, w), Err(EINVAL));
            let mut buf = [0u8; 16];
            let mut planes = PlanesMut {
                data: [&mut buf, &mut [], &mut [], &mut []],
                linesizes: [16, 0, 0, 0],
            };
            assert_eq!(fill_color(&mut planes, f, [0; 4], w, 1), Err(EINVAL));
            assert_eq!(
                fill_black(&mut planes, f, ColorRange::MPEG, w, 1),
                Err(EINVAL)
            );
            let src = [0u8; 16];
            let src = Planes {
                data: [&src, &[], &[], &[]],
                linesizes: [16, 0, 0, 0],
            };
            assert_eq!(
                copy(&mut planes, src, f, w, 1),
                Err(ImageCopyError::InvalidSize)
            );
        }
        // Planes past C's arrays.
        assert_eq!(linesize(PixelFormat::YUV420P, 8, 4), Err(EINVAL));
        assert_eq!(linesize(PixelFormat::YUV420P, 8, usize::MAX), Err(EINVAL));
        // Heights C divides by.
        for h in [0, -1, -2, i32::MIN] {
            assert_eq!(
                fill_plane_sizes(PixelFormat::YUV420P, h, [1; 4]),
                Err(EINVAL)
            );
            assert_eq!(
                buffer_size_for_linesizes(PixelFormat::YUV420P, h, [1; 4]),
                Err(EINVAL)
            );
            assert_eq!(
                fill_pointers(&[0; 8], PixelFormat::YUV420P, h, [1; 4]).unwrap_err(),
                EINVAL
            );
        }
        // Alignments C's rounding overflows or ignores.
        for align in [0, 3, 6, 1 << 31, usize::MAX] {
            assert_eq!(buffer_size(PixelFormat::YUV420P, 8, 8, align), Err(EINVAL));
            assert_eq!(
                Image::alloc(8, 8, PixelFormat::YUV420P, align).unwrap_err(),
                EINVAL
            );
            assert_eq!(
                fill_arrays(&[0; 256], PixelFormat::YUV420P, 8, 8, align).unwrap_err(),
                EINVAL
            );
            let mut out = [0u8; 256];
            let src = Image::alloc(8, 8, PixelFormat::YUV420P, 1).unwrap();
            assert_eq!(
                copy_to_buffer(&mut out, src.planes(), PixelFormat::YUV420P, 8, 8, align),
                Err(EINVAL)
            );
        }
        // Heights whose chroma rounding overflows in C.
        let mut img = Image::alloc(8, 8, PixelFormat::YUV420P, 1).unwrap();
        assert_eq!(
            fill_color(
                &mut img.planes_mut(),
                PixelFormat::YUV420P,
                [1; 4],
                8,
                i32::MIN
            ),
            Ok(())
        );
        assert_eq!(
            fill_black(
                &mut img.planes_mut(),
                PixelFormat::YUV420P,
                ColorRange::MPEG,
                8,
                -1
            ),
            Ok(())
        );
        assert!(img.planes().data.iter().all(|p| p.iter().all(|&b| b == 0)));
        let src = Image::alloc(8, 8, PixelFormat::YUV420P, 1).unwrap();
        for (w, h) in [(-1, 8), (8, -1), (i32::MIN, i32::MIN)] {
            assert_eq!(
                copy(
                    &mut img.planes_mut(),
                    src.planes(),
                    PixelFormat::YUV420P,
                    w,
                    h
                ),
                Err(ImageCopyError::InvalidSize)
            );
        }
        // Formats past the descriptor table, and unknown or hardware ones.
        let nb = AVPixelFormat::AV_PIX_FMT_NB;
        for f in [
            PixelFormat::from_raw(nb),
            PixelFormat::from_raw(AVPixelFormat(i32::MAX)),
            PixelFormat::NONE,
            PixelFormat::VAAPI,
        ] {
            let mut buf = [0u8; 64];
            assert_eq!(
                f.describe(&mut buf).is_none(),
                f != PixelFormat::NONE && f != PixelFormat::VAAPI
            );
            assert!(fill_linesizes(f, 8).is_err());
            assert!(Image::alloc(8, 8, f, 1).is_err());
            assert_eq!(
                copy(&mut img.planes_mut(), src.planes(), f, 8, 8),
                Err(ImageCopyError::UnsupportedFormat)
            );
            assert!(fill_color(&mut img.planes_mut(), f, [0; 4], 8, 8).is_err());
        }
        // Extreme sample aspect ratios and sizes.
        for (num, den) in [
            (i32::MIN, 1),
            (1, i32::MIN),
            (i32::MAX, i32::MAX),
            (1, i32::MAX),
        ] {
            let _ = check_sar(u32::MAX, u32::MAX, Rational::new(num, den));
        }
        assert_eq!(
            Image::alloc(i32::MAX, i32::MAX, PixelFormat::RGBAF32LE, 64).unwrap_err(),
            EINVAL
        );
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn layouts_hand_out_disjoint_exact_planes() {
        for format in formats().filter(|&f| copyable(f)) {
            let Ok(size) = buffer_size(format, 13, 7, 4) else {
                continue;
            };
            let mut buf = plane(size, 0);
            let planes = fill_arrays_mut(&mut buf, format, 13, 7, 4).unwrap();
            let total: usize = planes.data.iter().map(|d| d.len()).sum();
            assert_eq!(total, size, "{format:?}");
            for (i, d) in planes.data.into_iter().enumerate() {
                d.fill(i as u8 + 1);
            }
            let read = fill_arrays(&buf, format, 13, 7, 4).unwrap();
            for (i, d) in read.data.iter().enumerate() {
                assert!(d.iter().all(|&b| b == i as u8 + 1));
            }
            let ls = fill_linesizes(format, 13).unwrap();
            let size = buffer_size_for_linesizes(format, 7, ls).unwrap();
            let mut buf = plane(size, 0);
            let planes = fill_pointers_mut(&mut buf, format, 7, ls).unwrap();
            for d in planes.data {
                d.fill(0xEE);
            }
            assert!(fill_pointers(&buf[..size - 1], format, 7, ls).is_err());
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn misaligned_uncacheable_images_never_fault_or_overrun() {
        let format = PixelFormat::YUV420P;
        let (w, h) = (100, 6);
        let mut src = Image::alloc(w, h, format, 64).unwrap();
        for d in src.planes_mut().data {
            d.fill(0x42);
        }
        for (off, short) in [(0, false), (1, false), (8, false), (0, true), (17, true)] {
            let ls = [128isize, 64, 64, 0];
            let ext = [(100usize, 6usize), (50, 3), (50, 3)];
            let mut bufs: Vec<(Box<[u8]>, usize)> = ext
                .iter()
                .zip(ls)
                .map(|(&(bw, rows), l)| {
                    let width = if short { bw } else { bw.next_multiple_of(64) };
                    placed(span(l, width, rows), off, 7)
                })
                .collect();
            let mut it = bufs.iter_mut();
            let mut slot = || {
                let (b, at) = it.next().unwrap();
                &mut b[*at..]
            };
            let mut planes = PlanesMut {
                data: [slot(), slot(), slot(), &mut []],
                linesizes: ls,
            };
            copy_from_uncacheable(&mut planes, src.planes(), format, w, h).unwrap();
            for (i, &(bw, rows)) in ext.iter().enumerate() {
                for r in 0..rows {
                    let at = r * ls[i] as usize;
                    assert!(planes.data[i][at..at + bw].iter().all(|&b| b == 0x42));
                }
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn images_live_and_die_on_many_threads() {
        let format = PixelFormat::YUV420P;
        let mut src = Image::alloc(40, 30, format, 32).unwrap();
        fill_color(&mut src.planes_mut(), format, [10, 20, 30, 0], 40, 30).unwrap();
        let shared = src.planes();
        let made: Vec<Image> = thread::scope(|s| {
            let handles: Vec<_> = (0..8)
                .map(|t| {
                    s.spawn(move || {
                        let mut img = Image::alloc(40, 30, format, 1 << (t % 7)).unwrap();
                        copy(&mut img.planes_mut(), shared, format, 40, 30).unwrap();
                        copy_from_uncacheable(&mut img.planes_mut(), shared, format, 40, 30)
                            .unwrap();
                        fill_black(&mut img.planes_mut(), format, ColorRange::JPEG, 20, 15)
                            .unwrap();
                        let mut out = vec![0u8; buffer_size(format, 40, 30, 4).unwrap()];
                        copy_to_buffer(&mut out, img.planes(), format, 40, 30, 4).unwrap();
                        img
                    })
                })
                .collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });
        // Dropped on other threads than they were made on.
        thread::scope(|s| {
            for img in made {
                s.spawn(move || {
                    assert_eq!(img.planes().data[0][39], 10);
                    drop(img);
                });
            }
        });
    }
}

/// `max_pixel_steps`: C's table, and Rust-built descriptors with every slot's
/// plane at the edges of C's four-plane arrays, which live on the wrapper's
/// stack (so an index past them is an `ASan` report).
mod pixel_steps {
    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::imgutils::max_pixel_steps;
    use ffmpeg_bindings::libavutil::libavutil::pixdesc::{
        ComponentDescriptor, InvalidDescriptor, PixelFormatDescriptor,
    };
    use ffmpeg_bindings::libavutil::libavutil::pixfmt::PixelFormat;

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn every_table_entry_has_steps() {
        for raw in 0..ffi::AVPixelFormat::AV_PIX_FMT_NB.0 {
            let d = PixelFormat::from_raw(ffi::AVPixelFormat(raw))
                .descriptor()
                .unwrap();
            let s = max_pixel_steps(d).unwrap();
            assert!(s.components.iter().all(|&c| c < 4));
            assert!(s.steps.iter().all(|&st| st >= 0));
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn edge_planes_are_refused_and_the_rest_accepted() {
        let planes = [i32::MIN, -1, 0, 1, 2, 3, 4, 5, i32::MAX];
        for (slot, plane) in (0..4).flat_map(|s| planes.map(|p| (s, p))) {
            for step in [i32::MIN, -1, 0, 1, i32::MAX] {
                let mut d = PixelFormatDescriptor::new();
                d.as_mut().components_mut().get_mut(slot).unwrap().set(
                    ComponentDescriptor::from_raw(ffi::AVComponentDescriptor {
                        plane,
                        step,
                        offset: 0,
                        shift: 0,
                        depth: 0,
                    }),
                );
                let r = max_pixel_steps(d.as_ref());
                if (0..4).contains(&plane) {
                    let s = r.unwrap();
                    assert_eq!(s.steps[plane as usize], step.max(0));
                } else {
                    assert_eq!(r, Err(InvalidDescriptor));
                }
            }
        }
    }
}

/// UB tests of the size checks with a parent log context through the safe
/// API: a `BareObject`'s `ObjectRef`, with level offsets up to and past the
/// overflow bound, on valid and invalid sizes and formats, on several
/// threads.
mod with_context {
    use core::ffi::c_int;
    use std::thread;

    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::imgutils::{
        check_size_max_pixels_with_context, check_size_with_context,
    };
    use ffmpeg_bindings::libavutil::libavutil::log::{Class, ClassCategory};
    use ffmpeg_bindings::libavutil::libavutil::opt::{BareObject, ObjectRef};
    use ffmpeg_bindings::libavutil::libavutil::pixfmt::PixelFormat;

    const EINVAL: c_int = -22;

    static CLASS: Class = Class::new(c"ub-safe-imgutils-parent", ClassCategory::NA);

    /// The checks, in `ctx`'s context or none.
    fn checks(ctx: Option<ObjectRef<'_>>) {
        let max = c_int::MAX - ffi::AV_LOG_ERROR as c_int;
        for offset in [c_int::MIN, -8, 0, 8, max] {
            assert_eq!(check_size_with_context(64, 48, offset, ctx), Ok(()));
            assert_eq!(check_size_with_context(0, 48, offset, ctx), Err(EINVAL));
            assert_eq!(
                check_size_with_context(u32::MAX, u32::MAX, offset, ctx),
                Err(EINVAL)
            );
            for f in [
                PixelFormat::YUV420P,
                PixelFormat::MONOBLACK,
                PixelFormat::NONE,
            ] {
                let check = |w, h, px| check_size_max_pixels_with_context(w, h, px, f, offset, ctx);
                assert_eq!(check(64, 48, 64 * 48), Ok(()));
                assert_eq!(check(64, 48, 64 * 48 - 1), Err(EINVAL));
                assert_eq!(check(i32::MAX as u32, 1, i64::MAX), Err(EINVAL));
            }
        }
        for offset in [max + 1, c_int::MAX] {
            // Rejected before C runs.
            assert_eq!(check_size_with_context(0, 0, offset, ctx), Err(EINVAL));
            let f = PixelFormat::YUV420P;
            assert_eq!(
                check_size_max_pixels_with_context(0, 0, 1, f, offset, ctx),
                Err(EINVAL)
            );
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn no_parent_and_a_bare_object_parent() {
        checks(None);
        let parent = BareObject::new(CLASS.as_ref()).unwrap();
        checks(Some(parent.as_object()));
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn one_parent_shared_across_threads() {
        let parent = BareObject::new(CLASS.as_ref()).unwrap();
        thread::scope(|s| {
            for _ in 0..4 {
                let parent = &parent;
                s.spawn(move || checks(Some(parent.as_object())));
            }
        });
    }
}
