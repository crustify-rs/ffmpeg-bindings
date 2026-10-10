//! Safe wrappers over `libavutil/pixelutils.h`.
//!
//! [`SadFn`] calls a sum-of-absolute-differences function C selected for a
//! square block size and an [`SadAlignment`] (`av_pixelutils_sad_fn`), as
//! [`SadFn::new`] asks C for one (`av_pixelutils_get_sad_fn`).

use core::ffi::c_int;
use core::fmt;

use super::ffi;
use super::imgutils::plane_origin;

/// What a [`SadFn`] assumes of its blocks' addresses, as requested from
/// `av_pixelutils_get_sad_fn` (its `aligned` argument), so that C may pick
/// aligned SIMD loads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SadAlignment {
    /// No particular alignment (`0`).
    Unaligned,
    /// The first block aligned on the block size (`1`).
    First,
    /// Both blocks aligned on the block size (`2`).
    Both,
}

impl SadAlignment {
    /// The `aligned` argument of `av_pixelutils_get_sad_fn`.
    fn into_c(self) -> c_int {
        match self {
            Self::Unaligned => 0,
            Self::First => 1,
            Self::Both => 2,
        }
    }
}

/// Why [`SadFn::call`] refused a block pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SadError {
    /// The first block's rows end past its slice.
    FirstTooShort,
    /// The second block's rows end past its slice.
    SecondTooShort,
    /// A row of the first block is not aligned on the block size, as the
    /// function's [`SadAlignment`] assumes.
    FirstMisaligned,
    /// A row of the second block is not aligned on the block size, as the
    /// function's [`SadAlignment`] assumes.
    SecondMisaligned,
}

impl fmt::Display for SadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::FirstTooShort => "first SAD block ends past its buffer",
            Self::SecondTooShort => "second SAD block ends past its buffer",
            Self::FirstMisaligned => "first SAD block is not aligned on the block size",
            Self::SecondMisaligned => "second SAD block is not aligned on the block size",
        })
    }
}

impl std::error::Error for SadError {}

/// Wraps: av_pixelutils_sad_fn
///
/// A sum-of-absolute-differences function over square blocks of
/// [`size`](Self::size) x `size` bytes, as `av_pixelutils_get_sad_fn` selects
/// it for a block size and an [`SadAlignment`]: possibly SIMD code relying on
/// that alignment.
///
/// It keeps no state and reads only the blocks it is given: a handle is
/// `Copy`, `Send` and `Sync`.
#[derive(Clone, Copy, Debug)]
pub struct SadFn {
    /// A function from `av_pixelutils_get_sad_fn(log2_size, log2_size,
    /// alignment, _)`, which reads a `size` x `size` block from each source
    /// and nothing else, relying on `alignment`.
    sad: unsafe extern "C" fn(*const u8, isize, *const u8, isize) -> c_int,
    /// The block side's base-2 logarithm, `1..=5`.
    log2_size: u8,
    alignment: SadAlignment,
}

impl SadFn {
    /// Wraps: av_pixelutils_get_sad_fn
    ///
    /// The function C selects for blocks of `2^log2_size` x `2^log2_size`
    /// bytes whose addresses meet `alignment`: SIMD code when the CPU has it
    /// (as `av_get_cpu_flags` reports), plain C otherwise. `None` for a block
    /// size C has no function for: `log2_size` outside `1..=5` (2 to 32
    /// bytes). C compares square blocks only, so one size stands for both
    /// sides.
    ///
    /// C's log context is used only by a libavutil built without
    /// pixelutils, which logs that it is missing; this passes none.
    pub fn new(log2_size: u32, alignment: SadAlignment) -> Option<Self> {
        let bits = c_int::try_from(log2_size).ok()?;
        // SAFETY: C only reads its integer arguments and the CPU flags; the
        // log context, which it reads only to log that pixelutils is not
        // compiled in, is NULL.
        let sad = unsafe {
            ffi::av_pixelutils_get_sad_fn(bits, bits, alignment.into_c(), core::ptr::null_mut())
        }?;
        Some(Self {
            sad,
            // C returned a function, so `bits` is within its table, `1..=5`.
            log2_size: u8::try_from(log2_size).ok()?,
            alignment,
        })
    }

    /// The side of the square blocks the function compares, in bytes: 2, 4,
    /// 8, 16 or 32.
    #[inline]
    pub fn size(&self) -> usize {
        1 << self.log2_size
    }

