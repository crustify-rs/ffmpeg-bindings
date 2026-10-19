# Review: libavutil/options wave 0, batch 0 (the wave's types)

- Items: `AVDictionary` (`dict::Dictionary`, owned as `DictionaryBox` / `Dict`),
  `AVDictionaryEntry` (`dict::DictionaryEntry`, fields `key`, `value`),
  `AVExpr` (`eval::Expr`, owned as `ExprBox`), `AVOptionArrayDef`
  (`opt::OptionArrayDef`, fields `def`, `sep`, `size_max`, `size_min`),
  `AVOptionRange` (`opt::OptionRange`, fields `str`, `is_range`,
  `component_max`, `component_min`, `value_max`, `value_min`) and
  `AVOptionType` (`opt::OptionType`).
- Homes: `crustify/rust/ffmpeg/libavutil/{dict,eval,opt}.rs`.
- Reviewed commit: the wave tip `cde88fc249` (types landed by batches 0
  `aa6053aec5`, 1 `66fa3604d1` and 2 `e605248b0c`; symbols by batches 4
  `ae14a85553` and 3 `cde88fc249`).
- Reports: `unsafe/expr-parse-with-context-raw-log-context`,
  `conventions/expr-send-safety-comment-past-scan-window`.
- Not the sub-campaign's last wave (options has waves 0-2), so no BSan gate:
  BSan runs only on reproducers, and this review emits none.

## Verdicts by step

