# Equivalence: `Rational::nearest` refused lists `av_find_nearest_q_idx` finishes

- File: `crustify/rust/ffmpeg/libavutil/rational.rs`
- Affected commit: `23dc07587c` ("wrap libavutil core wave-1 batch 2 (rational,
  rescaling, names, escape)")
- Item: `Rational::nearest` (`av_find_nearest_q_idx`)
- Kind: over-rejection of well-defined C input (the wrapper returns `None`
  where C returns an index without undefined behaviour)

## Finding

`av_find_nearest_q_idx` compares its first candidate with itself and every
later candidate with the nearest so far, through `av_nearer_q`, which
overflows `2 * (int64_t)q1.den * q2.den` only when both denominators are
`INT_MIN`. The wrapper guarded that overflow by refusing any list whose first
candidate has an `INT_MIN` denominator (right: C compares it with itself) and
any list holding two such candidates anywhere (wrong: C compares the second
with the nearest so far, which is the first only when that one took over).

Its doc stated the wrong rule as C's ("`None` too when C would overflow
comparing two candidates whose denominators are both `i32::MIN`: when the
first compared candidate has one or two of them do"), and the safety comment
relied on it.

A differential replay of the wrapper's refusals under the asan-ubsan C build
(each refused input in its own process, `UBSAN_OPTIONS=halt_on_error=1`,
because UBSan reports each location once per process) found 37 of 150 sampled
refused `nearest` lists finishing in C without any report; every refused
`cmp_distance`, `to_f32_bits`, `gcd`, `add_stable` and `rescale_delta` input in
the same replay (150 each, all 266 for `cmp_distance`) did trigger a report, so
the other guards of the batch are exact.

Trace of `cargo run` in this directory on the affected commit (exit code 1):

```text
(1, 3) in [(1, 3), (1, -2147483648), (2, 3), (1, -2147483648)]: C 0, Rust None: DIFFER
(1, 2) in [(1, 3), (1, -2147483648), (1, -2147483648)]: C 0, Rust None: DIFFER
(1, 3) in [(2, 3), (1, -2147483648), (1, 3), (1, -2147483648)]: C 2, Rust None: DIFFER
```

With the fix every line reads `agree` and the program exits 0. The C calls
run clean under the asan-ubsan build (the equivalence regression below runs
under it).

## Fix

`nearest` keeps the cheap exact rules (empty or terminated-first list, first
candidate with an `INT_MIN` denominator) and, only when the list holds two or
more such candidates, walks C's loop with `cmp_distance` (C's own
`av_nearer_q`, one FFI call per step, `None` exactly for the overflowing pair)
before calling `av_find_nearest_q_idx`. `None` now means "C would overflow"
and nothing else; the doc and the safety comment say so.

`cmp_distance`'s doc also overstated its unordered case ("overflows computing
one"): there C multiplies `INT_MIN` by -1, 0 or 1, an overflow only for -1, 0
or `INT_MIN` otherwise, none of them a verdict. The doc now says that; the
`None` result is unchanged.

## Regression

- `tests/equiv/rational.rs`:
  `nearest_walks_two_min_denominators_as_av_find_nearest_q_idx` walks six
  such lists against every edge rational with a C-driven oracle
  (`c_compares_two_min_dens`, C's loop stopped before the overflowing pair)
  and compares with `av_find_nearest_q_idx` wherever C finishes, asserting
  `None` where it would overflow; both outcomes are reached.
- `libavutil/rational.rs` `unit_tests`:
  `nearest_rejects_two_min_denominators_only_when_c_compares_them` pins the
  rule on `-5/1` (which `1/INT_MIN` takes over from) and `1/3` (which it
  does not).
