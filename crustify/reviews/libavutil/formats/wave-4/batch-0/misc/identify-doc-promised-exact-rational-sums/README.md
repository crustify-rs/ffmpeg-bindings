# `identify`'s doc promised an exact rational sum; C reduces it to `int`

- File: `crustify/rust/ffmpeg/libavutil/csp.rs`
- Affected commit: `7cf5d6fc5d` (the wave tip)
- Item: `ColorPrimariesDescriptorRef::identify` (`Wraps:
  av_csp_primaries_id_from_desc`)

The doc said the gamut is "the first entry of C's table ... for which the
sum of the absolute differences of all eight coordinates, computed exactly
as rationals, is below 1/1000". The wrapper calls C, and C's sum is not
exact: `abs_sub_q` / `av_add_q` pass every difference and partial sum
through `av_reduce(..., INT_MAX)`, which is exact only while the result's
parts fit an `int`. The table coordinates have denominators 100000 (and 3
for SMPTE 428's white point), so a coordinate whose denominator exceeds
`INT_MAX / 100000 = 21474` makes the exact difference's denominator
overflow, and `av_reduce` replaces it by the closest rational with parts
`<= INT_MAX`: within about `2e-13` of the exact value near 1/1000 (the
representable neighbours of 1/1000 at that bound, `2147483/2147483001`
and `2147483/2147482999`, lie `4.66e-13` away).

A description whose exact distance is between 1/1000 and that rounding
error therefore gets C's verdict, not exact arithmetic's. `probe/exact.py`
(ctypes over the plain `libavutil`, an exact `Fraction` model) finds, in
200k random one-coordinate placements, three BT.709 descriptions that the
exact model identifies as BT.709 while C returns `AVCOL_PRI_UNSPECIFIED`
(`probe/output.txt`):

```
wp.x = 311418082/999095547    exact distance 0.0009999999998999 (1.0e-13 inside 1/1000)  exact: BT709  C: UNSPECIFIED
wp.x = 322460225/1034521094   exact distance 0.0009999999998067 (1.9e-13 inside)          exact: BT709  C: UNSPECIFIED
wp.x = 528928917/1696916641   exact distance 0.0009999999998232 (1.8e-13 inside)          exact: BT709  C: UNSPECIFIED
```

For each, `av_reduce` picks `1/1000` itself (the convergent, closer than
the semiconvergent `2147483/2147483001`), and `av_cmp_q(1/1000, 1/1000)`
is not `< 0`.

The wrapper's behaviour is right (it must and does match C; the
equivalence suite's 4800 random edit steps and the tolerance sweep never
disagreed with C), so only the doc changes: it now says the sum is C's, in
`int` rationals, exact for the tables' denominators and approximate to
about `2e-13` once a coordinate's denominator makes the exact one exceed
`i32::MAX`, with the first fixture above as the example. The same paragraph
also overstated the zero-denominator divergence ("Diverges from C for a
description with a zero denominator"): C agrees for a single infinite
coordinate (its difference stays `1/0`, which `av_cmp_q` orders above
1/1000); only a `0/0` coordinate or two infinite ones diverge, as the
following sentence already explained. The doc now says so.

Regression: `tests/equiv/csp.rs::primaries_lookup::reduced_sums_follow_c_not_exact_arithmetic`
places one coordinate of BT.709 at a random denominator in
`100001..=i32::MAX` one part short of, at, or past 1/1000 away (3000
rounds plus the three fixtures), asserts the wrapper's verdict equals C's
for each, and that at least the three fixtures are "inside" for an
`i128` exact model while C and the wrapper answer `None`.

The wavefront note for `av_csp_primaries_id_from_desc` does not make the
exactness claim and needs no resubmission.
