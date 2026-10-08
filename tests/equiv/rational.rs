//! Equivalence of `libavutil/rational.h`'s [`Rational`] with C's `AVRational`,
//! on independently owned values: built by `av_make_q` against
//! [`Rational::new`], read by C's `av_inv_q`, `av_q2d` and `av_cmp_q` against
//! the safe getters, and written in place by `av_reduce` against the safe
//! setters, in Rust-owned and C-owned storage.

use core::ffi::c_void;
use core::mem::{align_of, offset_of, size_of};

use ffibox::CZeroable;
use ffmpeg::libavutil::ffi;
use ffmpeg::libavutil::mem::{HeapBox, alloc_cell_array_zeroed};
use ffmpeg::libavutil::rational::{Rational, RationalMut, RationalRef};

/// Pairs at the edges C stores: `0/0`, the infinities, signs on either part,
/// the `int` extremes, and unreduced fractions.
const PAIRS: [(i32, i32); 14] = [
    (0, 0),
    (1, 0),
    (-1, 0),
    (0, 1),
    (1, 1),
    (6, 4),
    (-6, 4),
    (6, -4),
    (30000, 1001),
    (1, 90000),
    (i32::MAX, 1),
    (i32::MIN, 1),
    (1, i32::MIN),
    (i32::MIN, i32::MAX),
];

/// C's `av_make_q(num, den)`.
fn c_make(num: i32, den: i32) -> ffi::AVRational {
    // SAFETY: `av_make_q` takes both parts by value and has no preconditions.
    unsafe { ffi::av_make_q(num, den) }
}

/// What C reads out of `q`: its parts, through `av_inv_q`'s swap, and its
/// value as a `double` through `av_q2d`, as bits.
fn c_reads(q: ffi::AVRational) -> (i32, i32, u64) {
    // SAFETY: both take the struct by value and have no preconditions.
    let (inv, d) = unsafe { (ffi::av_inv_q(q), ffi::av_q2d(q)) };
    (inv.den, inv.num, d.to_bits())
}

/// The same reads from a Rust handle: its getters, and C's `av_q2d` on the
/// copy it hands out.
fn rust_reads(r: RationalRef<'_>) -> (i32, i32, u64) {
    // SAFETY: `av_q2d` takes the struct by value and has no preconditions.
    let d = unsafe { ffi::av_q2d(r.get().into_raw()) };
    (r.num(), r.den(), d.to_bits())
}

/// C's `av_cmp_q(a, b)`.
fn c_cmp(a: ffi::AVRational, b: ffi::AVRational) -> i32 {
    // SAFETY: `av_cmp_q` takes both structs by value; its products are
    // computed in 64 bits, so no `int` pair overflows.
    unsafe { ffi::av_cmp_q(a, b) }
}

/// C's `av_reduce` of `num / den` into the two `int`s at `dst`.
///
/// # Safety
///
/// `dst` must address a live `AVRational` nothing else accesses during the
/// call.
unsafe fn c_reduce_into(dst: *mut ffi::AVRational, num: i64, den: i64, max: i64) -> i32 {
    // SAFETY: the caller guarantees `dst` is live and unaliased; the field
    // pointers are projected without forming a reference, and `av_reduce`
    // writes one `int` through each.
    unsafe { ffi::av_reduce(&raw mut (*dst).num, &raw mut (*dst).den, num, den, max) }
}

#[test]
fn layout_matches_the_c_struct() {
    assert_eq!(size_of::<Rational>(), size_of::<ffi::AVRational>());
    assert_eq!(align_of::<Rational>(), align_of::<ffi::AVRational>());
    assert_eq!(offset_of!(ffi::AVRational, num), 0);
    assert_eq!(offset_of!(ffi::AVRational, den), size_of::<i32>());
}

#[test]
fn new_is_av_make_q() {
    for (num, den) in PAIRS {
        let rust = Rational::new(num, den);
        let c = c_make(num, den);
        assert_eq!(rust_reads(rust.as_ref()), c_reads(c), "{num}/{den}");
        assert_eq!((rust.as_ref().num(), rust.as_ref().den()), (num, den));
        let back = rust.into_raw();
        assert_eq!((back.num, back.den), (c.num, c.den));
    }
}

