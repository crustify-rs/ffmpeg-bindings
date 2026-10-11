# BSan gate: every report at the options tip is the glibc `strto*` end-pointer false positive

- Kind: tool false positive (no code defect); triage of the options
  sub-campaign's BSan gate, run by this review because wave 2 is the
  sub-campaign's last wave
- Runtime: BorrowSanitizer `de6ede0149` (clang 23.1.3), bsan C build
  `crustify/builds/bsan`
- Evidence: `probe.c` (pure C, no Rust), `skip-list.txt` (the tests that
  report when run alone)

## What the gate saw

On the wave tip `1d434aef61`, before any change, each of the four test
binaries (`cargo bsan test -p ffmpeg --lib` / `--test equiv` / `--test
ub_safe` / `--test ub_unsafe`, `-- --skip compile_fail`,
`gc_hang_threshold_ms=600000`, in parallel) stopped at its first report
within 2-91 s, so most of each binary never ran: `lib` and `equiv` at
`eval.c:120`, `ub_safe` at `eval.c:120`, `ub_unsafe` at `parseutils.c:391`.
BSan has no recover mode.

To see past the first report, every test (1882) was then run alone in its
own process (`<binary> --exact <name> --test-threads=1`, 40 at a time). 44
reported, at five sites, and every other test passed:

| site | reports | read |
|---|---|---|
| `eval.c:120` (`av_strtod`) | 29 | `next[0]` after `strtod(numstr, &next)` |
| `parseutils.c:391` (`av_parse_color`) | 7 | `*tail` after `strtoul(color_string2, &tail, 16)` |
| `eval.c:429` (`parse_primary`) | 4 | `*next` after `av_strtod(p->s, &next)` |
| `eval.c:581` (`parse_dB`) | 2 | `next[0]` after `strtod(p->s, &next)` |
| `parseutils.c:166` (`av_parse_video_size`) | 2 | `*p` after `strtol(str, &p, 10)` |

