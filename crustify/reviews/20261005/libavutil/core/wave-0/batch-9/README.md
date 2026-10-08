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