#[test]
fn getters_read_what_c_reads() {
    for (num, den) in PAIRS {
        // Rust builds its value through the setters, C through its
        // constructor; each side reads its own.
        let mut rust = Rational::zeroed();
        let mut m = rust.as_mut();
        m.set_num(num);
        m.set_den(den);
        let c = c_make(num, den);
        assert_eq!(rust_reads(rust.as_ref()), c_reads(c), "{num}/{den}");
        // C orders the Rust value and its own as the same number, or as
        // incomparable for `0/0` exactly as it does against itself.
        assert_eq!(c_cmp(rust.into_raw(), c), c_cmp(c, c), "{num}/{den}");
        for (n2, d2) in PAIRS {
            let other = c_make(n2, d2);
            assert_eq!(c_cmp(rust.into_raw(), other), c_cmp(c, other));
        }
    }
}

#[test]
fn whole_value_set_and_get_match_c_struct_copies() {
    // One Rust value is overwritten at every step; C copies its struct anew.
    let mut rust = Rational::zeroed();
    for (step, (num, den)) in PAIRS.into_iter().enumerate() {
        let next = c_make(num, den);
        rust.as_mut().set(Rational::from_raw(next));
        let mut c = next;
        let copy = rust.as_ref().get();
        assert_eq!(rust_reads(copy.as_ref()), c_reads(c), "step {step}");
        // C swaps the parts of the value Rust hands it; Rust stores the swap.
        // SAFETY: `av_inv_q` takes the struct by value and has no
        // preconditions.
        let inv = unsafe { ffi::av_inv_q(rust.into_raw()) };
        rust.as_mut().set(inv.into());
        // SAFETY: as above.
        c = unsafe { ffi::av_inv_q(c) };
        assert_eq!(
            rust_reads(rust.as_ref()),
            c_reads(c),
            "step {step} inverted"
        );
    }
}

#[test]
fn c_writes_in_place_read_back_through_the_handles() {
    // `av_reduce` writes the parts of a Rust-owned `av_mallocz` box and of a
    // C-owned stack struct; after each call, and after each Rust write, both
    // read the same through C and through the handles.
    let mut rust = HeapBox::<Rational>::new_zeroed().unwrap();
    let mut c = ffi::AVRational::default();
    let steps: [(i64, i64, i64); 7] = [
        (6, 4, i32::MAX as i64),
        (30000, 1001, i32::MAX as i64),
        (-90000, 3003, 1000),
        (1, 0, i32::MAX as i64),
        (0, 7, 255),
        (i64::MAX, 3, i32::MAX as i64),
        (355, 113, 100),
    ];
    for (step, (num, den, max)) in steps.into_iter().enumerate() {
        // SAFETY: `as_c_ptr` addresses the live box, which no handle borrows
        // during the call.
        let exact_rust = unsafe { c_reduce_into(rust.as_c_ptr(), num, den, max) };
        // SAFETY: `c` is a live local struct, not otherwise borrowed.
        let exact_c = unsafe { c_reduce_into(&raw mut c, num, den, max) };
        assert_eq!(exact_rust, exact_c, "step {step}");
        assert_eq!(rust_reads(rust.as_ref()), c_reads(c), "step {step}");

        // A Rust write of one part, mirrored on the C struct.
        let num = rust.as_ref().num().wrapping_neg();
        rust.as_mut().set_num(num);
        c.num = c.num.wrapping_neg();
        assert_eq!(rust_reads(rust.as_ref()), c_reads(c), "step {step} negated");
        // SAFETY: `as_c_ptr` addresses the live box, read by value here.
        let in_c = unsafe { rust.as_c_ptr().read() };
        assert_eq!(c_reads(in_c), c_reads(c), "step {step} as C reads it");
    }
}

#[test]
fn handles_over_c_filled_storage_match_c_reads() {
    // C fills an `av_malloc` block in place, as C fills a struct field a
    // wrapper's accessor later hands out as a handle.
    let mut c = c_make(24, 1);
    let mut boxed = HeapBox::<Rational>::new_uninit().unwrap();
    // SAFETY: `as_c_ptr` addresses `size_of::<AVRational>()` writable,
    // aligned bytes the box owns.
    unsafe { boxed.as_c_ptr().write(c) };
    // SAFETY: the write above initialised the whole struct.
    let mut boxed = unsafe { boxed.assume_init() };
    let mut m: RationalMut<'_> = boxed.as_mut();
    m.set_den(25);
    c.den = 25;
    assert_eq!(rust_reads(boxed.as_ref()), c_reads(c));
    // A clone is `av_memdup`'s byte copy; C's is a struct copy.
    let copy = boxed.clone();
    let c_copy = c;
    boxed.as_mut().set_num(0);
    c.num = 0;
    assert_eq!(rust_reads(copy.as_ref()), c_reads(c_copy));
    assert_eq!(rust_reads(boxed.as_ref()), c_reads(c));
}

