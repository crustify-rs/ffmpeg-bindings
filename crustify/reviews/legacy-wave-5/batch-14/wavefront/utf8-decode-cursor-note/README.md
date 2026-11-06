# Wavefront: `av_utf8_decode.bufp` note understated what a malformed sequence consumes

- Store: `crustify/wavefront/ownership-store.json`, record `av_utf8_decode`
  (`libavutil/avstring.c`), `ptr_args` position 1 (`bufp`)
- Affected commit: `083c4d43d0` (core wave 0, batch 14), which submitted
  the record
- Item: `av_utf8_decode` (symbol route, wrap)

## Finding

The structured slots (`scalar.by_ref` borrowed cursor, `string: false`,
`nullable: false`, `mutable: false` for the const innermost pointee) are
correct. The note's description of how far the cursor advances, "1 for an
invalid lead or continuation byte, the whole sequence otherwise", omits one
shape and misstates another against `libavutil/avstring.c:364-438`:

- A sequence `buf_end` cuts short (`E2 82` at the end of the buffer) is not
  an invalid continuation byte, so the note's "otherwise" clause puts it in
  the whole-sequence group; C does `(*bufp)++` and returns at line 393-394,
  consuming 1.
- "1 for an invalid ... continuation byte" does not say that the good tail
  bytes read before the bad one are not consumed either: `E2 82 41`
  consumes 1 (line 400-401), not 2.
- The whole-sequence group includes the overlong and `>= 2^31` encodings
  (lines 410-418 `goto end`, `*bufp = p`), which leave `*codep` untouched;
  the note did not say they consume the whole sequence while the `codep`
  note calls them "untouched", which reads as if they were short-circuited
  like the 1-byte cases.

The Rust doc written from this note carried the same gap (see
`../../misc/utf8-decode-consumption-doc/`).

## Fix

Resubmitted with `wavefront query symbols --name av_utf8_decode --file
libavutil/avstring.c --update submission.json` (this directory's
`submission.json`): the `bufp` note now enumerates the three 1-byte shapes
with the `E2 82 41` and `E2 82` examples, states that the whole sequence is
consumed otherwise, overlong and `>= 2^31` encodings included (`E0 80 80`
consumes 3 with `*codep` untouched), and keeps the empty-buffer and
const-pointee clauses. The other two slots and the symbol summary are
resubmitted unchanged.
