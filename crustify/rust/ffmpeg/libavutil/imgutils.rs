//! Safe wrappers over `libavutil/imgutils.h`.
//!
//! [`check_size`] validates image dimensions (`av_image_check_size`);
//! [`copy_plane`] and [`copy_plane_from_uncacheable`] copy one image plane
//! between byte slices (`av_image_copy_plane`, `av_image_copy_plane_uc_from`).
//!
//! A plane is `height` rows, `linesize` bytes apart, held in one slice that
//! spans exactly its rows: row 0 starts the slice for a non-negative linesize
//! and is the last row in memory for a negative one (a bottom-up image), so a
//! plane of `height` rows of `bytewidth` bytes needs
//! `(height - 1) * |linesize| + bytewidth` bytes.

use core::ffi::{c_int, c_void};
use core::fmt;

use super::ffi;

/// `AVERROR(EINVAL)`.
const AVERROR_EINVAL: c_int = -(ffi::EINVAL as c_int);

/// Wraps: av_image_check_size
///
/// Checks that a `width` x `height` image is valid, as FFmpeg requires of
/// every image it allocates: both dimensions non-zero and at most `INT_MAX`,
/// and every row (with 1 KiB of padding) and the whole image (with 128 rows
/// of padding) addressable in an `int`, assuming 8 bytes per pixel.
///
/// Fails with `AVERROR(EINVAL)` otherwise, after logging the size at
/// `AV_LOG_ERROR` in C's `IMGUTILS` class context.
pub fn check_size(width: u32, height: u32) -> Result<(), i32> {
    // SAFETY: a NULL log context is "no parent context", and a zero offset
    // leaves the error level unchanged; C reads nothing else through
    // pointers.
    let ret = unsafe { ffi::av_image_check_size(width, height, 0, core::ptr::null_mut()) };
    if ret < 0 { Err(ret) } else { Ok(()) }
}

/// Wraps: av_image_check_size
///
/// As [`check_size`], logging a failure at `AV_LOG_ERROR + log_offset` with
/// `log_ctx` as the parent log context.
///
/// A `log_offset` above `INT_MAX - AV_LOG_ERROR`, which would overflow the
/// level C logs at, is rejected with `AVERROR(EINVAL)` before C runs.
///
/// `log_ctx` stays a raw seam while logging contexts have no safe handle:
/// pass NULL for no parent context.
///
/// # Safety
///
/// `log_ctx` must be NULL or point to a live struct whose first member is a
/// `const AVClass *` (NULL, or a valid class) that `av_log` and the installed
/// log callback may read during the call, as `parent_log_context_offset`
/// contexts require.
pub unsafe fn check_size_with_context(
    width: u32,
    height: u32,
    log_offset: i32,
    log_ctx: *mut c_void,
) -> Result<(), i32> {
    if log_offset > c_int::MAX - ffi::AV_LOG_ERROR as c_int {
        return Err(AVERROR_EINVAL);
    }
    // SAFETY: the caller vouches for `log_ctx`, which C only reads while
    // logging; `log_offset` cannot overflow the error level C adds it to.
    let ret = unsafe { ffi::av_image_check_size(width, height, log_offset, log_ctx) };
    if ret < 0 { Err(ret) } else { Ok(()) }
}

/// Why [`copy_plane`] or [`copy_plane_from_uncacheable`] refused a copy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PlaneCopyError {
    /// A dimension or linesize exceeds what C's parameters take.
    TooLarge,
    /// `bytewidth` exceeds the magnitude of a linesize, so rows would
    /// overlap; C asserts against it and aborts.
    RowTooWide,
    /// The destination slice is shorter than the plane's rows reach.
    DestinationTooShort,
    /// The source slice is shorter than the plane's rows reach.
    SourceTooShort,
}

impl fmt::Display for PlaneCopyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::TooLarge => "plane dimension or linesize out of range",
            Self::RowTooWide => "plane row width exceeds a linesize",
            Self::DestinationTooShort => "destination plane ends past its buffer",
            Self::SourceTooShort => "source plane ends past its buffer",
        })
    }
}

impl std::error::Error for PlaneCopyError {}

