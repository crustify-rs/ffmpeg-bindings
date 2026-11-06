# Review: libavutil/core wave-0 batch-13 (realloc, scan, search, rational and saturating symbols)

Workset (symbol route, objective `review`): `av_parse_cpu_caps`,
`av_popcount_c`, `av_popcount64_c`, `av_random_bytes`, `av_realloc`,
`av_realloc_array`, `av_realloc_f`, `av_reallocp`, `av_reallocp_array`,
`av_reduce`, `av_rescale`, `av_sat_{add,sub,dadd,dsub}32_c`,
`av_sat_{add,sub}64_c`, `av_size_mult`, `av_sscanf`, `av_strcasecmp`,
`av_strdup`, `av_strerror`, `av_strireplace`, `av_stristart`, `av_stristr`;
homes `cpu.rs`, `common.rs`, `random_seed.rs`, `mem.rs`, `rational.rs`,
`mathematics.rs`, `avstring.rs`, `error.rs`.

| step | outcome |
|---|---|
| wavefront | findings correct (ownership, nullability, the realloc family's failure semantics, `av_stristart`'s const out-slot); the `av_parse_cpu_caps` note restated for the fixed wrapper |
| lifecycle | no type or raw-lifetime item; `av_strdup` stays the string tier's clone strategy |
| unsafe | no public unsafe API added; every raw pointer is a local FFI seam inside a safe wrapper (`scan-unsafe`: one out-slot local in `strip_prefix_ignore_ascii_case`, nothing else) |
| ub_safe / ub_unsafe | no defect: suites clean under ASan/UBSan, TSan, BSan, Miri (FFI-free units); `av_reduce` swept with 40 000 extreme in-range arguments and `av_rescale` with 200 000 under UBSan without a report; 30 000 random `av_sscanf` formats/inputs clean |
| equiv | one defect: a `+`/`-` operand of `parse_cpu_caps` was rounded through C's unprefixed path where C truncates it, so `-0.999` cleared bit 0 and `+2.5` failed (`equiv/parse-cpu-caps-prefixed-operand-rounds`), fixed; 300 000 random flag strings now agree with C |
| internal | no defect: `bind_scan_args` mirrors `ff_vfscanf`'s parser case by case |
| conventions | none |
| misc | none beyond the doc comment corrected with the equiv fix |
| coverage | C: 100% of the blocks of every workset function except paths the wrappers refuse by design (`%n$`, `%m`, `L`, `%C`/`%S`, `fmt_fail`), unreachable `ffintscan` bases and limits, and `/dev/urandom` failure; Rust: 99.7% of regions across the eight homes, the rest an `unreachable!`, the 32-bit `long` branch and the urandom failure |
