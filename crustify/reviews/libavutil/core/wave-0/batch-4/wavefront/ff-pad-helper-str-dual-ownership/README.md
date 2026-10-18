# Wavefront: `ff_pad_helper_AVBPrint.str` was submitted as owned only

- Store: `crustify/wavefront/ownership-store.json`, record
  `ff_pad_helper_AVBPrint` (`libavutil/bprint.h`)
- Affected commit: `ce6afb6d6c` (core wave 0, batch 4), which submitted the
  record on behalf of the `FF_PAD_STRUCTURE` item: the generator has no
  Wavefront record of its own, so the translator analysed the sizing helper
  it defines and mirrored `AVBPrint.str` onto it
- Item: `FF_PAD_STRUCTURE` (type route, wrap)

## Finding

The submitted block for `str` was `owned: true, borrowed: null`, with a note
that itself described two borrowed paths: "it otherwise borrows the struct
itself (automatic storage) or a caller buffer (av_bprint_init_for_buffer)".
The schema allows `owned` and `borrowed` to be set together for
"runtime-conditional dual ownership (owned on one path, borrowed on
another)", and that is exactly what `bprint.c` does, discriminated by
`av_bprint_is_allocated(buf)` (`str != reserved_internal_buffer`):

- `av_bprint_init` (`libavutil/bprint.c:69-83`) sets
  `buf->str = buf->reserved_internal_buffer`: a borrow of the structure's own
  storage (`lifetime: self`). With `size_max == AV_BPRINT_SIZE_AUTOMATIC` the
  buffer never allocates (`av_bprint_alloc` returns `EIO` once
  `size == size_max`), so the borrow lasts the whole lifetime.
- `av_bprint_init_for_buffer` (`bprint.c:85-98`) sets `buf->str = buffer`:
  a borrow of a caller buffer (`lifetime: other`).
- `av_bprint_alloc` (`bprint.c:36-58`) replaces it with `av_realloc`
  storage the structure owns, which `av_bprint_finalize(buf, NULL)` frees
  (`bprint.c:254-256`).

The note's last clause, "av_bprint_finalize(buf, &ret) leaves it NULL", was
also only true of the owned path: on the borrowed path (`bprint.c:247-252`)
finalize `av_memdup`s a copy into `*ret_str` and leaves `buf->str` pointing
at the borrowed storage. A wave-1 translator reading the structured slots
would have taken `str` for a plain `CBox`-style owner and missed the
automatic-storage case, which is the one `FF_PAD_STRUCTURE` exists for.

## Fix

Resubmitted through `wavefront query types --name ff_pad_helper_AVBPrint
--update` (submission kept at `submission.json` beside this report):

- `borrowed: {lifetime: "self"}` beside `owned: true`; the schema's single
  lifetime slot cannot also carry `other`, so the caller-buffer path is
  spelled out in the note;
- the note names the discriminator (`av_bprint_is_allocated`), each path's
  routine, and states that the field is NULL only after the owned path's
  finalize;
- `_comment_agent` records the correction.

No Rust code changes: the batch's `PaddedStruct` describes the layout only,
and `AVBPrint`'s ownership wrapper is wave 1's item.