    /// The alignment the function assumes of its blocks.
    #[inline]
    pub fn alignment(&self) -> SadAlignment {
        self.alignment
    }

    /// The sum of `|a - b|` over the bytes of the [`size`](Self::size) x
    /// `size` blocks in `src1`, rows `stride1` apart, and `src2`, rows
    /// `stride2` apart.
    ///
    /// Each block is laid out in its slice as an image plane is (see
    /// [`imgutils`](super::imgutils)): row 0 starts the slice for a
    /// non-negative stride and is the last row in memory for a negative one,
    /// so a block needs `(size - 1) * |stride| + size` bytes. Strides may be
    /// smaller than the block, overlapping its rows.
    ///
    /// Fails when a block ends past its slice, or when a row of a block the
    /// function's [`SadAlignment`] assumes aligned does not start at an
    /// address that is a multiple of the block size.
    pub fn call(
        &self,
        src1: &[u8],
        stride1: isize,
        src2: &[u8],
        stride2: isize,
    ) -> Result<u32, SadError> {
        let size = self.size();
        let at1 = plane_origin(src1.len(), stride1, size, size).ok_or(SadError::FirstTooShort)?;
        let at2 = plane_origin(src2.len(), stride2, size, size).ok_or(SadError::SecondTooShort)?;
        // SAFETY: `plane_origin` placed both row-0 offsets inside their
        // slices, and the pointers derive from the whole slices.
        let (row1, row2) = unsafe { (src1.as_ptr().add(at1), src2.as_ptr().add(at2)) };
        let aligned = |row: *const u8, stride: isize| {
            row.addr().is_multiple_of(size) && stride.unsigned_abs().is_multiple_of(size)
        };
        let (check1, check2) = match self.alignment {
            SadAlignment::Unaligned => (false, false),
            SadAlignment::First => (true, false),
            SadAlignment::Both => (true, true),
        };
        if check1 && !aligned(row1, stride1) {
            return Err(SadError::FirstMisaligned);
        }
        if check2 && !aligned(row2, stride2) {
            return Err(SadError::SecondMisaligned);
        }
        // SAFETY: C reads `size` bytes at `row1 + r * stride1` and
        // `row2 + r * stride2` for `r` in `0..size`, all inside `src1` and
        // `src2` by `plane_origin`, only reading them during the call; every
        // row the function's alignment assumes aligned is.
        let sum = unsafe { (self.sad)(row1, stride1, row2, stride2) };
        // At most `32 * 32 * 255`: never negative.
        Ok(sum as u32)
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    /// A 64-byte aligned buffer, so tests choose every row's alignment.
    #[repr(C, align(64))]
    struct Aligned<const N: usize>([u8; N]);

    const ALIGNMENTS: [(SadAlignment, c_int); 3] = [
        (SadAlignment::Unaligned, 0),
        (SadAlignment::First, 1),
        (SadAlignment::Both, 2),
    ];

    /// The function C selects for `log2_size` and `alignment`.
    fn sad_fn(log2_size: u8, alignment: SadAlignment) -> SadFn {
        SadFn::new(log2_size.into(), alignment).expect("a supported block size")
    }

    /// Every function C selects, by block size and alignment.
    fn all() -> impl Iterator<Item = SadFn> {
        (1..=5).flat_map(|bits| ALIGNMENTS.map(|a| sad_fn(bits, a.0)))
    }

    /// The sum of absolute differences of two blocks, as `sad_wxh` computes
    /// it, from the row-0 offsets.
    fn reference(
        size: usize,
        a: &[u8],
        at1: usize,
        s1: isize,
        b: &[u8],
        at2: usize,
        s2: isize,
    ) -> u32 {
        let mut sum = 0;
        for r in 0..size {
            let (r1, r2) = (
                at1.wrapping_add_signed(r as isize * s1),
                at2.wrapping_add_signed(r as isize * s2),
            );
            for x in 0..size {
                sum += u32::from(a[r1 + x].abs_diff(b[r2 + x]));
            }
        }
        sum
    }

    fn noise(seed: u32) -> Aligned<4096> {
        let mut x = seed | 1;
        Aligned(core::array::from_fn(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            x as u8
        }))
    }

    #[test]
    fn alignment_encodes_as_c_s_aligned_argument() {
        for (a, c) in ALIGNMENTS {
            assert_eq!(a.into_c(), c);
        }
    }