Each read goes through an end pointer that uninstrumented glibc wrote into a
local of the C frame, and points into the live string being parsed. BSan
reports it as "access an allocation that has been freed" (or "out of
bounds" when the stale tag belonged to a smaller block). The store updates
the slot's bytes but not its shadow, which keeps the provenance an earlier,
dead frame left there, usually a block a previous parse freed. `eval.c:120`,
`eval.c:429` and `parseutils.c:391` are listed as this false-positive class in
the campaign's notes. `eval.c:581` and `parseutils.c:166` are new sites of the
same pattern. The same tests are clean under ASan/UBSan (the reads are in
bounds of live strings) at the reviewed and landed trees.

Which tests trip depends on stack layout. Run alone, earlier waves' suites
that passed inside the harness at the formats gate (`cpu::parse_caps`,
`parseutils`, `eval`) report too. Most of the 44 are options suites that
build `tests/common/opt.rs` objects, whose `av_opt_set_defaults` parses
numbers, sizes and colours, or that evaluate expressions.

## Pure-C reproduction

`probe.c` makes one expression parse and one colour parse at `main`'s depth,
`alloca`s `pad` bytes, then makes the measured call from a deeper frame. No
Rust code runs. Built with the bsan plugin and runtimes against the bsan
`libavutil` (command in the campaign's test recipes, BSan section) and swept
over `pad` = 0, 16, ..., 2048, it reports at:

- `./probe size <pad> "1+2*3"` (`av_parse_video_size("640x480")`):
  `parseutils.c:166:13`, pads 256 352 384 608 848 880 1120 1808;
- `./probe neg <pad> "1+2*3"` (`av_expr_parse_and_eval("-3")`):
  `eval.c:581:37`, pad 80;
- `./probe color <pad> "1+2*3"` (`av_parse_color("0xff00ff")`):
  `parseutils.c:391:13`, pads 320 528 768 1088 1440;

and nowhere when the earlier parses run at the same depth as the measured
call (pad applied before them). Reports from a C program that only parses
valid literals cannot be Rust defects.

```text
error: Undefined Behavior: trying to access an allocation that has been freed.
   --> /target/crustify/builds/bsan/src/libavutil/parseutils.c:166:13
166 | if (*p)
0: av_parse_video_size
error: Undefined Behavior: trying to access an allocation that has been freed.
   --> /target/crustify/builds/bsan/src/libavutil/eval.c:581:37
581 | if (next != p->s && next[0] == 'd' && next[1] == 'B') {
0: parse_dB
```

## Consequence for the gate

Following the policy (pursue true positives only), no existing test was
changed for these. The gate on the landed tree ran each binary as before,
with `--exact --skip <name>` for each test in `skip-list.txt` (binary, then
test name). `equiv`, `ub_safe` and `ub_unsafe` then passed in parallel
mode. The lib binary, run in parallel, reported once more at `eval.c:120`
in a test it could not name, so it was rerun with `--test-threads=1`, where
libtest names each test before it runs. Four more options unit tests
reported at `eval.c:120` one after another, each passing alone. They were
added to the skips (the last four lines of `skip-list.txt`, listed below),
and the fifth run passed. The review's two new object tests that trip the same sites alone
return early under `CRUSTIFY_FFMPEG_VARIANT=bsan`, as the older object suites
do. A future gate over these suites needs the same skips, or the early
returns the wave-0 object suites already use.

## A sweep that cannot finish under BSan

One more lib test never finished:
`libavutil::parseutils::unit_tests::is_nan_spelling_matches_libc_strtod`
(options wave 0). Run alone it hit the 3000 s timeout. It compares
`is_nan_spelling` with libc `strtod` on every string of up to four bytes
over a 14-byte alphabet, about 41k strings, each needing a `CString` and a
libc call. No FFmpeg code runs. Under `CRUSTIFY_FFMPEG_VARIANT=bsan` it now
stops at three bytes (about 3k strings, 65 s alone). Plain, ASan/UBSan and
TSan still sweep four.

## The 44 tests

| binary (`ffmpeg` is the lib unit-test binary) | test | site |
|---|---|---|
| `equiv` | `cpu::parse_caps::fractional_wide_and_named_operands_match_c` | `eval.c:120:13` |
| `equiv` | `cpu::parse_caps::operand_sequences_match_c_after_each_step` | `eval.c:120:13` |
| `equiv` | `cpu::parse_caps::sequences_match_c_after_each_step` | `eval.c:120:13` |
| `equiv` | `cpu::parse_caps::single_strings_match_c_from_several_starting_words` | `eval.c:120:13` |
| `equiv` | `eval::expressions::expressions_evaluate_as_in_c` | `eval.c:581:37` |
| `equiv` | `opt::every_key_agrees_on_a_real_object` | `eval.c:120:13` |
| `equiv` | `opt::lookups::class_finds_search_potential_children_as_c` | `parseutils.c:166:13` |
| `equiv` | `opt::lookups::defaults_agree_after_every_step` | `eval.c:120:13` |
| `equiv` | `opt::lookups::evaluators_agree_on_objects_and_classes` | `eval.c:120:13` |
| `equiv` | `opt::lookups::finds_agree_on_objects_and_classes` | `eval.c:120:13` |
| `equiv` | `opt::lookups::flag_sequences_agree_after_every_step` | `eval.c:120:13` |
| `equiv` | `opt::lookups::options_walk_agrees_with_av_opt_next` | `eval.c:120:13` |
| `equiv` | `opt::objects::children_are_listed_as_c_lists_them` | `eval.c:120:13` |
| `equiv` | `opt::query_sequences_keep_lists_independent` | `eval.c:120:13` |
| `equiv` | `parseutils::colors_agree_at_every_prefix` | `parseutils.c:391:13` |
| `equiv` | `parseutils::failure_messages_agree` | `parseutils.c:391:13` |
| `equiv` | `parseutils::ratios_and_dates::random_ratios_agree` | `eval.c:120:13` |
| `equiv` | `parseutils::ratios_and_dates::ratio_messages_agree` | `eval.c:120:13` |
| `equiv` | `parseutils::ratios_and_dates::ratios_agree_at_every_bound` | `eval.c:581:37` |
| `ffmpeg` | `libavutil::opt::unit_tests::getters::allocation_failures_release_the_elements_written` | `eval.c:120:13` |
| `ffmpeg` | `libavutil::opt::unit_tests::lookups::evaluators_read_constants_and_bounds_of_the_context` | `eval.c:120:13` |
| `ffmpeg` | `libavutil::opt::unit_tests::lookups::values_reaching_unchecked_conversions_are_refused_before_c` | `eval.c:120:13` |
| `ffmpeg` | `libavutil::opt::unit_tests::objects::too_many_unit_constants_fail_as_in_c` | `parseutils.c:166:13` |
| `ffmpeg` | `libavutil::opt::unit_tests::objects::values_reaching_unchecked_conversions_are_refused` | `eval.c:120:13` |
| `ffmpeg` | `libavutil::parseutils::unit_tests::prefixes_inside_the_marker_fail_before_c` | `parseutils.c:391:13` |
| `ub_safe` | `cpu::parse_caps::concurrent_parses_do_not_race` | `eval.c:120:13` |
| `ub_safe` | `opt::lookups::cpuflags_class_lookups_and_evaluations_agree_with_parse_cpu_caps` | `eval.c:120:13` |
| `ub_safe` | `parseutils::colors_with_log_contexts` | `parseutils.c:391:13` |
| `ub_safe` | `parseutils::ratios_and_dates::many_threads` | `eval.c:429:15` |
| `ub_safe` | `parseutils::ratios_and_dates::ratios_at_every_bound_and_offset` | `eval.c:429:15` |
| `ub_safe` | `parseutils::ratios_and_dates::ratios_log_in_a_context` | `eval.c:429:15` |
| `ub_safe` | `parseutils::ratios_and_dates::video_rates` | `eval.c:429:15` |
| `ub_unsafe` | `opt::log_contexts::classed_contexts` | `parseutils.c:391:13` |
| `ub_unsafe` | `opt::log_contexts::long_colors` | `parseutils.c:391:13` |
| `ub_unsafe` | `opt::log_contexts::null_and_classless_contexts_are_no_context` | `parseutils.c:391:13` |
| `ub_unsafe` | `opt::lookups::class_contexts_evaluate_on_many_threads` | `eval.c:120:13` |
| `ub_unsafe` | `opt::lookups::defaults_are_checked_across_writes_and_frees_on_parent_and_child` | `eval.c:120:13` |
| `ub_unsafe` | `opt::lookups::every_evaluator_on_every_option_refuses_undefined_values` | `eval.c:120:13` |
| `ub_unsafe` | `opt::lookups::found_options_and_walks_outlive_the_handles_that_found_them` | `eval.c:120:13` |
| `ub_unsafe` | `opt::objects::fixture_strings_are_c_strings` | `eval.c:120:13` |
| `ub_unsafe` | `opt::query::real_object_with_children_search` | `eval.c:120:13` |
| `ub_unsafe` | `opt::query_default::real_object` | `eval.c:120:13` |
| `ub_unsafe` | `opt::set_options_string::class_is_the_fixture_class` | `eval.c:120:13` |
| `ub_unsafe` | `opt::typed_setters::every_typed_setter_on_every_option` | `eval.c:120:13` |

Added while running the lib binary single-threaded on the landed tree (each
passes alone):

| binary | test | site |
|---|---|---|
| `ffmpeg` | `libavutil::opt::unit_tests::objects::a_refused_pair_sets_nothing` | `eval.c:120:13` |
| `ffmpeg` | `libavutil::opt::unit_tests::objects::child_constants_are_looked_up_in_children` | `eval.c:120:13` |
| `ffmpeg` | `libavutil::opt::unit_tests::objects::option_lists_are_refused_pair_by_pair` | `eval.c:120:13` |
| `ffmpeg` | `libavutil::opt::unit_tests::typed_setters::array_values_reaching_undefined_conversions_are_refused` | `eval.c:120:13` |
