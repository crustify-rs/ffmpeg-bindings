# Misc: `IntFloat32` had no equality, and `Alias64` was `Eq` without `Hash`

- Files: `crustify/rust/ffmpeg/libavutil/intfloat.rs`,
  `crustify/rust/ffmpeg/libavutil/intreadwrite.rs`
- Affected commit: `d96b454a0c` ("wrap libavutil av_alias64 and av_intfloat32")
- Items: `av_intfloat32` (`IntFloat32`), `av_alias64` (`Alias64`)

## Finding

Both items are plain `Copy` punning unions whose every bit pattern is a
valid value of every member, and C compares them only as words
(`memcmp`, `==` on `.u64` / `.i`). The wave's other punning unions expose
that: `Alias16`, `Alias32` and `Alias64` implement bitwise `PartialEq` /
`Eq` ("a NaN equals itself, and `0.0` differs from `-0.0`"), and
`Unaligned16` / `Unaligned32` / `Unaligned64` add `Hash` and `From` on top.
The two items of this batch broke the pattern:

- `IntFloat32` implemented neither `PartialEq` nor `Hash`, so a consumer
  could not `assert_eq!` two values or key a map by one, and the crate's own
  tests compared `a.as_ref().bits()` by hand (`intfloat32_setters_and_copies`,
  `intfloat32_members_reinterpret_each_other`). Its 64-bit sibling
  `IntFloat64`, outside this batch, has the same gap.
- `Alias64` was `Eq` but not `Hash`, so it could be compared but not stored
  in a `HashSet` / `HashMap`, unlike `Unaligned64`.

Neither is a soundness or equivalence defect: nothing C-observable is
involved. It is an inconsistent safe surface across one family of types.

## Fix

- `IntFloat32`: bitwise `PartialEq` / `Eq` (through `bits()`), and `Hash` of
  the same word, documented on the type and in the module docs.
- `Alias64`: `Hash` of the `u64` view, consistent with its `PartialEq`.
- Regression tests in `mod unit_tests`:
  `intfloat::unit_tests::intfloat32_equality_and_hash_are_bitwise` and
  `intreadwrite::unit_tests::alias64_hash_follows_the_bitwise_equality`
  check the NaN / signed-zero cases, that the hash equals the integer's, and
  `HashSet` membership.

`IntFloat64` (batch of `av_intfloat64`) is left for its own review.