| step | finding |
|---|---|
| wavefront | The submitted findings match the C for all six types: `AVDictionary` (opaque, `elems` owned and NULL only before the first insert; lifecycle `av_dict_set` constructor, `av_dict_free` dropper, `av_dict_copy` deep cloner), `AVDictionaryEntry` (`key` / `value` owned by the dictionary, never NULL, `value` written by `AV_DICT_APPEND` only), `AVExpr` (`av_expr_parse` alloc constructor, `av_expr_free` recursive dropper, `param` children unshared, `func1` / `func2` copies of the caller's functions, `func0` static), `AVOptionArrayDef` (static descriptor, `def` a static string), `AVOptionRange` (element of an `AVOptionRanges` list, `str` owned and released by `av_opt_freep_ranges`) and `AVOptionType` (open enum with the array flag). The lifecycle symbols' records (`av_expr_parse`, `av_expr_free`, `av_dict_set`, `av_dict_copy`, `av_dict_free`, `av_opt_freep_ranges`) agree with the C bodies. Nothing to resubmit. |
| lifecycle | Complete. `AVDictionary`: owned non-empty (`DictionaryBox`, `av_dict_free` + `av_dict_copy` with `AV_DICT_MULTIKEY`), owned nullable slot (`Dict`, built by `new` / `from_entries` through `av_dict_set`, which allocates and frees the dictionary with its first and last entry) and borrowed (`DictionaryRef`); entries are borrow-only, as C stores them only in the entry array. `AVExpr`: `ExprBox` with `av_expr_free`, built only by `Expr::parse` / `parse_with_context`; C has no copy. `AVOptionRange`: borrowed from its list, its `str` replaced, taken and released as a `HeapString` (the list frees it with `av_freep`). `AVOptionArrayDef`: a value type (`new`, `Default`, `CZeroable`) with no C lifecycle. `AVOptionType`: a value. No gap. |
| unsafe boundary | `Expr::parse_with_context` was a public `unsafe fn` taking the parent log context as `*mut c_void`, a raw seam made obsolete by batch 3's `ObjectRef` (already used by `parse_and_eval`): now safe over `Option<ObjectRef<'_>>` (`unsafe/expr-parse-with-context-raw-log-context`). `scan-unsafe` then reports no smell counter in `eval.rs` or `dict.rs`. The workset's remaining `opt.rs` smells are not its own: `OptionRanges::query` / `query_default` (core's `AVOptionRanges`; their `void *obj` could now take `ObjectRef` too, which their scheduled symbols in options wave 2 should pick up) and `set_options_string` (a symbol). Every field accessor projects with `addr_of!` / `addr_of_mut!` through the handle pointers; the `Send` / `Sync` proofs hold (`DictionaryMut` and `DictionaryEntryMut` have no setters; `OptionRangeMut` and `OptionArrayDefMut` write only through `&mut self`; `Expr` is `Send` but not `Sync`, since evaluation writes the root's registers). |
| ub_safe / ub_unsafe | No UB found. Read against the C: `av_expr_parse` with the wrapper's level-offset guard (C adds the offset to `AV_LOG_ERROR` only; `av_log_default_callback` masks and clips any level), the parser's depth limits and failure paths (every partial tree freed); `av_dict_set` / `av_dict_copy` / `av_dict_free` with `MULTIKEY`; `av_opt_freep_ranges` over `get_mut`'s distinct elements; the array separator rules against `av_opt_set_defaults`' `av_assert0`. Whole-crate ASan/UBSan and TSan are clean on the wave tip and on the review's tree, and the 22 FFI-free unit tests of the workset's types pass under Miri. With `parse_with_context` safe, the workset adds no public unsafe API, so `tests/ub_unsafe/eval.rs` moved to `tests/ub_safe/eval.rs::parse_with_context`. |
| equiv | The suites compare dictionaries built, copied and edited by C and by the wrappers step by step, parsed trees through C's evaluator and counters, range writes against C's own and array definitions through `av_opt_set_defaults`. Added: `equiv/eval.rs::parse_with_context::parses_log_as_c_under_each_parent`, the result and every logged line of `parse_with_context` against `av_expr_parse` under no parent, a NULL-class parent (C) vs `None`, and a classed parent. No divergence found. |
| internal | Private helpers (`Dict::append`, `terminated`, `opt_c_str`, `OptionRangesRef::slot` / `slots`, `RangeIter`) checked; no defect. |
| conventions | `Send for Expr`'s nine-line `SAFETY:` block exceeded `scan-unsafe`'s six-line window and counted as undocumented (`conventions/expr-send-safety-comment-past-scan-window`). Anchors: one `Wraps:` per type, `Field:` on every getter and setter of the eleven field anchors; Rust names throughout; clippy `--all-targets -D warnings` and rustdoc `-D warnings` clean; the changed files are rustfmt-clean, as they were at the base. |
| misc | Nothing to report. Hand-off for wave 1 (`av_expr_eval`): `AVExpr` keeps no constant count and C reads `const_values[const_index]` unchecked, so a safe evaluator must carry the count `Expr::parse` was given; `ExprBox` does not record it today. |

## Coverage

Details in `coverage.json`. Rust: `dict.rs` 100% of lines, regions and
functions; `eval.rs` and `opt.rs` miss only test-harness arms (helper
functions C is meant never to call, BSan-only early returns, `panic!` arms)
and one line of a symbol outside the workset. C: `av_expr_parse` and its
parser, `av_expr_free`, `av_dict_free`, `av_dict_copy`,
`av_opt_freep_ranges` and `av_opt_query_ranges(_default)` 100% of lines;
`av_dict_set` 70%, the rest being flag paths (`DONT_STRDUP_VAL`, `DEDUP`,
`DONT_OVERWRITE`, `APPEND`) that the workset's `MULTIKEY`-only constructor
cannot reach and wave 1's `av_dict_set` wrapper owns.

## Validation at the landed tree

- `cargo check --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `RUSTDOCFLAGS=-D warnings cargo doc -p ffmpeg --no-deps`: clean.
- plain `cargo test --workspace`: 630 lib, 515 equiv, 533 ub_safe (the
  compile-fail programs included), 93 ub_unsafe, 87 doctests, 0 failed.
- ASan/UBSan and TSan (`-Zbuild-std`, the matching C variants, whole crate):
  630 / 515 / 532 / 93 each, no `runtime error:` and no sanitizer report;
  on the wave tip before the change, 630 / 514 / 528 / 98, equally clean.
- Miri: the 22 FFI-free unit tests of the workset's types (`dict` entry
  getters, `eval` terminators and box layout, `opt` option types, array
  definitions and range handles over Rust-built lists).
- `crustify scan-unsafe --sites all`: `unsafe_fns_pub_smell`,
  `raw_ptr_pub_smell` and `void_ptr_pub_smell` each down by one (the
  `parse_with_context` seam), undocumented unsafe sites 24 -> 23 (none left
  in `eval.rs` or `dict.rs`).
- BSan: not run; this is not the sub-campaign's last wave and the review
  emits no reproducer.
