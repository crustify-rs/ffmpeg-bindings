# Equivalence: `parse_cpu_caps` rounded a `+`/`-` operand where C truncates it

- File: `crustify/rust/ffmpeg/libavutil/cpu.rs`
- Affected commit: `6978702856` ("wrap libavutil's realloc, scan, search,
  rational and saturating symbols")
- Item: `parse_cpu_caps` (`av_parse_cpu_caps`)
- Kind: functional divergence from C on well-defined input (no UB on either
  side)

## Finding

`av_parse_cpu_caps` evaluates a flags string through `set_string_number`
(`libavutil/opt.c`). For a token after `+` or `-` it evaluates the token to a
double `d`, converts it with `(int64_t)d` (truncation toward zero), ors the
result into the flag word or masks it out, and only then range-checks the
combination as a 32-bit flag set (`write_number`). A bare token is instead
range-checked on its own value, which must round within `-1.5..=2^32+0.5` and
within 1/512 of an integer, and is stored with `llrint` (round to nearest).

The wrapper never passed a prefixed token to C. It evaluated the token
through C's *unprefixed* path on a scratch word and combined the bits itself,
so every prefixed operand went through rounding and the unprefixed range
check instead of truncation:

- `-0.999`: C truncates to 0 and clears nothing; the wrapper rounded to 1
  and cleared bit 0.
- `+1.9995`: C truncates to 1, already set in `0x55`; the wrapper rounded to
  2 and set bit 1.
- `+2.5`, `-0.5`, `+PI`, `-5G`, `-min`, `-0x100000000`: C accepts each
  (`+2.5` sets bit 1, `+PI` sets bits 0 and 1, `-5G` clears the low 32 bits
  of 5·10⁹, the rest change nothing); the wrapper failed them with
  `AVERROR(ERANGE)`, and in a sequence such as `sse2+2.5-0.999` it stopped
  after `sse2`.

The doc comment acknowledged part of this ("C accepts some such tokens that
clear bits (`-0x100000000` and `+0.5` change nothing); the wrapper rejects
them") but was wrong twice over: the accepted operands do not merely "change
nothing", and operands within 1/512 of an integer were not rejected but
applied to the wrong bit. Every input above is well-defined in C: the
conversion only overflows at or beyond 2^63 in magnitude (`max`, `inf`,
`1e300`), which the wrapper rightly refuses.

Trace of `cargo run` in this directory on the affected commit (exit code 1):

```text
"-0.999" from 0x55: C Ok(()) -> 0x55, Rust Ok(()) -> 0x54: DIFFER
"+1.9995" from 0x55: C Ok(()) -> 0x55, Rust Ok(()) -> 0x57: DIFFER
"-1.9995" from 0x55: C Ok(()) -> 0x54, Rust Ok(()) -> 0x55: DIFFER
"+2.5" from 0x55: C Ok(()) -> 0x57, Rust Err(-34) -> 0x55: DIFFER
"-0.5" from 0x55: C Ok(()) -> 0x55, Rust Err(-34) -> 0x55: DIFFER
"+PI" from 0x55: C Ok(()) -> 0x57, Rust Err(-34) -> 0x55: DIFFER
"-5G" from 0x55: C Ok(()) -> 0x55, Rust Err(-34) -> 0x55: DIFFER
"-min" from 0x55: C Ok(()) -> 0x55, Rust Err(-34) -> 0x55: DIFFER
"-0x100000000" from 0x55: C Ok(()) -> 0x55, Rust Err(-34) -> 0x55: DIFFER
"sse2+2.5-0.999" from 0x55: C Ok(()) -> 0x12, Rust Err(-34) -> 0x10: DIFFER
```

With the fix every line reads `agree` and the program exits 0.

## Fix

`combine_flags` now applies a prefixed token as C does. The operand's value
comes from `expression_value`: `av_strtod` read whole (a number with C's SI
and `dB` suffixes), else one of the constants the option evaluator
(`default`, `max`, `min`, `none`, `all`) and the expression evaluator (`E`,
`PI`, `PHI`, `QP2LAMBDA`) define; anything else is a CPU-flag name, which
C's own unprefixed evaluation resolves to its exact bits, or a token C
rejects with its own error (a NaN included, so C logs it as it would). The
value is truncated to an `int64_t` when C's conversion is defined
(`-2^63 <= d < 2^63`), combined with the flag word, and the combination
range-checked as `write_number` does; a value C would overflow on fails with
`AVERROR(ERANGE)` as before. `av_strtod` is bound raw in `libavutil-sys`
(with the four evaluator constants) until `libavutil/eval.h` is wrapped.

Regression tests: `tests/equiv/cpu.rs`
(`parse_caps::fractional_wide_and_named_operands_match_c`,
`parse_caps::operand_sequences_match_c_after_each_step`) and the inline
`prefixed_operands_truncate_as_c_does` /
`expression_values_follow_av_strtod_and_the_evaluators_constants`. A
300 000-case differential run over tokens drawn from numbers, suffixes,
constants, CPU names and junk (`/tmp` probe, not committed) found no
remaining divergence under ASan/UBSan.