#[test]
fn zeroed_arrays_read_as_c_zeroed_storage() {
    const LEN: usize = 8;
    let mut rust = alloc_cell_array_zeroed::<Rational>(LEN).unwrap();
    // SAFETY: `av_calloc` takes its sizes by value.
    let c = unsafe { ffi::av_calloc(LEN, size_of::<ffi::AVRational>()) }.cast::<ffi::AVRational>();
    assert!(!c.is_null());
    for i in 0..LEN {
        // SAFETY: `i` is in bounds of the live, zero-filled C block.
        let ci = unsafe { c.add(i).read() };
        assert_eq!(rust_reads(rust.as_handles().get(i).unwrap()), c_reads(ci));
    }
    // A `{0, 0}`-terminated list, as `av_find_nearest_q_idx` takes: written
    // by both sides, then compared slot by slot.
    for (i, &(num, den)) in PAIRS[1..LEN].iter().enumerate() {
        rust.as_handles_mut()
            .get_mut(i)
            .unwrap()
            .set(Rational::new(num, den));
        // SAFETY: `i` is in bounds of the live C block, owned here.
        unsafe { c.add(i).write(c_make(num, den)) };
    }
    for i in 0..LEN {
        // SAFETY: as above.
        let ci = unsafe { c.add(i).read() };
        assert_eq!(
            rust_reads(rust.as_handles().get(i).unwrap()),
            c_reads(ci),
            "slot {i}"
        );
    }
    // SAFETY: `c` is the live `av_calloc` block allocated above.
    unsafe { ffi::av_free(c.cast::<c_void>()) };
}

// ---- av_reduce ----

/// Equivalence of `libavutil/rational.h`'s [`reduce`] with `av_reduce`, on
/// every argument the wrapper passes on.
mod reduce {
    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::rational::reduce;

    /// `av_reduce` in C, on its own out-parameters: the numerator,
    /// denominator and exactness.
    fn c_reduce(num: i64, den: i64, max: i64) -> (i32, i32, bool) {
        let (mut n, mut d) = (i32::MIN, i32::MIN);
        // SAFETY: both out-parameters are live locals; the tests pass `num` and
        // `den` other than `INT64_MIN` and `max` in `0..=INT_MAX`, for which C's
        // arithmetic does not overflow.
        let exact = unsafe { ffi::av_reduce(&mut n, &mut d, num, den, max) };
        (n, d, exact != 0)
    }

    /// The same parts of the wrapper's result.
    fn rust_reduce(num: i64, den: i64, max: i32) -> (i32, i32, bool) {
        let r = reduce(num, den, max).unwrap();
        (r.value.as_ref().num(), r.value.as_ref().den(), r.exact)
    }

    /// Terms at every magnitude class C branches on.
    const TERMS: [i64; 18] = [
        -i64::MAX,
        -(1 << 62) - 1,
        -(1 << 40),
        -1_000_003,
        -6,
        -1,
        0,
        1,
        2,
        3,
        6,
        1001,
        30000,
        i32::MAX as i64,
        i32::MAX as i64 + 1,
        3_141_592_653_589_793,
        (1 << 62) + 1,
        i64::MAX,
    ];

    /// Bounds from none to `INT_MAX`.
    const MAX: [i32; 8] = [0, 1, 2, 255, 1000, 65535, 1 << 30, i32::MAX];

    #[test]
    fn every_term_pair_and_bound_matches_c() {
        for num in TERMS {
            for den in TERMS {
                for max in MAX {
                    let r = rust_reduce(num, den, max);
                    assert_eq!(
                        r,
                        c_reduce(num, den, max.into()),
                        "{num}/{den} within {max}"
                    );
                }
            }
        }
    }

    #[test]
    fn pseudo_random_fractions_match_c() {
        let mut s = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = || {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            s
        };
        for _ in 0..5_000 {
            let (a, b, m) = (next(), next(), next());
            let num = (a as i64 >> (a % 63)).max(-i64::MAX);
            let den = (b as i64 >> (b % 63)).max(-i64::MAX);
            let max = (m % (i32::MAX as u64 + 1)) as i32 >> (m % 31);
            let r = rust_reduce(num, den, max);
            assert_eq!(
                r,
                c_reduce(num, den, max.into()),
                "{num}/{den} within {max}"
            );
        }
    }
}
