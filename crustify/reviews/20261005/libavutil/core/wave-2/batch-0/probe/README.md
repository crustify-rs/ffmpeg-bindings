# Differential probe: random appender sequences, C against Rust

Not a defect reproducer: the sweep that found none. `cargo run --offline --
<iterations> <seed>` builds a C `AVBPrint` (raw `ffi` calls on leaked `Box`
storage) and a Rust `BPrint` of the same random shape (`av_bprint_init` with
`size_init`/`size_max` from an edge set, or `av_bprint_init_for_buffer` on
caller buffers of 0 to 5000 bytes), applies 1 to 25 random steps to both,
and compares `(len, size, size_max, inline, complete, stored bytes, string
up to NUL)` and any returned count after every step; owned buffers are then
finalized through `av_bprint_finalize(buf, &str)` against `into_bytes` or
`into_string`, caller buffers compared after both owners drop.

Steps: `av_bprint_append_data` (binary data with NULs, up to 3000 bytes),
`av_bprint_chars` (0 to 5000 copies, around the inline size), `av_bprint_clear`,
the `len`-lowering truncation C callers write, `av_bprintf("%d-%s")` against
`append_fmt`, `av_vbprintf("%s")` through the shim against `append_c_str`,
`av_bprint_escape` over every mode, all 16 flag sets and three kinds of
special list, `av_bprint_strftime` over ten formats (empty, `%Z`, `%2000c`,
twenty `%c`s), `av_bprint_get_buffer` committed as `pngdec.c` does against
`append_with` (0 to `u32::MAX` requested), and `av_max_alloc` limits from 0 to
`INT_MAX` switched mid-sequence so growth, the strftime fallbacks and the
finalizers fail on both sides alike.

Results on the affected base `9ff8fb29d7ea1c3174abaac31e506c229f3f9115`:
20 000 sequences on the plain build (seed `0x9e3779b97f4a7c15`) and 15 000
under ASan/UBSan against the `asan-ubsan` library (seed `12345`,
`RUSTFLAGS=-Zsanitizer=address cargo +nightly build -Zbuild-std`): no
mismatch, no sanitizer report, no `runtime error`, no leak.