/// The offset of row 0 in a plane slice of `len` bytes when `rows` rows of
/// `row_len` bytes, `linesize` apart, all lie inside it, as the
/// [module docs](self) lay a plane out; `None` when a row would leave it.
pub(crate) fn plane_origin(
    len: usize,
    linesize: isize,
    row_len: usize,
    rows: usize,
) -> Option<usize> {
    let Some(last) = rows.checked_sub(1) else {
        return Some(0);
    };
    let span = last.checked_mul(linesize.unsigned_abs())?;
    if span.checked_add(row_len)? > len {
        return None;
    }
    Some(if linesize < 0 { span } else { 0 })
}

/// The offsets of row 0 in `dst` and `src` for a copy of `height` rows of
/// `bytewidth` bytes, after the checks C asserts.
fn plane_copy_origins(
    dst_len: usize,
    dst_linesize: isize,
    src_len: usize,
    src_linesize: isize,
    bytewidth: usize,
    height: usize,
) -> Result<(usize, usize), PlaneCopyError> {
    if bytewidth > dst_linesize.unsigned_abs() || bytewidth > src_linesize.unsigned_abs() {
        return Err(PlaneCopyError::RowTooWide);
    }
    let dst_at = plane_origin(dst_len, dst_linesize, bytewidth, height)
        .ok_or(PlaneCopyError::DestinationTooShort)?;
    let src_at = plane_origin(src_len, src_linesize, bytewidth, height)
        .ok_or(PlaneCopyError::SourceTooShort)?;
    Ok((dst_at, src_at))
}

/// Wraps: av_image_copy_plane
///
/// Copies `height` rows of `bytewidth` bytes from the plane in `src`, rows
/// `src_linesize` apart, to the plane in `dst`, rows `dst_linesize` apart,
/// laid out as the [module docs](self) describe: row `r` of `src` becomes
/// row `r` of `dst`, so linesizes of opposite signs flip the plane
/// vertically. Bytes between rows are left alone.
///
/// Fails without touching `dst` when `bytewidth` exceeds the magnitude of
/// either linesize (on which C aborts), when either plane ends past its
/// slice, or when `bytewidth` or `height` exceeds `INT_MAX`.
pub fn copy_plane(
    dst: &mut [u8],
    dst_linesize: i32,
    src: &[u8],
    src_linesize: i32,
    bytewidth: usize,
    height: usize,
) -> Result<(), PlaneCopyError> {
    let (Ok(c_bytewidth), Ok(c_height)) = (c_int::try_from(bytewidth), c_int::try_from(height))
    else {
        return Err(PlaneCopyError::TooLarge);
    };
    let (dst_at, src_at) = plane_copy_origins(
        dst.len(),
        dst_linesize as isize,
        src.len(),
        src_linesize as isize,
        bytewidth,
        height,
    )?;
    if bytewidth == 0 || height == 0 {
        // C copies nothing; the slices' pointers may be dangling.
        return Ok(());
    }
    // SAFETY: `plane_origin` placed both row-0 offsets inside their slices,
    // and the pointers derive from the whole slices.
    let (dst_row0, src_row0) = unsafe { (dst.as_mut_ptr().add(dst_at), src.as_ptr().add(src_at)) };
    // SAFETY: C writes `bytewidth` bytes at `dst_row0 + r * dst_linesize`
    // and reads as many at `src_row0 + r * src_linesize` for `r` in
    // `0..height`, all inside `dst` and `src` by `plane_origin`; the borrows
    // are disjoint, so no row copy overlaps; both linesizes cover
    // `bytewidth`, so C's asserts hold. C keeps no pointer.
    unsafe {
        ffi::av_image_copy_plane(
            dst_row0,
            dst_linesize,
            src_row0,
            src_linesize,
            c_bytewidth,
            c_height,
        )
    };
    Ok(())
}

/// The row width `av_image_copy_plane_uc_from`'s x86 SSE4 path copies:
/// `FFALIGN(bytewidth, 64)`.
const UC_ROW_ALIGN: usize = 64;

/// The alignment the SSE4 path's `movntdqa` loads and `movdqa` stores
/// require of every row it copies.
const UC_ACCESS_ALIGN: usize = 16;

