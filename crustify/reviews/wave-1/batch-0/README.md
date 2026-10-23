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


---

<!-- merged from crustify/reviews/wave-5/batch-4 -->

# Review: libavutil/core wave-0 batch-14 (string, version, timestamp, sleep and vlog symbols)

Workset (symbol route, objective `review`, translated in `083c4d43d0`):
`av_strlcat`, `av_strlcatf`, `av_strlcpy`, `av_strncasecmp`, `av_strnlen`,
`av_strnstr`, `av_strstart`, `av_strtok`, `av_tolower`, `av_toupper`,
`av_utf8_decode` (`avstring.rs`); `av_strndup` (`mem.rs`);
`av_ts_make_string` (`timestamp.rs`); `av_usleep` (`time.rs`); `av_vlog`
(`log.rs`: `log`, `log_with_context`); `av_x_if_null`, `av_version_info`,
`avutil_configuration`, `avutil_license`, `avutil_version` (`avutil.rs`);
`av_zero_extend_c` (`common.rs`).

| step | outcome |
|---|---|
| wavefront | structured slots and lifecycle (`av_strndup` deep cloner of `s`, everything else borrowed for the call or `static`) verified against the C bodies; the `av_utf8_decode.bufp` note understated the 1-byte cases, resubmitted (`wavefront/`) |
| lifecycle | no type or raw-lifetime item; `av_strndup` binds to the string tier's `HeapString` (`av_free` release, `av_strdup` clone) already landed |
| unsafe | one public `unsafe fn`, `log_with_context`: its `# Safety` list was checked clause by clause against `av_vlog`, `format_line`, `get_category`, `item_name` and `av_log_default_callback` (`log.c`); correct and complete, nothing to tighten. The remaining raw pointers are call-local temporaries (`av_strstart`'s out slot, `av_strnstr`'s base, `av_x_if_null`'s `void *` casts), which `scan-unsafe` lists and which the wrappers never expose |
| ub_safe / ub_unsafe | no defect found: every wrapper's bound (`strlcpy`/`strlcat` sizes, the NUL precondition `strlcat`/`strlcatf`/`tokens` check before C, `av_strnstr`'s `memcmp` windows, `av_utf8_decode`'s cursor, `Tokens`' in-place NULs behind yielded `&'a CStr`) re-derived from the C source; suites clean under ASan/UBSan, TSan, BSan and Miri (FFI-free unit tests) |
| equiv | no defect found: return values, buffers and multi-step sequences agree with C in the existing suites; the one gap was a wrong *description* of `utf8_decode`'s consumption rule (`misc/`), now pinned by `utf8_decode_consumes_as_c_on_each_malformed_shape` |
| internal | `text_until_nul` (shared with `log.rs`) and `Utf8Error` correct; no defect |
| conventions | `strncasecmp` kept its C name beside batch 10's `cmp_ignore_ascii_case`, and the batch's pure queries lacked `#[must_use]` (`conventions/strncasecmp-transliterated-name`); `zero_extend` spelled the shift bound as a literal instead of `MAX_POW2` and lacked `#[inline]` (`conventions/zero-extend-pow2-bound-constant`) |
| misc | `utf8_decode` doc misdescribed what a malformed sequence consumes and whether `i32::MAX` is reachable (`misc/utf8-decode-consumption-doc`) |
| coverage | C (gcov build, whole `ffmpeg` crate suite): every line and branch of `av_strstart`, `av_strnstr`, `av_strlcpy`, `av_strlcat`, `av_strlcatf`, `av_strtok`, `av_strncasecmp`, `av_strndup`, `av_usleep`, `av_vlog` and the three `version.c` getters executed; in `av_utf8_decode` only `av_assert0(tail_len <= 5)`'s abort and the `code >= 1U<<31` cut remain, both unreachable (a 32-bit `top` ends the tail loop after 5 bytes, and `FD BF BF BF BF BF` tops out at `0x7fffffff`). Header inlines (`av_strnlen`, `av_toupper`, `av_tolower`, `av_ts_make_string`, `av_x_if_null`, `av_zero_extend_c`) compile into the `-sys` shim and have no gcov figure. Rust (`llvm-cov`): 100% of the workset's lines, regions and functions; the only missed region in `time.rs` is `usleep`'s `Err` arm, which needs a platform without `nanosleep` |

No UB or equivalence reproducer was warranted: no candidate survived the
source-level check, so `ub_safe/`, `ub_unsafe/` and `equiv/` hold no entries.
