# Misc: `Level`'s doc overstated what its order decides

- File: `crustify/rust/ffmpeg/libavutil/log.rs`
- Affected commit: `5d0c0b3b31`
- Item: `Level` (the `int` of `av_log_set_level` / `av_log_get_level` and
  the log functions)

## Finding

`Level` derives `Ord` on the raw `int`, and its doc said "The order is C's:
a message is printed by the default callback when its level is at most
`level()`". That holds for the plain levels, but `av_log_default_callback`
first splits a non-negative level into its tint and its low byte
(`tint = level & 0xff00; level &= 0xff`) and compares the low byte with the
threshold. A tinted level such as `AV_LOG_C(134) | AV_LOG_ERROR` (34320) is
printed at the default `INFO` threshold (32), while `Level::from_raw(34320)
<= Level::INFO` is `false`. A Rust callback mimicking the default filter with
the derived order would drop tinted messages the default callback prints.
The doc's previous sentence mentions the tint bits, but not that the
comparison ignores them.

The behaviour itself is C's and the equivalence suite covers tinted levels
(`LEVELS` in `tests/equiv/log.rs`); only the doc was off.

## Fix

The doc now states that the default callback compares a non-negative level
with its tint bits cleared, which the derived order does not.
