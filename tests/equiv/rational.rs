//! Equivalence of `libavutil/rational.h`'s [`Rational`] with C's `AVRational`,
//! on independently owned values: built by `av_make_q` against
//! [`Rational::new`], read by C's `av_inv_q`, `av_q2d` and `av_cmp_q` against
//! the safe getters, and written in place by `av_reduce` against the safe
//! setters, in Rust-owned and C-owned storage.
#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "test inputs are chosen within range, and the casts mirror the C conversions under comparison"
)]
#![expect(
    clippy::many_single_char_names,
    reason = "names follow the C fields, formulas and fixtures they mirror"
)]

use core::ffi::c_void;
use core::mem::{align_of, offset_of, size_of};

use ffibox::CZeroable;
use ffmpeg_bindings::libavutil::libavutil::ffi;
use ffmpeg_bindings::libavutil::libavutil::mem::{
    HeapBox, HeapVec, alloc_array_from_fn, alloc_cell_array_zeroed,
};
use ffmpeg_bindings::libavutil::libavutil::rational::{Rational, RationalMut, RationalRef};

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
    let (inv, d) = (unsafe { ffi::av_inv_q(q) }, unsafe { ffi::av_q2d(q) });
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
    // SAFETY: `dst` is to a live object, as the block below relies on, so the
    // place of its `den` field is in bounds; nothing is accessed here.
    let den_ptr = unsafe { &raw mut (*dst).den };
    // SAFETY: `dst` is to a live object, as the block below relies on, so the
    // place of its `num` field is in bounds; nothing is accessed here.
    let num_ptr = unsafe { &raw mut (*dst).num };
    // SAFETY: the caller guarantees `dst` is live and unaliased; the field
    // pointers are projected without forming a reference, and `av_reduce`
    // writes one `int` through each.
    unsafe { ffi::av_reduce(num_ptr, den_ptr, num, den, max) }
}