/// Wraps: av_image_copy_plane_uc_from
///
/// As [`copy_plane`], for a source in uncacheable memory (GPU-mapped, say):
/// on x86 CPUs with SSE4.1, C reads it with streaming loads, which can be
/// much faster there. Linesizes are `isize`, as in C.
///
/// That fast path copies whole 64-byte blocks: when both linesizes are at
/// least `bytewidth` rounded up to a multiple of 64, every row's bytes past
/// `bytewidth` up to that multiple may be copied too, which both linesizes
/// cover. Aligning the rows to 64 bytes gets the best performance.
///
/// Fails as [`copy_plane`] does, and when `height` exceeds `INT_MAX`, a
/// linesize is `isize::MIN`, or `bytewidth` is within 63 of `isize::MAX`.
///
/// # Divergence from C
///
/// C takes the fast path whenever both linesizes cover the rounded-up width,
/// and it then faults unless both planes and linesizes are 16-byte aligned,
/// and writes and reads past the plane when the last row of either slice
/// does not hold the rounded-up width. For such planes this wrapper copies
/// the `bytewidth` bytes of each row itself instead, as C's plain path (on
/// other CPUs) does.
pub fn copy_plane_from_uncacheable(
    dst: &mut [u8],
    dst_linesize: isize,
    src: &[u8],
    src_linesize: isize,
    bytewidth: usize,
    height: usize,
) -> Result<(), PlaneCopyError> {
    let Ok(c_height) = c_int::try_from(height) else {
        return Err(PlaneCopyError::TooLarge);
    };
    // `FFABS` of `PTRDIFF_MIN` and `FFALIGN` near `PTRDIFF_MAX` overflow in C.
    if dst_linesize == isize::MIN
        || src_linesize == isize::MIN
        || bytewidth > isize::MAX as usize - (UC_ROW_ALIGN - 1)
    {
        return Err(PlaneCopyError::TooLarge);
    }
    let (dst_at, src_at) = plane_copy_origins(
        dst.len(),
        dst_linesize,
        src.len(),
        src_linesize,
        bytewidth,
        height,
    )?;
    if bytewidth == 0 || height == 0 {
        // C's fast path would loop away on either; it copies nothing.
        return Ok(());
    }
    if !uc_call_is_sound(dst, dst_linesize, src, src_linesize, bytewidth, height) {
        copy_rows(
            dst,
            dst_at,
            dst_linesize,
            src,
            src_at,
            src_linesize,
            bytewidth,
            height,
        );
        return Ok(());
    }
    // SAFETY: as in `copy_plane`.
    let (dst_row0, src_row0) = unsafe { (dst.as_mut_ptr().add(dst_at), src.as_ptr().add(src_at)) };
    // SAFETY: as in `copy_plane`, with `bytewidth` and `height` positive. On
    // the SSE4 path C copies `FFALIGN(bytewidth, 64)` bytes of each row
    // instead, which `uc_call_is_sound` checked inside both slices (and
    // inside each linesize, so no two rows overlap), from and to 16-byte
    // aligned rows; otherwise it takes the plain path.
    unsafe {
        ffi::av_image_copy_plane_uc_from(
            dst_row0,
            dst_linesize,
            src_row0,
            src_linesize,
            bytewidth as isize,
            c_height,
        )
    };
    Ok(())
}

/// Whether C may call `av_image_copy_plane_uc_from` for these validated,
/// non-empty planes: its SSE4 path, taken whatever the CPU when both
/// linesizes cover the 64-byte-rounded width, needs that width inside both
/// slices' last rows and 16-byte aligned rows, both of which the plain path
/// does without.
fn uc_call_is_sound(
    dst: &[u8],
    dst_linesize: isize,
    src: &[u8],
    src_linesize: isize,
    bytewidth: usize,
    height: usize,
) -> bool {
    // No overflow: `bytewidth` is at most `isize::MAX - 63`.
    let wide = bytewidth.next_multiple_of(UC_ROW_ALIGN);
    let fast = |linesize: isize| linesize >= 0 && wide <= linesize as usize;
    if !(fast(dst_linesize) && fast(src_linesize)) {
        return true;
    }
    // Both linesizes are positive: row 0 starts each slice.
    let aligned = |at: *const u8, linesize: isize| {
        at.addr().is_multiple_of(UC_ACCESS_ALIGN)
            && (linesize as usize).is_multiple_of(UC_ACCESS_ALIGN)
    };
    aligned(dst.as_ptr(), dst_linesize)
        && aligned(src.as_ptr(), src_linesize)
        && plane_origin(dst.len(), dst_linesize, wide, height).is_some()
        && plane_origin(src.len(), src_linesize, wide, height).is_some()
}

