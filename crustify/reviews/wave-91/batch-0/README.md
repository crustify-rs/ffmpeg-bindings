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


---

<!-- merged from crustify/reviews/wave-31/batch-3 -->

# Review: libavutil/core wave-0 batch-9 (`unaligned_64`)

Workset: `unaligned_64` (type, field `l`), home
`crustify/rust/ffmpeg/libavutil/intreadwrite.rs`, objective `review`.

| step | outcome |
|---|---|
| wavefront | agent findings correct; composer `kind` misreports the union (`wavefront/`) |
| lifecycle | complete: no C lifecycle routine exists; value, byte view, `av_malloc` owner |
| unsafe | no public unsafe API; nothing to tighten |
| ub_safe / ub_unsafe | no defect; suites clean under ASan/UBSan, TSan, BSan, Miri |
| equiv | no defect; raw-union and `av_memcpy_backptr` references agree at every offset |
| internal | no defect |
| conventions | none |
| misc | `Unaligned64` lacked its siblings' value traits (`misc/`), fixed |
| coverage | Rust lines/regions/functions of `intreadwrite.rs` 100%; no instrumented C code belongs to the item (header-only union) |