#[test]
fn layout_matches_the_c_struct() {
    assert_eq!(size_of::<Rational>(), size_of::<ffi::AVRational>());
    assert_eq!(align_of::<Rational>(), align_of::<ffi::AVRational>());
    assert_eq!(offset_of!(ffi::AVRational, num), 0);
    assert_eq!(offset_of!(ffi::AVRational, den), size_of::<i32>());
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
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
#[cfg_attr(miri, ignore = "calls C")]
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
#[cfg_attr(miri, ignore = "calls C")]
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
#[cfg_attr(miri, ignore = "calls C")]
fn c_writes_in_place_read_back_through_the_handles() {
    // `av_reduce` writes the parts of a Rust-owned `av_mallocz` box and of a
    // C-owned stack struct; after each call, and after each Rust write, both
    // read the same through C and through the handles.
    let mut rust = HeapBox::<Rational>::new_zeroed().unwrap();
    let mut c = ffi::AVRational::default();
    let steps: [(i64, i64, i64); 7] = [
        (6, 4, i64::from(i32::MAX)),
        (30000, 1001, i64::from(i32::MAX)),
        (-90000, 3003, 1000),
        (1, 0, i64::from(i32::MAX)),
        (0, 7, 255),
        (i64::MAX, 3, i64::from(i32::MAX)),
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
#[cfg_attr(miri, ignore = "calls C")]
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
#[cfg_attr(miri, ignore = "calls C")]
fn zeroed_arrays_read_as_c_zeroed_storage() {
    const LEN: usize = 8;
    let mut rust = alloc_cell_array_zeroed::<Rational>(LEN).unwrap();
    // SAFETY: `av_calloc` takes its sizes by value.
    let c = unsafe { ffi::av_calloc(LEN, size_of::<ffi::AVRational>()) }.cast::<ffi::AVRational>();
    assert!(!c.is_null());
    for i in 0..LEN {
        // SAFETY: as the comment below states, for this part of the operation.
        let c_add_5 = unsafe { c.add(i) };
        // SAFETY: `i` is in bounds of the live, zero-filled C block.
        let ci = unsafe { c_add_5.read() };
        assert_eq!(rust_reads(rust.as_handles().get(i).unwrap()), c_reads(ci));
    }
    // A `{0, 0}`-terminated list, as `av_find_nearest_q_idx` takes: written
    // by both sides, then compared slot by slot.
    for (i, &(num, den)) in PAIRS[1..LEN].iter().enumerate() {
        rust.as_handles_mut()
            .get_mut(i)
            .unwrap()
            .set(Rational::new(num, den));
        // SAFETY: as the comment below states, for this part of the operation.
        let c_add_4 = unsafe { c.add(i) };
        // SAFETY: `i` is in bounds of the live C block, owned here.
        unsafe { c_add_4.write(c_make(num, den)) };
    }
    for i in 0..LEN {
        // SAFETY: as the comment below states, for this part of the operation.
        let c_add_3 = unsafe { c.add(i) };
        // SAFETY: as above.
        let ci = unsafe { c_add_3.read() };
        assert_eq!(
            rust_reads(rust.as_handles().get(i).unwrap()),
            c_reads(ci),
            "slot {i}"
        );
    }
    // SAFETY: `c` is the live `av_calloc` block allocated above.
    unsafe { ffi::av_free(c.cast::<c_void>()) };
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn copies_of_a_run_match_c_memdup_after_every_step() {
    // Rust copies a run of rationals two ways: a Rust slice into an
    // `av_malloc` block, and a borrowed run element by element into another;
    // C copies the same `AVRational[]` with `av_memdup`. Each side then
    // writes one slot of its copy per step, and all three copies read alike
    // while the sources stay as they were.
    let values: Vec<Rational> = PAIRS.iter().map(|&(n, d)| Rational::new(n, d)).collect();
    let raw: Vec<ffi::AVRational> = PAIRS.iter().map(|&(n, d)| c_make(n, d)).collect();
    let mut from_slice = HeapVec::from_slice(&values).unwrap();
    let run = from_slice.as_handles();
    let mut from_run = alloc_array_from_fn(run.len(), |i| run.get(i).unwrap().get()).unwrap();
    let bytes = raw.len() * size_of::<ffi::AVRational>();
    // SAFETY: `raw` is `bytes` readable bytes for the call, which `av_memdup`
    // only reads; the result is a fresh block of the same size.
    let c = unsafe { ffi::av_memdup(raw.as_ptr().cast(), bytes) }.cast::<ffi::AVRational>();
    assert!(!c.is_null());
    let len = raw.len();
    for step in 0..len {
        let slot = (step * 7) % len;
        let next = c_make(raw[(step + 1) % len].den, raw[step].num.wrapping_neg());
        from_slice
            .as_handles_mut()
            .get_mut(slot)
            .unwrap()
            .set(next.into());
        from_run
            .as_handles_mut()
            .get_mut(slot)
            .unwrap()
            .set_num(next.num);
        from_run
            .as_handles_mut()
            .get_mut(slot)
            .unwrap()
            .set_den(next.den);
        // SAFETY: as the comment below states, for this part of the operation.
        let c_add_2 = unsafe { c.add(slot) };
        // SAFETY: `slot` is in bounds of the live C block, owned here.
        unsafe { c_add_2.write(next) };
        for i in 0..len {
            // SAFETY: as the comment below states, for this part of the operation.
            let c_add = unsafe { c.add(i) };
            // SAFETY: `i` is in bounds of the live, initialised C block.
            let ci = unsafe { c_add.read() };
            let expect = c_reads(ci);
            assert_eq!(
                rust_reads(from_slice.as_handles().get(i).unwrap()),
                expect,
                "step {step} slot {i}"
            );
            assert_eq!(
                rust_reads(from_run.as_handles().get(i).unwrap()),
                expect,
                "step {step} slot {i}"
            );
            assert_eq!(
                rust_reads(values[i].as_ref()),
                c_reads(raw[i]),
                "step {step} source {i}"
            );
        }
    }
    // SAFETY: `c` is the live `av_memdup` block allocated above.
    unsafe { ffi::av_free(c.cast::<c_void>()) };
}

// ---- av_reduce ----

/// Equivalence of `libavutil/rational.h`'s [`reduce`] with `av_reduce`, on
/// every argument the wrapper passes on.
mod reduce {
    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::rational::reduce;

    /// `av_reduce` in C, on its own out-parameters: the numerator,
    /// denominator and exactness.
    fn c_reduce(num: i64, den: i64, max: i64) -> (i32, i32, bool) {
        let (mut n, mut d) = (i32::MIN, i32::MIN);
        // SAFETY: both out-parameters are live locals; the tests pass `num` and
        // `den` other than `INT64_MIN` and `max` in `0..=INT_MAX`, for which C's
        // arithmetic does not overflow.
        let exact = unsafe { ffi::av_reduce(&raw mut n, &raw mut d, num, den, max) };
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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

/// Arithmetic, comparison and conversion (core wave 1, batch 2): each safe
/// operation on Rust values against the C function on independently built C
/// values, alone and chained, compared after every step.
mod arithmetic {
    use core::cmp::Ordering;

    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::rational::Rational;

    /// Every pair class the operations branch on: zero and infinite parts,
    /// signs on either side, the `int` extremes, unreduced fractions and
    /// common time bases.
    const PAIRS: [(i32, i32); 22] = [
        (0, 0),
        (1, 0),
        (-1, 0),
        (0, 1),
        (0, -7),
        (1, 1),
        (6, 4),
        (-6, 4),
        (6, -4),
        (-6, -4),
        (30000, 1001),
        (24000, 1001),
        (1, 90000),
        (1, 1000),
        (i32::MAX, 1),
        (i32::MIN, 1),
        (1, i32::MAX),
        (1, i32::MIN),
        (i32::MIN, i32::MAX),
        (i32::MAX, i32::MIN),
        (i32::MIN, i32::MIN),
        (i32::MAX - 1, i32::MAX),
    ];

    fn rust(num: i32, den: i32) -> Rational {
        Rational::new(num, den)
    }

    fn c(num: i32, den: i32) -> ffi::AVRational {
        ffi::AVRational { num, den }
    }

    fn parts(r: Rational) -> (i32, i32) {
        (r.as_ref().num(), r.as_ref().den())
    }

    fn c_parts(r: ffi::AVRational) -> (i32, i32) {
        (r.num, r.den)
    }

    /// Whether C's `av_add_q` overflows on `b + c` (both `INT_MIN/INT_MIN`).
    fn add_overflows(b: (i32, i32), c: (i32, i32)) -> bool {
        [b.0, b.1, c.0, c.1] == [i32::MIN; 4]
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn operators_are_the_c_functions() {
        for b in PAIRS {
            for k in PAIRS {
                let (rb, rk) = (rust(b.0, b.1), rust(k.0, k.1));
                let (cb, ck) = (c(b.0, b.1), c(k.0, k.1));
                // For the unsafe operations below: each takes its structs by value; for `int` terms
                // only `av_add_q` of two `INT_MIN/INT_MIN` overflows, which is
                // skipped.
                {
                    // SAFETY: as the comment above this block states.
                    unsafe {
                        if !add_overflows(b, k) {
                            assert_eq!(
                                parts(rb + rk),
                                c_parts(ffi::av_add_q(cb, ck)),
                                "{b:?}+{k:?}"
                            );
                        }
                    }
                    // SAFETY: as the comment above this block states.
                    unsafe {
                        assert_eq!(
                            parts(rb - rk),
                            c_parts(ffi::av_sub_q(cb, ck)),
                            "{b:?}-{k:?}"
                        );
                    };
                    // SAFETY: as the comment above this block states.
                    unsafe {
                        assert_eq!(
                            parts(rb * rk),
                            c_parts(ffi::av_mul_q(cb, ck)),
                            "{b:?}*{k:?}"
                        );
                    };
                    // SAFETY: as the comment above this block states.
                    unsafe {
                        assert_eq!(
                            parts(rb / rk),
                            c_parts(ffi::av_div_q(cb, ck)),
                            "{b:?}/{k:?}"
                        );
                    };
                }
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn order_is_av_cmp_q() {
        for b in PAIRS {
            for k in PAIRS {
                let (rb, rk) = (rust(b.0, b.1), rust(k.0, k.1));
                // SAFETY: `av_cmp_q` takes the structs by value and handles
                // every `int` term.
                let cmp = unsafe { ffi::av_cmp_q(c(b.0, b.1), c(k.0, k.1)) };
                let want = match cmp {
                    -1 => Some(Ordering::Less),
                    0 => Some(Ordering::Equal),
                    1 => Some(Ordering::Greater),
                    i32::MIN => None,
                    other => panic!("av_cmp_q returned {other}"),
                };
                assert_eq!(rb.partial_cmp(&rk), want, "{b:?} vs {k:?}");
                assert_eq!(rb == rk, cmp == 0, "{b:?} == {k:?}");
                assert_eq!(rb < rk, cmp == -1);
                assert_eq!(rb >= rk, cmp == 0 || cmp == 1);
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn conversions_are_the_c_functions() {
        for b in PAIRS {
            let r = rust(b.0, b.1);
            // SAFETY: both take the struct by value and have no
            // preconditions.
            let (inv, d) = (unsafe { ffi::av_inv_q(c(b.0, b.1)) }, unsafe {
                ffi::av_q2d(c(b.0, b.1))
            });
            assert_eq!(parts(r.recip()), c_parts(inv));
            assert_eq!(r.to_f64().to_bits(), d.to_bits(), "{b:?}");
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn from_f64_is_av_d2q() {
        let values = [
            0.0,
            -0.0,
            0.5,
            -0.25,
            1.0 / 3.0,
            29.97,
            23.976,
            core::f64::consts::PI,
            -core::f64::consts::E,
            1e-12,
            -1e-300,
            f64::MIN_POSITIVE,
            2_147_483_647.0,
            2_147_483_650.0,
            2_147_483_651.0,
            -2_147_483_650.5,
            1e300,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NAN,
        ];
        for d in values {
            for max in [0, 1, 100, 1001, 65535, i32::MAX] {
                // SAFETY: `av_d2q` takes its arguments by value; `max` is
                // non-negative.
                let c = unsafe { ffi::av_d2q(d, max) };
                let r = Rational::from_f64(d, max).unwrap();
                assert_eq!(parts(r), c_parts(c), "{d} within {max}");
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn f32_bits_are_av_q2intfloat_on_non_negative_values() {
        for b in PAIRS {
            if b.0 == i32::MIN || b.1 == i32::MIN {
                continue;
            }
            // C overflows shifting a negative value's sign bit; its other
            // bits come from the magnitude, so compare those and the sign.
            let (num, den) = if b.1 < 0 { (-b.0, -b.1) } else { b };
            let (mag, sign) = if num < 0 { (-num, 1 << 31) } else { (num, 0) };
            // SAFETY: `av_q2intfloat` takes the struct by value; both parts
            // are non-negative, so it neither negates nor shifts a sign.
            let c = unsafe { ffi::av_q2intfloat(c(mag, den)) };
            assert_eq!(rust(b.0, b.1).to_f32_bits(), Some(c | sign), "{b:?}");
            assert_eq!(rust(mag, den).to_f32_bits(), Some(c), "{b:?}");
        }
    }

    /// `av_gcd_q(a, b, max, def)`, or `None` when C returned `def`: told
    /// apart by calling with two different defaults.
    fn c_gcd(a: ffi::AVRational, b: ffi::AVRational, max: i32) -> Option<(i32, i32)> {
        let (d1, d2) = (c(-7, 3), c(5, -11));
        // SAFETY: `av_gcd_q` takes the structs by value; the callers never
        // pass two zero denominators, the one case it divides by zero.
        let (r1, r2) = (unsafe { ffi::av_gcd_q(a, b, max, d1) }, unsafe {
            ffi::av_gcd_q(a, b, max, d2)
        });
        (c_parts(r1) == c_parts(r2)).then_some(c_parts(r1))
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn gcd_is_av_gcd_q() {
        for b in PAIRS {
            for k in PAIRS {
                let (rb, rk) = (rust(b.0, b.1), rust(k.0, k.1));
                for max in [i32::MIN, -1, 0, 1, 120, 1001, 90000, i32::MAX] {
                    if b.1 == 0 && k.1 == 0 {
                        assert_eq!(rb.gcd(rk, max), None);
                        continue;
                    }
                    let want = c_gcd(c(b.0, b.1), c(k.0, k.1), max);
                    assert_eq!(rb.gcd(rk, max).map(parts), want, "{b:?} {k:?} {max}");
                }
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn cmp_distance_is_av_nearer_q() {
        for q in PAIRS {
            for a in PAIRS {
                for b in PAIRS {
                    let (ra, rb) = (rust(a.0, a.1), rust(b.0, b.1));
                    let got = rust(q.0, q.1).cmp_distance(ra, rb);
                    if (a.1 == i32::MIN && b.1 == i32::MIN) || rb.partial_cmp(&ra).is_none() {
                        assert_eq!(got, None, "{q:?} {a:?} {b:?}");
                        continue;
                    }
                    // SAFETY: `av_nearer_q` takes the structs by value; the
                    // arguments it overflows on are skipped above.
                    let c = unsafe { ffi::av_nearer_q(c(q.0, q.1), c(a.0, a.1), c(b.0, b.1)) };
                    let want = match c {
                        1 => Ordering::Less,
                        0 => Ordering::Equal,
                        -1 => Ordering::Greater,
                        other => panic!("av_nearer_q returned {other}"),
                    };
                    assert_eq!(got, Some(want), "{q:?} {a:?} {b:?}");
                }
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn nearest_is_av_find_nearest_q_idx() {
        let lists: [&[(i32, i32)]; 8] = [
            &[
                (24000, 1001),
                (24, 1),
                (25, 1),
                (30000, 1001),
                (30, 1),
                (50, 1),
                (60000, 1001),
                (60, 1),
            ],
            &[(1, 1)],
            &[(3, 4), (1, 4), (3, 4)],
            &[(-1, 2), (1, -3), (7, 5)],
            &[(i32::MAX, 1), (i32::MIN, 1), (1, i32::MAX)],
            &[(1, 3), (1, i32::MIN), (2, 3)],
            &[(5, 1), (6, 1), (0, 9), (4, 1)],
            &[(0, 5), (0, -5), (1, 1)],
        ];
        for list in lists {
            let candidates: Vec<Rational> = list.iter().map(|&(n, d)| rust(n, d)).collect();
            let mut terminated: Vec<ffi::AVRational> = list.iter().map(|&(n, d)| c(n, d)).collect();
            terminated.push(c(0, 0));
            for q in PAIRS {
                // SAFETY: `terminated` ends in `{0, 0}`; C reads it up to the
                // first zero denominator and never writes it. No list holds
                // two `INT_MIN` denominators or one first, which C overflows
                // on.
                let c = unsafe { ffi::av_find_nearest_q_idx(c(q.0, q.1), terminated.as_ptr()) };
                assert_eq!(
                    rust(q.0, q.1).nearest(&candidates),
                    Some(c as usize),
                    "{q:?} in {list:?}"
                );
            }
        }
    }

    /// Whether C's `av_find_nearest_q_idx` loop over `list` reaches a
    /// comparison of two `INT_MIN` denominators, which it overflows on: C's
    /// loop, stopped just before that pair.
    fn c_compares_two_min_dens(q: ffi::AVRational, list: &[ffi::AVRational]) -> bool {
        let mut nearest = 0;
        for (i, &cand) in list.iter().enumerate() {
            if cand.den == i32::MIN && list[nearest].den == i32::MIN {
                return true;
            }
            // SAFETY: `av_nearer_q` takes the structs by value; the pair it
            // overflows on was returned above, and no denominator is zero,
            // so `av_cmp_q` never reports `INT_MIN`.
            if unsafe { ffi::av_nearer_q(q, cand, list[nearest]) } > 0 {
                nearest = i;
            }
        }
        false
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn nearest_walks_two_min_denominators_as_av_find_nearest_q_idx() {
        let m = (1, i32::MIN);
        let lists: [&[(i32, i32)]; 6] = [
            &[(1, 3), m, (2, 3), m],
            &[(-5, 1), m, m],
            &[(-5, 1), m, (-7, 1), m],
            &[(1, 3), m, m],
            &[
                (0, 1),
                (3, i32::MIN),
                (1, 1),
                (-1, 1),
                (i32::MAX, i32::MIN),
                (1, -1),
            ],
            &[(i32::MIN, 1), (i32::MIN, i32::MIN), (i32::MIN, i32::MIN)],
        ];
        let (mut finished, mut overflowed) = (0, 0);
        for list in lists {
            let candidates: Vec<Rational> = list.iter().map(|&(n, d)| rust(n, d)).collect();
            let mut terminated: Vec<ffi::AVRational> = list.iter().map(|&(n, d)| c(n, d)).collect();
            terminated.push(c(0, 0));
            for q in PAIRS {
                let got = rust(q.0, q.1).nearest(&candidates);
                if c_compares_two_min_dens(c(q.0, q.1), &terminated[..list.len()]) {
                    assert_eq!(got, None, "{q:?} in {list:?}");
                    overflowed += 1;
                    continue;
                }
                // SAFETY: `terminated` ends in `{0, 0}`, the only entry with
                // a zero denominator, which C reads up to and never writes;
                // the walk above found no comparison C overflows on.
                let want = unsafe { ffi::av_find_nearest_q_idx(c(q.0, q.1), terminated.as_ptr()) };
                assert_eq!(got, Some(want as usize), "{q:?} in {list:?}");
                finished += 1;
            }
        }
        // Both outcomes are exercised.
        assert!(finished > 0 && overflowed > 0, "{finished} {overflowed}");
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn chained_sums_and_products_agree_after_every_step() {
        let steps = [
            (1, 3),
            (2, 7),
            (-5, 11),
            (1, 90000),
            (30000, 1001),
            (i32::MAX, 3),
            (7, -2),
        ];
        let (mut r_sum, mut c_sum) = (rust(0, 1), c(0, 1));
        let (mut r_prod, mut c_prod) = (rust(1, 1), c(1, 1));
        for (i, &(n, d)) in steps.iter().cycle().take(40).enumerate() {
            r_sum = r_sum + rust(n, d) - rust(d, 17);
            r_prod = r_prod * rust(n, d) / rust(d, n);
            // For the unsafe operations below: each takes its structs by value; no step adds two
            // `INT_MIN/INT_MIN`.
            {
                // SAFETY: as the comment below states, for this part of the operation.
                let add_q = unsafe { ffi::av_add_q(c_sum, c(n, d)) };
                // SAFETY: as the comment above this block states.
                unsafe { c_sum = ffi::av_sub_q(add_q, c(d, 17)) };
                // SAFETY: as the comment below states, for this part of the operation.
                let mul_q = unsafe { ffi::av_mul_q(c_prod, c(n, d)) };
                // SAFETY: as the comment above this block states.
                unsafe { c_prod = ffi::av_div_q(mul_q, c(d, n)) };
            }
            assert_eq!(parts(r_sum), c_parts(c_sum), "step {i}");
            assert_eq!(parts(r_prod), c_parts(c_prod), "step {i}");
        }
    }
}
