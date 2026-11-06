# Misc: `Alias16` / `Alias32` implemented `Eq` but not `Hash`

- File: `crustify/rust/ffmpeg/libavutil/intreadwrite.rs`
- Affected commit: `5e81aab756`
- Items: `av_alias16` (`Alias16`), `av_alias32` (`Alias32`)

## Finding

Both values implement bitwise `PartialEq` / `Eq` (a NaN equals itself, `0.0`
differs from `-0.0`) but no `Hash`, so they cannot key a `HashMap` or sit in a
`HashSet`, while their `Unaligned16` / `Unaligned32` siblings in the same
module (landed later, in `058790db28`) hash through their integer. The value is
plain data with a total bitwise equality, so there is no reason to withhold
the trait, and leaving it off is the kind of ergonomic hole that pushes a
consumer towards the raw `ffi::av_alias32` instead.

## Fix

`impl Hash for Alias16` / `Alias32` hash the `u16` / `u32` view, so equal values
hash equally (`Hash`'s contract with `Eq`), and `0.0` / `-0.0` stay distinct
keys. `mod unit_tests::alias_hash_follows_the_integer_view` checks the hash
against the integer's and the `HashSet` behaviour.

## Not changed

`Alias64` (another batch's item) has the same omission; left for its review.
