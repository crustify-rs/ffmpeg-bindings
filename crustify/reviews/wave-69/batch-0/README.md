# Review: libavutil/options wave 1, batch 0 (the wave's symbols)

Route: **symbol** (35 items). Objective: **review**. Reviewed commit: the
wave tip `c771c3e8bd` (translation batches 0 `a2aace4c3d` and 1
`c771c3e8bd`).

Items and homes:

- `crustify/rust/ffmpeg/libavutil/dict.rs`: `av_dict_copy`
  (`Dict::copy_from`), `av_dict_count` (`len`), `av_dict_free` (wave-0 drop
  policy), `av_dict_get` (`get`, `get_all` / `Matches`),
  `av_dict_get_string` (`serialize`), `av_dict_iterate` (`iter` / `Iter`),
  `av_dict_parse_string` (`Dict::parse_pairs`), `av_dict_set` (`Dict::set`,
  `remove`, `set_owned`), `av_dict_set_int` (`Dict::set_int`);
- `crustify/rust/ffmpeg/libavutil/eval.rs`: `av_expr_count_func`,
  `av_expr_count_vars` (`ExprRef::count_functions`, `count_constants`),
  `av_expr_eval` (`Expression::eval`, `eval_with`), `av_expr_free` (wave-0
  drop policy), `av_expr_parse` (`Expression::parse`,
  `parse_with_context`);
- `crustify/rust/ffmpeg/libavutil/opt.rs`: `av_opt_child_class_iterate`
  (`ClassRef::potential_children`), the typed getters
  (`ObjectRef::get_rational`, `get_video_rate`, `get_pixel_format`,
  `get_sample_format`, `get_channel_layout`, `get_dict`, `get_array`), the
  typed setters (`ObjectMut::set_rational`, `set_video_rate`,
  `set_pixel_format`, `set_sample_format`, `set_channel_layout`,
  `set_dictionary`), `av_opt_set_array` (`insert_array`, `replace_array`,
  `remove_array`), `av_opt_set_dict` / `av_opt_set_dict2`
  (`set_from_dict`, `set_from_dict_with`);
- `crustify/rust/ffmpeg/libavutil/parseutils.rs`: `av_parse_ratio`
  (`parse_ratio`, `parse_ratio_with_context`), `av_parse_video_rate`,
  `av_small_strptime`, `av_timegm`.

Reports:

- `ub_unsafe/remove-array-shrink-failure-double-free` (reproducer + fix)
- `misc/insert-array-rational-conversion-misdocumented`
- `wavefront/set-array-shrink-failure-video-rate-path-and-set-int-key`

Not the sub-campaign's last wave (options has waves 0-2 in
`schedule.json`), so no BSan gate: BSan ran only on this review's one
reproducer, alone.

## Verdicts by step

