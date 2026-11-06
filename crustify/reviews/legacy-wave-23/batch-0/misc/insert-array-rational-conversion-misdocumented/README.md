# `insert_array` / `replace_array` documented rationals as converted "as by `set_rational`"

- File: `crustify/rust/ffmpeg/libavutil/opt.rs`
- Affected commit: `c771c3e8bd` (translation batch 1)
- Items: `av_opt_set_array` (`ObjectMut::insert_array`,
  `ObjectMut::replace_array`), `av_opt_set_q` (`ObjectMut::set_rational`)
- Regression: `tests/equiv/opt.rs`,
  `rational_array_conversion::rationals_into_integer_arrays_and_options_convert_as_c`

## Finding

`insert_array`'s documentation said each value "is converted as the scalar
setter of its type would", a rational "as by `set_rational`". The wrappers
pass the values to C, so behaviour matches C; the documentation did not.
`av_opt_set_q` calls `write_number(obj, o, dst, num, den, 1)`, which stores
`llrint(num / den)` into an integer option, but `av_opt_set_array` reads a
`RATIONAL` value as `intnum = num; den = den` with `num` left at 1
(`opt.c:2480`) and calls `write_number(obj, o, dst, 1, den, num)`, which
stores `llrint(1 / den) * num` into an integer, flags, boolean, duration or
format element. A C probe on the plain build:

```console
array ret 0: 0 0 0        # av_opt_set_array of {3/2, 7/2, -9/4} into an int array
scalar 3/2 -> 2           # av_opt_set_q of the same values into an int option
scalar 7/2 -> 4
scalar -9/4 -> -2
```

A rational or video rate element still stores the pair as is, and a float
or double element the quotient (`num * intnum / den`), so only integer-like
elements differ.

The same mix-up sat in `numeric_elements!`, whose `Rational` arm previewed
`write_number` with C's scalar operands `(num, den, 1)` instead of the
array's `(1, den, num)`. Its verdict was the same for every value (the
preview only refuses rational and video rate targets, where both operand
orders give `value = num` and `(int) 1` / `(int) num` both fit), so this was
not a safety defect, but the macro claims to pass C's operands.

## Fix

Documentation only for the behaviour, which stays C's: `insert_array` now
says how C converts a rational (stored as is into rational and video rate
elements, the quotient into float and double ones, `llrint(1 / den) * num`
into the integer-like ones, `0` for `3/2` where `set_rational` stores `2`),
and the `Rational` arm of `numeric_elements!` passes C's array operands
`(1.0, den, num)`. The new equivalence test inserts `3/2, 7/2, -9/4, 5/1,
6/1` into the `int` and `int64` arrays of independently owned C and Rust
objects and compares C's `av_opt_get_array` with `get_array::<i64>` (both
`[0, 0, 0, 5, 6]`), then stores each value with `av_opt_set_q` and
`set_rational` into an `int` option and compares the results and errors.

Correcting the conversion in Rust (passing a rational as the `DOUBLE`
quotient to integer-like arrays) would diverge from C and from C's own range
check, which multiplies the bounds by `den` (a negative `den` already
inverts it in both setters); it is left as C does it.
