# Review: libavutil core wave 0, batch 5 (`av_alias16`, `av_alias32`)

Objective `review` of the `type` route items `av_alias16` and `av_alias32`
(`crustify/rust/ffmpeg/libavutil/intreadwrite.rs`, wrapped in `5e81aab756`).

Findings with a fix and a report:

- `lifecycle/av-alias16`, `lifecycle/av-alias32`: no borrowed view over caller
  bytes, the form every C use of the unions takes (`AV_RN32A`, `AV_WN16A`,
  `AV_COPY32`, ...). Added `from_bytes` on the four handles, `None` when
  misaligned.
- `misc/alias-eq-without-hash`: `Eq` without `Hash`. Added `Hash`.

Verified with no finding (no report):

- wavefront: both type findings are complete and correct (no pointer fields,
  no lifecycle routines; `kind: struct` is the composer's spelling for a
  typedef'd anonymous union, as the submitted comment notes).
- unsafe boundary: the workset adds no public `unsafe fn`; the handles' raw
  seams stay `pub(crate)`; every `unsafe` block's `SAFETY` comment holds
  (aligned `read` / `write` through `addr_of!` of a handle whose every
  constructor, including the new `from_bytes`, guarantees the alignment).
- UB: no candidate reached a sanitizer or Miri; the `Send` / `Sync` proofs
  hold (plain data, writes only through `&mut Alias*Mut`).
- equivalence: the only observable behaviour is the layout and the
  native-order aliasing of the views, which the existing and the new suites
  compare with the raw unions and the aligned macro expansions after every
  step.
- conventions: anchors, naming, handle placement and projection follow the
  conventions.