/// `image_copy_plane`'s plain row loop, on planes `plane_copy_origins`
/// validated.
#[allow(clippy::too_many_arguments)]
fn copy_rows(
    dst: &mut [u8],
    dst_at: usize,
    dst_linesize: isize,
    src: &[u8],
    src_at: usize,
    src_linesize: isize,
    bytewidth: usize,
    height: usize,
) {
    // No overflow: every row start lies inside its slice, at most
    // `isize::MAX` bytes from row 0.
    let row = |at: usize, linesize: isize, r: usize| at.wrapping_add_signed(r as isize * linesize);
    for r in 0..height {
        let d = row(dst_at, dst_linesize, r);
        let s = row(src_at, src_linesize, r);
        dst[d..d + bytewidth].copy_from_slice(&src[s..s + bytewidth]);
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    /// A 64-byte aligned buffer, so tests choose every row's alignment.
    #[repr(C, align(64))]
    struct Aligned<const N: usize>([u8; N]);

    fn pattern(len: usize, seed: u8) -> Vec<u8> {
        (0..len)
            .map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed))
            .collect()
    }

    /// `av_image_copy_plane` of the same planes, the C reference of the plain
    /// path.
    #[allow(clippy::too_many_arguments)]
    fn c_copy_plane(
        dst: &mut [u8],
        dst_at: usize,
        dl: i32,
        src: &[u8],
        src_at: usize,
        sl: i32,
        w: usize,
        h: usize,
    ) {
        // SAFETY: the callers pass planes `plane_copy_origins` accepted, with
        // positive `w` and `h`, at the origins it computed.
        unsafe {
            ffi::av_image_copy_plane(
                dst.as_mut_ptr().add(dst_at),
                dl,
                src.as_ptr().add(src_at),
                sl,
                w as c_int,
                h as c_int,
            )
        }
    }

    #[test]
    fn plane_origin_spans_exactly_the_rows() {
        assert_eq!(plane_origin(0, 5, 3, 0), Some(0));
        assert_eq!(plane_origin(0, -5, 3, 0), Some(0));
        assert_eq!(plane_origin(3, 5, 3, 1), Some(0));
        assert_eq!(plane_origin(2, 5, 3, 1), None);
        assert_eq!(plane_origin(13, 5, 3, 3), Some(0));
        assert_eq!(plane_origin(12, 5, 3, 3), None);
        assert_eq!(plane_origin(13, -5, 3, 3), Some(10));
        assert_eq!(plane_origin(12, -5, 3, 3), None);
        // Overlapping and repeated rows.
        assert_eq!(plane_origin(4, 1, 3, 2), Some(0));
        assert_eq!(plane_origin(3, 0, 3, 100), Some(0));
        // Spans beyond `usize` never fit.
        assert_eq!(plane_origin(usize::MAX - 1, isize::MAX, 1, 3), None);
        assert_eq!(plane_origin(usize::MAX, isize::MAX, 1, 3), Some(0));
        assert_eq!(plane_origin(usize::MAX, 1, usize::MAX, 2), None);
    }

    #[test]
    fn plane_copy_origins_rejects_as_c_asserts() {
        use PlaneCopyError::*;
        assert_eq!(plane_copy_origins(100, 4, 100, 8, 5, 1), Err(RowTooWide));
        assert_eq!(plane_copy_origins(100, 8, 100, -4, 5, 0), Err(RowTooWide));
        assert_eq!(
            plane_copy_origins(100, isize::MIN, 100, 8, 5, 1),
            Ok((0, 0))
        );
        assert_eq!(
            plane_copy_origins(100, isize::MIN, 100, 8, 5, 2),
            Err(DestinationTooShort)
        );
        assert_eq!(
            plane_copy_origins(12, 8, 13, 8, 5, 2),
            Err(DestinationTooShort)
        );
        assert_eq!(plane_copy_origins(13, 8, 12, -8, 5, 2), Err(SourceTooShort));
        assert_eq!(plane_copy_origins(13, -8, 13, 8, 5, 2), Ok((8, 0)));
    }

    #[test]
    fn plane_copy_errors_display() {
        for e in [
            PlaneCopyError::TooLarge,
            PlaneCopyError::RowTooWide,
            PlaneCopyError::DestinationTooShort,
            PlaneCopyError::SourceTooShort,
        ] {
            assert!(!e.to_string().is_empty());
            let _: &dyn std::error::Error = &e;
        }
    }

    #[test]
    fn copy_rows_matches_c_plain_path() {
        for (dl, sl, w, h) in [
            (7isize, 9isize, 5usize, 4usize),
            (-7, 9, 5, 4),
            (7, -9, 7, 3),
            (-8, -8, 8, 1),
        ] {
            let src = pattern(256, 3);
            let (mut rust, mut c) = (pattern(256, 100), pattern(256, 100));
            let (dst_at, src_at) = plane_copy_origins(rust.len(), dl, src.len(), sl, w, h).unwrap();
            copy_rows(&mut rust, dst_at, dl, &src, src_at, sl, w, h);
            c_copy_plane(&mut c, dst_at, dl as i32, &src, src_at, sl as i32, w, h);
            assert_eq!(rust, c, "{dl} {sl} {w} {h}");
        }
    }

    #[test]
    fn uc_call_is_sound_only_for_aligned_covered_fast_planes() {
        let buf = Aligned([0u8; 512]);
        let b = &buf.0[..];
        // Not eligible for the fast path: C's plain path takes anything.
        assert!(uc_call_is_sound(&b[1..], 63, b, 64, 10, 3));
        assert!(uc_call_is_sound(&b[1..], -64, &b[3..], 64, 10, 3));
        assert!(uc_call_is_sound(&b[1..200], 64, b, 64, 65, 2));
        // Eligible, aligned and covered.
        assert!(uc_call_is_sound(b, 64, b, 128, 10, 3));
        assert!(uc_call_is_sound(&b[..192], 64, &b[16..16 + 192], 64, 64, 3));
        // Eligible but misaligned rows or pointers, or a short last row.
        assert!(!uc_call_is_sound(b, 72, b, 64, 10, 3));
        assert!(!uc_call_is_sound(b, 64, b, 80 + 8, 10, 3));
        assert!(!uc_call_is_sound(&b[8..], 64, b, 64, 10, 3));
        assert!(!uc_call_is_sound(b, 64, &b[1..], 64, 10, 3));
        assert!(!uc_call_is_sound(&b[..64 * 2 + 10], 64, b, 64, 10, 3));
        assert!(!uc_call_is_sound(b, 64, &b[..64 * 2 + 63], 64, 10, 3));
    }

    /// Regression for a C defect: `av_image_copy_plane_uc_from` faults on
    /// such planes on SSE4.1 CPUs; the wrapper copies them as C's plain path
    /// does.
    #[test]
    fn misaligned_fast_planes_copy_as_the_plain_path() {
        let src = Aligned::<1024>(core::array::from_fn(|i| (i * 7 % 251) as u8));
        for (dst_off, dl, src_off, sl, w, h) in [
            (0usize, 72isize, 0usize, 64isize, 10usize, 4usize),
            (8, 64, 0, 64, 10, 4),
            (0, 64, 1, 64, 64, 4),
            (0, 80, 0, 96, 17, 3),
        ] {
            let mut rust = Aligned([0xeeu8; 1024]);
            let mut c = Aligned([0xeeu8; 1024]);
            let len = (h - 1) * dl as usize + w;
            copy_plane_from_uncacheable(
                &mut rust.0[dst_off..dst_off + len],
                dl,
                &src.0[src_off..],
                sl,
                w,
                h,
            )
            .unwrap();
            c_copy_plane(
                &mut c.0, dst_off, dl as i32, &src.0, src_off, sl as i32, w, h,
            );
            assert_eq!(rust.0, c.0, "{dst_off} {dl} {src_off} {sl} {w} {h}");
        }
    }

    /// A fast-path plane whose last row lacks the rounded-up width copies
    /// only `bytewidth` bytes there, never past the slice.
    #[test]
    fn short_last_rows_copy_as_the_plain_path() {
        let src = Aligned::<256>(core::array::from_fn(|i| i as u8));
        let mut dst = Aligned([0u8; 256]);
        let len = 64 * 2 + 10;
        copy_plane_from_uncacheable(&mut dst.0[..len], 64, &src.0, 64, 10, 3).unwrap();
        for r in 0..3 {
            assert_eq!(dst.0[r * 64..r * 64 + 10], src.0[r * 64..r * 64 + 10]);
        }
        assert!(dst.0[len..].iter().all(|&b| b == 0));
    }

    #[test]
    fn oversized_arguments_are_rejected_before_c() {
        let mut dst = [0u8; 8];
        let src = [0u8; 8];
        assert_eq!(
            copy_plane(&mut dst, 8, &src, 8, c_int::MAX as usize + 1, 0),
            Err(PlaneCopyError::TooLarge)
        );
        assert_eq!(
            copy_plane(&mut dst, 8, &src, 8, 1, c_int::MAX as usize + 1),
            Err(PlaneCopyError::TooLarge)
        );
        let uc = |dl, sl, w, h| copy_plane_from_uncacheable(&mut [], dl, &[], sl, w, h);
        assert_eq!(uc(isize::MIN, 8, 1, 0), Err(PlaneCopyError::TooLarge));
        assert_eq!(uc(8, isize::MIN, 1, 0), Err(PlaneCopyError::TooLarge));
        assert_eq!(
            uc(isize::MAX, isize::MAX, isize::MAX as usize - 62, 0),
            Err(PlaneCopyError::TooLarge)
        );
        assert_eq!(
            uc(isize::MAX, isize::MAX, isize::MAX as usize - 63, 0),
            Ok(())
        );
        assert_eq!(
            uc(8, 8, 1, c_int::MAX as usize + 1),
            Err(PlaneCopyError::TooLarge)
        );
    }

    #[test]
    fn overflowing_log_offsets_are_rejected_before_c() {
        let max = c_int::MAX - ffi::AV_LOG_ERROR as c_int;
        // SAFETY: a NULL log context.
        let r = unsafe { check_size_with_context(0, 0, max + 1, core::ptr::null_mut()) };
        assert_eq!(r, Err(AVERROR_EINVAL));
        // SAFETY: as above; the size is valid, so nothing is logged.
        let r = unsafe { check_size_with_context(16, 16, c_int::MAX, core::ptr::null_mut()) };
        assert_eq!(r, Err(AVERROR_EINVAL));
        // SAFETY: as above.
        let r = unsafe { check_size_with_context(16, 16, max, core::ptr::null_mut()) };
        assert_eq!(r, Ok(()));
        assert_eq!(AVERROR_EINVAL, -22);
    }

    /// Runs the test `name` alone in a child process, where it may force the
    /// process-wide CPU flags; true in that child.
    fn isolated(name: &str) -> bool {
        const KEY: &str = "CRUSTIFY_UNIT_IMGUTILS_ISOLATED";
        if std::env::var_os(KEY).is_some() {
            return true;
        }
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", name, "--test-threads=1"])
            .env(KEY, "1")
            .status()
            .unwrap();
        assert!(status.success(), "isolated run failed: {status}");
        false
    }

    /// Without SSE4.1, C copies even fast-path planes on its plain path:
    /// `bytewidth` bytes a row, as `av_image_copy_plane` does.
    #[test]
    fn uncacheable_copy_without_sse4_is_the_plain_copy() {
        if !isolated(
            "libavutil::imgutils::unit_tests::uncacheable_copy_without_sse4_is_the_plain_copy",
        ) {
            return;
        }
        // SAFETY: no flags at all, in a process running only this test.
        unsafe { ffi::av_force_cpu_flags(0) };
        let src = Aligned::<1024>(core::array::from_fn(|i| (i * 5 % 253) as u8));
        for (dl, sl, w, h) in [
            (64isize, 64isize, 10usize, 4usize),
            (128, 192, 64, 3),
            (64, 128, 33, 5),
        ] {
            let (mut rust, mut c) = (Aligned([0x11u8; 1024]), Aligned([0x11u8; 1024]));
            copy_plane_from_uncacheable(&mut rust.0, dl, &src.0, sl, w, h).unwrap();
            c_copy_plane(&mut c.0, 0, dl as i32, &src.0, 0, sl as i32, w, h);
            assert_eq!(rust.0, c.0, "{dl} {sl} {w} {h}");
        }
        // SAFETY: as above; detection again.
        unsafe { ffi::av_force_cpu_flags(-1) };
    }
}