    #[test]
    fn new_selects_exactly_the_supported_sizes() {
        for (a, _) in ALIGNMENTS {
            for bits in 1..=5u32 {
                let f = SadFn::new(bits, a).unwrap();
                assert_eq!((f.size(), f.alignment()), (1 << bits, a));
                assert_eq!(u32::from(f.log2_size), bits);
            }
            for bits in [0, 6, 7, 31, 32, 255, 256, u32::MAX] {
                assert!(SadFn::new(bits, a).is_none(), "{bits} {a:?}");
            }
        }
    }

    #[test]
    fn new_returns_the_function_c_selects() {
        for (a, c) in ALIGNMENTS {
            for bits in 1..=5 {
                // SAFETY: a NULL log context; C only selects a function.
                let raw =
                    unsafe { ffi::av_pixelutils_get_sad_fn(bits, bits, c, core::ptr::null_mut()) };
                let f = SadFn::new(bits as u32, a).unwrap();
                assert_eq!(raw.map(|p| p as usize), Some(f.sad as usize));
            }
        }
    }

    #[test]
    fn handles_are_plain_shareable_values() {
        fn assert_traits<T: Copy + Send + Sync + core::fmt::Debug + 'static>() {}
        assert_traits::<SadFn>();
    }

    #[test]
    fn size_and_alignment_report_the_selection() {
        for f in all() {
            assert!([2, 4, 8, 16, 32].contains(&f.size()));
        }
        assert_eq!(sad_fn(3, SadAlignment::First).size(), 8);
        assert_eq!(
            sad_fn(5, SadAlignment::Both).alignment(),
            SadAlignment::Both
        );
    }

    #[test]
    fn sums_match_the_reference_and_c() {
        let (a, b) = (noise(7), noise(0x9e37_79b9));
        for f in all() {
            let n = f.size();
            for (s1, s2) in [
                (64isize, 64isize),
                (128, 32),
                (-64, 96),
                (-32, -64),
                (32, 0),
            ] {
                let need = |s: isize| (n - 1) * s.unsigned_abs() + n;
                let (l1, l2) = (&a.0[..need(s1)], &b.0[..need(s2)]);
                let sum = f.call(l1, s1, l2, s2).unwrap();
                let (at1, at2) = (
                    plane_origin(l1.len(), s1, n, n).unwrap(),
                    plane_origin(l2.len(), s2, n, n).unwrap(),
                );
                assert_eq!(
                    sum,
                    reference(n, l1, at1, s1, l2, at2, s2),
                    "{f:?} {s1} {s2}"
                );
                // SAFETY: both blocks lie inside the slices, at aligned rows
                // (64-byte aligned buffers, strides multiples of 32).
                let c = unsafe { (f.sad)(l1.as_ptr().add(at1), s1, l2.as_ptr().add(at2), s2) };
                assert_eq!(sum, c as u32);
            }
        }
    }

    #[test]
    fn extreme_blocks_sum_to_the_maximum() {
        let (zeros, ones) = (Aligned([0u8; 2048]), Aligned([0xffu8; 2048]));
        for f in all() {
            let n = f.size();
            let sum = f.call(&zeros.0, 64, &ones.0, 64).unwrap();
            assert_eq!(sum, (n * n * 255) as u32, "{f:?}");
            assert_eq!(f.call(&ones.0, 64, &ones.0, 64), Ok(0));
        }
    }

    #[test]
    fn short_blocks_are_rejected() {
        let a = noise(3);
        for f in all() {
            let n = f.size();
            let need = (n - 1) * 64 + n;
            assert_eq!(
                f.call(&a.0[..need - 1], 64, &a.0, 64),
                Err(SadError::FirstTooShort)
            );
            assert_eq!(
                f.call(&a.0, -64, &a.0[..need - 1], -64),
                Err(SadError::SecondTooShort)
            );
            assert_eq!(f.call(&[], 0, &a.0, 64), Err(SadError::FirstTooShort));
            assert_eq!(
                f.call(&a.0[..need], isize::MAX, &a.0, 64),
                Err(SadError::FirstTooShort)
            );
        }
    }

