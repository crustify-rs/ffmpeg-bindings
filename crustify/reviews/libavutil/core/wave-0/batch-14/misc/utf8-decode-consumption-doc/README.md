# Misc: `utf8_decode` misdescribed what a malformed sequence consumes

- File: `crustify/rust/ffmpeg/libavutil/avstring.rs`
- Affected commit: `ccdbbc0a77` (core wave 0, batch 14)
- Item: `av_utf8_decode` (symbol route, wrap), the `utf8_decode` wrapper

## Finding

The wrapper's contract is "the code point or why it was rejected, and the
number of bytes consumed", and a caller slices its buffer by that count to
resume. Its docs described the count as: "On a malformed sequence it
consumes up to the offending byte; on a well-formed one that `flags`
rejects, the whole sequence." That is not what C does
(`libavutil/avstring.c:364-438`), in two ways:

- A tail byte that is not `10xxxxxx`, and a sequence `buf_end` cuts short,
  make C do `(*bufp)++` and return: exactly one byte, however many good tail
  bytes it had already read. `E2 82 41` consumes 1 (the doc's "up to the
  offending byte" reads as 2), and the next call restarts at `82`, which is
  itself rejected, before `41` decodes. `E2 82` at the end of the buffer
  consumes 1 as well.
- An overlong encoding (`E0 80 80`) is malformed too, yet C consumes the
  whole sequence (`goto end` with `p` past it), with `*codep` untouched. The
  doc's rule gives 1.

A second sentence, "a code point may exceed U+10FFFF (but not `i32::MAX`)",
could be read as excluding `i32::MAX`, which the six-byte maximum
`FD BF BF BF BF BF` decodes to exactly (`code >= 1U<<31` is the only range
cut, and no sequence reaches it): `utf8_decode(.., ACCEPT_INVALID_BIG_CODES)`
returns `Ok(0x7fff_ffff)`.

The code matches C in every case; the existing equivalence tests
(`utf8_decode_steps_as_av_utf8_decode`, `utf8_decode_agrees_on_pseudo_random_bytes`)
already cover `E2 28 A1` and the overlong inputs step by step. Only the
documented rule was wrong, so this is a documentation defect: a consumer
implementing a resynchronising decoder from the doc alone would expect a
different cursor than the one it gets.

## Fix

The `utf8_decode` doc now states the rule C implements: one byte for an
invalid lead byte, for a bad tail byte after good ones (with the `E2 82 41`
example and where the next call restarts), and for a sequence the buffer
cuts short; the whole sequence otherwise, for an overlong encoding (no code
point) as for a well-formed sequence the flags reject (its code point). The
range sentence now says "up to `i32::MAX`". `#[must_use]` is added, as the
module's other pure queries carry.

Regression test `tests/equiv/avstring.rs`
`helpers::utf8_decode_consumes_as_c_on_each_malformed_shape` pins each shape
in the doc against C: the first step's `(ret, *codep, consumed)` for a lone
continuation byte, `E2 82 41`, `E2 82`, `E0 80 80`, `F4 90 80 80` and the
six-byte maximum with and without `AV_UTF8_FLAG_ACCEPT_INVALID_BIG_CODES`,
the full restart sequence after a bad tail byte, and
`Ok(i32::MAX as u32)` for the maximum.

The Wavefront `bufp` note had the same gap; see
`../../wavefront/utf8-decode-cursor-note/`.
