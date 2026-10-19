# Review: libavutil/options wave 2, batch 0 (the wave's symbols)

Route: **symbol** (14 items). Objective: **review**. Reviewed commit: the
wave tip `abf526dc98` (the wave's one translation batch).

Items, all homed in `crustify/rust/ffmpeg/libavutil/opt.rs`:

- `av_opt_find` (`ObjectRef::find`, `ClassRef::find`), `av_opt_find2`
  (`ObjectRef::find_with_target`), `av_opt_next` (`ObjectRef::options` /
  `Options`);
- `av_opt_is_set_to_default` (`ObjectRef::is_option_set_to_default`);
- `av_opt_eval_{flags,int,uint,int64,float,double,q}`
  (`OptionDefRef::eval_flags` ... `eval_rational`, in an `EvalContext`);
- `av_opt_query_ranges` / `av_opt_query_ranges_default`
  (`ObjectRef::option_ranges` / `default_option_ranges` and the `ClassRef`
  forms, `RangeFlags`), `av_opt_freep_ranges` (the `OptionRangesFree` drop
  policy of `OptionRangesBox`, unchanged).

Wave 2 is the options sub-campaign's last wave (`schedule.json`: waves 0-2),
so this review ran the sub-campaign's BSan gate (below).

Reports:

- `ub_safe/eval-flags-token-split-hides-evaluator-call` (reproducer + fix)
- `wavefront/eval-flags-val-token-split`
- `misc/bsan-gate-strto-end-pointer-false-positives` (gate triage, pure-C
  probe; no code defect)

## Verdicts by step

| step | finding |
|---|---|
| BSan gate | Wave tip, before any change: all four binaries stopped at their first report within 2-91 s, so most of each never ran. Every test was then run alone (1882 processes): 44 reported, all at reads through glibc `strtod` / `strtol` / `strtoul` end pointers (`eval.c:120`, `:429`, `:581`, `parseutils.c:166`, `:391`), the known false-positive class. The two sites not listed before reproduce from a C program with no Rust in it. No true positive. Landed tree: the four binaries pass with those tests skipped (four more in the lib binary, found when it was run single-threaded), figures below. One wave-0 unit test, a libc-only sweep that timed out at 3000 s alone, now sweeps three bytes instead of four under BSan. See `misc/`. |
| wavefront | 13 of the 14 records match the C and the wrappers. `av_opt_eval_flags`' `val` note ("sscanf num/den for rationals, av_expr_parse_and_eval otherwise", shared with the other evaluators) missed that a flags value is cut into tokens of at most 255 bytes at `+` / `-`, each one evaluated on its own. That cut is the cause of the UB defect below. Corrected and resubmitted. |
| lifecycle | Not applicable: a symbol route. `av_opt_freep_ranges` stays the wave-0 `OptionRangesFree` policy with its one anchor, and no second wrapper was emitted. Both queries hand the new list straight to `OptionRangesBox::from_c`, with a NULL success reported as `AVERROR_BUG`. |
| unsafe boundary | The wave adds no public `unsafe fn` and no raw pointer. It removed the unsafe raw-object `OptionRanges::query` / `query_default`, so the public smells fell from 16 to 14 unsafe fns and from 20 to 18 raw pointers (`scan-unsafe`, against the pre-wave tree). The remaining `opt.rs` sites are private (`element::Element::adopt`, the `ArrayInput` C sides, `PotentialChildren`'s state) or the `ObjectRef` / `ObjectMut::from_ptr` seams. The `SAFETY` comments of the new blocks were checked against the C: FAKE_OBJ reads only the class; a found or walked entry belongs to a table that outlives the borrow; `has_option` bounds `is_option_set_to_default`'s offset; a class context logs as a bare object. The review's changes leave every counter unchanged (undocumented sites 22 crate-wide, 4 in `opt.rs`, none new). |
| ub_safe / ub_unsafe | One true defect, reachable from safe code. `eval_flags`, and the setters through the same refusal, checked the whole flags value for evaluator calls C converts unchecked, and checked tokens only after a `+` / `-`. C cuts a token after 255 bytes, so a cut piece can start inside what the whole value reads as one number. `0*` + 253 zeros + `ld(1e300)` reaches `ld`'s unchecked `(int)1e300` (UBSan `eval.c:205`) through `eval_flags` on C's `cpuflags` class. Fixed: `flags_refusal` checks every token. Regressions: `ub_safe`, `ub_unsafe` (the evaluators in both contexts plus `set`, `set_options_string` and `set_from_string`), `equiv`, and an FFI-free unit test, which passes under Miri. Checked and clean: `write_number`'s conversions for every evaluator type (only `RATIONAL`'s `(int)` and the flags operand's `(int64_t)` are UB, both guarded; `llrint` overflow is unspecified, not UB); NaN cannot reach them (`av_expr_parse_and_eval` fails on NaN, constants are `i64`); class contexts (`child_next` and the logging reads refused); `is_option_set_to_default`'s table check and every type arm (binary arrays stay empty through safe paths, binary lengths are reset by `free_options`); `find_with_target`'s target; the walk's termination. The wave adds no public unsafe API, so it has no `ub_unsafe` suite of its own beyond these regressions. |
| equiv | No divergence on values C evaluates safely. The suite compared every evaluator on every option in both contexts, flag sequences, lookups, the walk, defaults after each step, and range queries. Gaps filled: flags values longer than C's token buffer (`long_flag_values_are_cut_and_refused_as_tokens`, compared after every call, with the refused values asserted Rust-side only), and a dictionary default C cannot parse (`unparsable_dictionary_defaults_fail_as_c`, both default checks). |
| internal | No further defect: `has_option`'s offset arithmetic, `Options`' fused end, `eval`'s type and read-only pre-check (C rejects those before reading `val`, so no refusal is needed), `query_ranges`' ownership on failure. |
| conventions | One filled anchor per item; duplicates only where two wrappers represent one item (`av_opt_find`, `av_opt_query_ranges`, `av_opt_query_ranges_default`, each on `ObjectRef` and `ClassRef`). Rust names (`eval_rational`, `option_ranges`, `RangeFlags`). `clippy --all-targets -D warnings` and rustdoc `-D warnings` are clean. The four changed files keep the base's rustfmt state: only the review's lines were formatted, and the hunk counts equal the base's. |
| misc | The BSan triage report, and `parseutils.rs`'s `is_nan_spelling_matches_libc_strtod` thinned under BSan so that the lib binary can finish. |

## Coverage

Details in `coverage.json`. C: every workset function in `opt.c` at 100%
of lines, `set_string_number` and `write_number` included, except
`av_opt_find2`'s FAKE_OBJ target store and `av_opt_is_set_to_default`'s
unsupported-type default. Neither can be reached through the wrappers or a
well-formed class. The one reachable gap, `is_set_to_default`'s failed
dictionary-default parse, is now covered. Rust: `opt.rs` 99.28% of lines.
The wave-2 section has no missed line; the misses are wave-0 lines and
test-harness arms.

## Validation at the landed tree

- `cargo check --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `RUSTDOCFLAGS=-D warnings cargo doc -p ffmpeg --no-deps`: clean.
- Plain `cargo test --workspace`: 678 lib, 544 equiv, 559 ub_safe (the
  compile-fail programs included), 107 ub_unsafe, 91 + 25 doctests, 0
  failed. On the wave tip before the review: 677 / 542 / 558 / 106.
- ASan/UBSan and TSan (`-Zbuild-std`, the matching C variants, `--lib
  --tests -- --skip compile_fail`): 678 / 544 / 558 / 107 each, with no
  `runtime error:` and no AddressSanitizer, LeakSanitizer or ThreadSanitizer
  report. The later BSan-only thinning of one lib test changes nothing in
  these builds (it was rerun plain).
- Reproducer: exits 1 on `abf526dc98` (UBSan `eval.c:206:57`
  float-cast-overflow, `halt_on_error=1`) and 0 on the fixed tree, ASan/UBSan
  build.
- Miri: `opt::unit_tests::lookups::flags_refusal_checks_every_cut_token`
  (the per-token check, FFI-free).
- BSan gate (`cargo bsan`, bsan C build, `gc_hang_threshold_ms=600000`, one
  invocation per binary under `timeout 7000`):
  - Wave tip `abf526dc98`, before any change, four binaries in parallel:
    each stopped at its first false positive (lib 15 s, equiv 18 s, ub_safe
    91 s, ub_unsafe 2 s). Then every test alone (1882 runs): 44 reports, all
    at the five glibc `strto*` end-pointer sites. One test,
    `is_nan_spelling_matches_libc_strtod`, timed out.
  - Landed tree, with the `misc/.../skip-list.txt` skips: equiv 525 passed
    (19 skipped, 1465 s), ub_safe 551 passed (7 skipped plus compile_fail,
    2431 s), ub_unsafe 95 passed (12 skipped, 194 s), each exit 0 in
    parallel. The lib binary reported a false positive at `eval.c:120` in
    parallel mode, so it was rerun with `--test-threads=1`. Four reruns each
    added one more `eval.c:120` test to the skips, and the fifth passed: 668
    passed, 10 skipped, 354 s.
  - The review's new tests run under BSan, each alone: the unit test, the
    `ub_safe` regression and the dictionary-default test pass. The two
    object tests that hit the false positive return early under BSan, as
    the older object suites do.
- `crustify scan-unsafe`: counters unchanged against the wave tip (22
  undocumented sites crate-wide).