    #[test]
    fn misaligned_blocks_are_rejected_as_assumed() {
        let a = noise(5);
        for f in all() {
            let n = f.size();
            let (first, second) = match f.alignment() {
                SadAlignment::Unaligned => (false, false),
                SadAlignment::First => (true, false),
                SadAlignment::Both => (true, true),
            };
            let err = |e| Err::<u32, _>(e);
            let off = &a.0[1..];
            let r = f.call(off, 64, &a.0, 64);
            assert_eq!(r.is_err(), first, "{f:?}");
            if first {
                assert_eq!(r, err(SadError::FirstMisaligned));
            }
            let r = f.call(&a.0, 64, off, 64);
            assert_eq!(r.is_err(), second, "{f:?}");
            if second {
                assert_eq!(r, err(SadError::SecondMisaligned));
            }
            // A stride that is not a multiple of the block size misaligns
            // the rows after the first.
            let r = f.call(&a.0, 64 + n as isize / 2, &a.0, 64);
            assert_eq!(r.is_err(), first, "{f:?}");
            let r = f.call(&a.0, 64, &a.0, -(64 + n as isize / 2));
            assert_eq!(r.is_err(), second, "{f:?}");
        }
    }

    #[test]
    fn unaligned_functions_take_any_offset() {
        let (a, b) = (noise(11), noise(13));
        for bits in 1..=5u8 {
            let f = sad_fn(bits, SadAlignment::Unaligned);
            let n = f.size();
            for (o1, o2) in [(1usize, 3usize), (7, 0), (15, 17), (33, 61)] {
                let (l1, l2) = (
                    &a.0[o1..o1 + (n - 1) * 72 + n],
                    &b.0[o2..o2 + (n - 1) * 40 + n],
                );
                let sum = f.call(l1, 72, l2, 40).unwrap();
                assert_eq!(sum, reference(n, l1, 0, 72, l2, 0, 40), "{f:?} {o1} {o2}");
            }
        }
    }

    #[test]
    fn errors_display() {
        for e in [
            SadError::FirstTooShort,
            SadError::SecondTooShort,
            SadError::FirstMisaligned,
            SadError::SecondMisaligned,
        ] {
            assert!(!e.to_string().is_empty());
            let _: &dyn std::error::Error = &e;
        }
    }

    #[test]
    fn handles_cross_threads() {
        let (a, b) = (noise(17), noise(19));
        let fns: Vec<SadFn> = all().collect();
        let expect: Vec<u32> = fns
            .iter()
            .map(|f| f.call(&a.0, 64, &b.0, 64).unwrap())
            .collect();
        std::thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(|| {
                    for (f, e) in fns.iter().zip(&expect) {
                        assert_eq!(f.call(&a.0, 64, &b.0, 64), Ok(*e));
                    }
                });
            }
        });
    }

    /// Runs the test `name` alone in a child process, where it may force the
    /// process-wide CPU flags; true in that child.
    fn isolated(name: &str) -> bool {
        const KEY: &str = "CRUSTIFY_UNIT_PIXELUTILS_ISOLATED";
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

    /// C's plain functions (no SIMD), and the SSE2 ones without AVX2, sum
    /// as the reference does.
    #[test]
    fn every_cpu_tier_sums_alike() {
        if !isolated("libavutil::pixelutils::unit_tests::every_cpu_tier_sums_alike") {
            return;
        }
        // SAFETY: `av_get_cpu_flags` takes no arguments.
        let detected = unsafe { ffi::av_get_cpu_flags() };
        let sse2 = (ffi::AV_CPU_FLAG_MMX
            | ffi::AV_CPU_FLAG_MMXEXT
            | ffi::AV_CPU_FLAG_SSE
            | ffi::AV_CPU_FLAG_SSE2) as c_int;
        let (a, b) = (noise(23), noise(29));
        for mask in [0, detected & sse2, -1] {
            // SAFETY: a subset of the detected flags (or detection again), in
            // a process running only this test, which selects every function
            // after the change.
            unsafe { ffi::av_force_cpu_flags(mask) };
            for f in all() {
                let n = f.size();
                for (s1, s2) in [(64isize, 96isize), (-64, 32)] {
                    let need = |s: isize| (n - 1) * s.unsigned_abs() + n;
                    let (l1, l2) = (&a.0[..need(s1)], &b.0[..need(s2)]);
                    let at = |l: &[u8], s| plane_origin(l.len(), s, n, n).unwrap();
                    let expect = reference(n, l1, at(l1, s1), s1, l2, at(l2, s2), s2);
                    assert_eq!(f.call(l1, s1, l2, s2), Ok(expect), "{mask:#x} {f:?}");
                }
            }
        }
    }
}
