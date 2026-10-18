# Review: options wave-0 symbols (option object API, evaluator, parsers)

Route: **symbol** (29 items). Objective: **review**. Affected base:
`dc97af2db7784834f60a23ec4667e9b11f57e9b9` (the wave's head, translation
batches 2 and 3 holding the 29 items).

Items and homes:

- `crustify/rust/ffmpeg/libavutil/eval.rs`: `av_expr_parse_and_eval`
  (`parse_and_eval`), `av_strtod` (`parse_number`);
- `crustify/rust/ffmpeg/libavutil/parseutils.rs`: `av_parse_color`
  (`parse_color`, `parse_color_prefix`, `parse_color_with_context`),
  `av_parse_video_size`, `av_parse_time` (`parse_date`, `parse_duration`),
  `av_find_info_tag`, `av_get_known_color_name` (`known_color_name`);
- `crustify/rust/ffmpeg/libavutil/opt.rs`: the `av_opt_*` object API on
  `ObjectRef` / `ObjectMut` (`av_opt_child_next`, `av_opt_copy`,
  `av_opt_flag_is_set`, `av_opt_free`, `av_opt_get`,
  `av_opt_get_array_size`, `av_opt_get_double`, `av_opt_get_image_size`,
  `av_opt_get_int`, `av_opt_is_set_to_default_by_name`, `av_opt_serialize`,
  `av_opt_set`, `av_opt_set_bin`, `av_opt_set_defaults`,
  `av_opt_set_defaults2`, `av_opt_set_double`, `av_opt_set_from_string`,
  `av_opt_set_image_size`, `av_opt_set_int`, `av_opt_show2`),
  `av_opt_get_key_value` (`parse_key_value`), `av_set_options_string`, and
  the hand-written `BareObject`.

This is not the sub-campaign's last wave (`options` has waves 0-2), so BSan
ran only on the three reproducers, each alone.

## Findings

- **UB, three defects, fixed** (`ub_unsafe/`; each reproducer adopts its
  object or class with the seam `ObjectMut::from_ptr` / `ObjectRef::from_ptr`
  within its contract, and the defect is in a safe method):
  - `set-flag-operand-clock-race`: `ObjectMut::set` previews a `+`/`-`
    flag operand (and a wide rational) to refuse C's unchecked integer
    conversions, then C evaluates it again; an operand calling `time()`
    changes value in between (UBSan float-cast-overflow, `opt.c:508`).
    Previews now refuse expressions that read the clock.
  - `set-array-default-constant-mismatch`: the preview evaluated an array
    option's `default` as 0, but C's `DEFAULT_NUMVAL` reads the definition's
    address bits (a non-zero subnormal); `+1e30*not(not(default))` passed the
    preview and overflowed in C (UBSan, `opt.c:508`). The preview now uses
    C's value.
  - `bare-object-foreign-callbacks-read-past-object`: `BareObject::new`
    accepted classes whose `item_name` / `get_category` read their objects;
    logging through the 8-byte bare object read past it (ASan
    heap-buffer-overflow from `log.c:343`). Such classes are now refused.
- **Safe boundary, two defects, fixed** (`unsafe/`): `set_options_string`
  (raw `*mut c_void` context, an incomplete UB contract) is now the safe
  `ObjectMut::set_options_string`, which refuses a list holding any value
  `set` refuses; `parse_color_with_context` takes `Option<ObjectRef>`
  instead of a raw log context. The workset now adds no public unsafe
  function beyond the documented adoption seams `ObjectRef::from_ptr` /
  `ObjectMut::from_ptr`, whose `# Safety` contracts were re-read against
  every C path (callbacks, children, defaults, owned fields) and stand.
- **Wavefront, two corrections, resubmitted** (`wavefront/`): the log
  context notes of `av_parse_color`, `av_opt_show2` and
  `av_expr_parse_and_eval` omitted that logging calls the class's callbacks
  on the context; `av_set_options_string.ctx` carried a stale ("unsafe")
  and incomplete UB list and `av_opt_set.val` none. The other 24 records
  were read against the C bodies and match.
- **No equivalence defect.** The landed suites compare every setter, getter,
  copy, free, listing and parse with the raw calls. The rewritten
  `set_options_string` equivalence module found one test defect, not a
  wrapper one: a failed color parse, like a failed image size, copies C's
  uninitialised `tmp[4]` into the field (`set_string_color`), so those two
  options are masked after a failing list and reset on both sides before
  the next step (it showed under TSan only).
- **No internal defect** beyond the two preview defects above; the new
  helpers (`eval::reads_clock`, `options_refusal`, `skip_byte`, the array
  case of `default_number`) have inline unit tests.
- **No convention deviation**: clippy `--workspace --all-targets -D
  warnings`, every unsafe block documented, one filled anchor per item or
  safe variant; `crustify scan-unsafe` no longer reports the two seams
  (`opt.rs:1175`, `parseutils.rs:160`) and reports nothing at a site this
  review added.
- **No lifecycle work**: symbol route.

## Coverage

`coverage.json`. The review added an equivalence test listing every option
type with the extreme bounds and defaults `av_opt_show2` spells out
(`objects::listings_of_every_type_and_extreme_log_as_c`), since the
fixture's options are mostly flagless and `av_opt_show2` lists only flagged
ones, and numeric pixel/sample format values to the setter corpus.

## Validation

On the tree rebased onto the wave tip `9ff1ec06ed` (the type review's
`Expr::parse_with_context` change; no conflict):

- `cargo check --workspace`, `cargo clippy --workspace --all-targets -- -D
  warnings`: clean.
- `cargo test --workspace` (plain): lib 634, equiv 516, ub_safe 534 (with
  the trybuild `compile_fail` cases), ub_unsafe 97, doctests; all pass.
- ASan/UBSan and TSan (`-Zbuild-std`, `--lib --tests -- --skip
  compile_fail`): all pass, no `runtime error:`, AddressSanitizer,
  LeakSanitizer or ThreadSanitizer report. The last two test additions (the
  `root` second-phase case and the `(ENCODING_PARAM, NONE)` serialize case)
  were run afterwards under plain, ASan/UBSan and TSan with their binaries'
  filters.
- Miri: `eval::unit_tests::clock_readers_are_found_by_name` (the only new
  FFI-free test).
- Reproducers: each exits non-zero on `dc97af2db7` (UBSan `opt.c:508:30`
  twice, ASan heap-buffer-overflow) and 0 on the fixed tree, built with
  `CRUSTIFY_FFMPEG_VARIANT=asan-ubsan RUSTFLAGS=-Zsanitizer=address`.
- BSan, each reproducer alone on the fixed tree:
  `bare-object-foreign-callbacks-read-past-object` passes;
  `set-array-default-constant-mismatch` reports "access an allocation that
  has been freed" at `eval.c:120` (`next[0]` in `av_strtod`) during the
  wrapper's own quiet evaluation, the known stale-shadow false positive of
  glibc's uninstrumented `strtod` end pointer (shared recipes; ASan is clean
  on the same run), not pursued; `set-flag-operand-clock-race` produced no
  report in 20 minutes (200 000 attempts are slow under BSan) and was
  stopped at the time limit.
- `crustify scan-unsafe`: `unsafe_fns_pub_smell` 19 -> 16,
  `raw_ptr_pub_smell` and `void_ptr_pub_smell` 23 -> 20 against the
  affected base (the type review's `eval.rs` seam included); no smell at a
  site this review added. The remaining ones in the workset's homes are the
  range queries (`opt.rs:763`, `:793`, type items) and core's `log.rs`.
