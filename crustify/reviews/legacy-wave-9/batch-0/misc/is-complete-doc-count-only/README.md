# `BPrintRef::is_complete` documented a count-only buffer as complete until appended to

Affected base: `06a7548c5c439c49f36f434a7251d23939b53462` (core wave 2, batch 0).
Home: `crustify/rust/ffmpeg/libavutil/bprint.rs`, `BPrintRef::is_complete`
(`Wraps: av_bprint_is_complete`) and `BPrintMut::clear`.

## Finding

The wrapper's doc said `is_complete` is `false` "always for a count-only
buffer that was appended to", and `clear`'s doc said emptying "makes a
truncated buffer complete again". C's inline is `return buf->len < buf->size;`,
and a count-only buffer (`av_bprint_init(buf, 0, AV_BPRINT_SIZE_COUNT_ONLY)`,
or `av_bprint_init_for_buffer(buf, p, 0)`) has `size == 0`, so `0 < 0` is
false from the moment it is initialised; `clear` and `truncate` leave it
incomplete too. A C check on the plain build:

```
count-only fresh: len=0 size=0 complete=0
for_buffer(0) fresh: complete=0
```

The behaviour itself was already equivalent: `tests/equiv/bprint.rs` compares
`av_bprint_is_complete` with `is_complete` right after every `INITS` shape,
`(0, SIZE_COUNT_ONLY)` included, so the wrapper never diverged. Only the
contract its consumers read was wrong, and `tests/ub_safe/bprint.rs`'s
`check_empty` never asserted completeness on a fresh buffer, so nothing
caught it.

## Fix

Reworded both docs: a count-only buffer is never complete, not even before
anything is appended, and `clear` restores completeness only to a truncated
buffer with storage. The regression evidence is a doctest on `is_complete`
asserting a fresh count-only `BPrint::new`, one cleared after an append, a
`BPrint::for_buffer(&mut [])` and, for contrast, a fresh automatic buffer.