| step | finding |
|---|---|
| wavefront | 32 of the 35 records match the C bodies and the wrappers. Corrected and resubmitted: `av_opt_set_array` (the removal path's failed shrink, which keeps a stale count, was missing; the input trait was named `ArrayElement` instead of `ArrayInput`; the `RATIONAL` conversion into integer elements was missing), `av_opt_set_video_rate` (claimed a `set_format` bounds conversion; it goes through `set_number` / `write_number`), `av_dict_set_int` (`key` recorded not owned, although `DONT_STRDUP_KEY` transfers it as in `av_dict_set`). |
| lifecycle | Not applicable: a symbol route. `av_dict_free` and `av_expr_free` remain the wave-0 drop policies (`DictionaryFree`, `ExprFree`), with no second wrapper. |
| unsafe boundary | The wave adds no public `unsafe fn` and no raw pointer on its public surface. `crustify scan-unsafe --sites all` in the workset's homes: the only public smells are `OptionRanges::query` / `query_default` (core's `AVOptionRanges`, scheduled for options wave 2); the private ones are the sealed `element::Element::adopt` (its obligation, "what C wrote for `TYPE`", is not expressible in types), the `ArrayInput` C-side element types, `PotentialChildren`'s opaque state and `dict.rs`'s `c_ptr` helper, all internal. Undocumented unsafe sites unchanged at 23 crate-wide (the same as at the end of wave 0). |
| ub_safe / ub_unsafe | One true defect: `ObjectMut::remove_array` let C's failed shrink keep a stale count, so `av_opt_free` freed a shifted element twice (ASan, and BSan, double free); `mem::set_max_alloc` is safe, so a safe call sequence reached it (`ub_unsafe/remove-array-shrink-failure-double-free`; fixed by writing the new count back after that `ENOMEM`). Checked and clean: every array conversion of `av_opt_set_array` / `av_opt_get_array` against `write_number` / `read_number` (the `(int)` and `(int64_t)` casts the wrappers refuse, `llrint` paths that are unspecified but not UB), the type-mismatch logs `type_refusal` guards, `av_opt_set_q` over every scalar type, `av_dict_set`'s `DONT_STRDUP` hand-over on every path, the reallocation failures of `av_dict_set`, `av_opt_set_array`'s insert/replace, `av_opt_get_array` and `av_channel_layout_copy`, `Expression`'s persistent registers (no path reseeds `random` from an unchecked register without `st` / `root`, which `refuses` already rejects together), the counters' `size` clamp, `av_small_strptime`'s reads up to the NULs and `timegm_terms`' mirror of `av_timegm`'s `int` order. The wave adds no public unsafe API, so no `ub_unsafe` suite of its own beyond the regression. |
| equiv | No behavioural divergence: the wrappers pass values to C, and the two re-implemented paths (`get_video_rate` reading the stored rate, `timegm`'s `None`) are documented corrections. One documentation defect: `insert_array` said rationals convert "as by `set_rational`", but C stores `llrint(1 / den) * num` into integer-like elements (`misc/insert-array-rational-conversion-misdocumented`; doc fixed, pinned by `equiv/opt.rs::rational_array_conversion`). `equiv/opt.rs::typed_setters::allocation_failures_agree` had been comparing C's stale count after a failed shrink; it now repairs the C reference as the wrapper repairs itself. |
| internal | No further defect. `Dict::with_slot`'s obligation holds for every `av_dict_*` writer (a dictionary is freed with its last entry on every path, failures included); `Iter`'s count cannot underflow under the shared borrow; `numeric_elements!`'s `Rational` arm now previews C's array operands `(1, den, num)` (same verdicts; part of the misc report). |
| conventions | One filled anchor per item (several only where several wrappers represent one item: `av_dict_set` x3, `av_opt_set_array` x3, `av_expr_parse` x4 with wave 0, `av_dict_get`, `av_expr_eval`, `av_parse_ratio` x2, `av_dict_copy` with the wave-0 cloner); Rust names throughout; clippy `--all-targets -D warnings` and rustdoc `-D warnings` clean; the changed files keep the base's rustfmt state (only the review's own lines were formatted). |
| misc | The documentation report above. |

## Coverage

Details in `coverage.json`. C: `dict.c`, `eval.c` and `parseutils.c` 100%
of lines; every workset function in `opt.c` 100% except
`av_opt_get_array`'s unreachable `av_assert0(0)` default (98.25%); the rest
of `opt.c`'s misses belong to options wave 2 or are unreachable for a
well-formed class. Rust: `dict.rs` and `parseutils.rs` 100% of lines,
`opt.rs` 99.38% and `eval.rs` 97.78%, the misses being test-harness arms
(BSan early returns, `panic!` arms, unused test functions) and two wave-0
lines. The new repair path of `remove_array` is covered.

## Validation at the landed tree

- `cargo check --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `RUSTDOCFLAGS=-D warnings cargo doc -p ffmpeg --no-deps`: clean.
- plain `cargo test --workspace`: 669 lib, 538 equiv, 556 ub_safe (the
  compile-fail programs included), 103 ub_unsafe, 91 + 25 doctests, 0
  failed. On the wave tip before the review: 668 / 537 / 556 / 102.
- ASan/UBSan and TSan (`-Zbuild-std`, the matching C variants, `--lib
  --tests -- --skip compile_fail`): 669 / 538 / 555 / 103 each, no
  `runtime error:`, AddressSanitizer, LeakSanitizer or ThreadSanitizer
  report; the wave tip before the fix was equally clean under ASan (the
  defect needs an allocation limit no existing test set before a removal of
  owned elements).
- Miri: the workset's FFI-free unit tests
  (`dict::...::flags_are_cs_bits`,
  `opt::...::array_elements_name_their_c_types`,
  `opt::...::write_refusal_mirrors_write_number`,
  `parseutils::...::timegm_terms_flag_every_int_overflow`).
- Reproducer: exits 1 on `c771c3e8bd` (ASan double free; BSan also reports
  the second `av_free`) and 0 on the fixed tree under ASan and under BSan,
  run alone.
- BSan: only the reproducer, alone (not the sub-campaign's last wave).
