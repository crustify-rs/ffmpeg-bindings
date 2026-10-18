# Review: libavutil/core wave 0, batch 0 (`AVClassCategory`, `AVClassStateFlags`)

- Home: `crustify/rust/ffmpeg/libavutil/log.rs` (`ClassCategory`, `ClassStateFlags`)
- Reviewed commit: `25fe7bf146` as landed on the wave branch at `3770f8f483`
- Reports: one, `misc/class-category-device-predicates` (the header's two
  predicates over the category were not wrapped). Every other step below
  found nothing to report.

## Verdicts by step

| step | finding |
|---|---|
| wavefront | The submitted type findings (field-less enums, no lifecycle, any `unsigned` reaches Rust unchecked, `log.c` guards only the field `>= AV_CLASS_CATEGORY_NB`) match the C: `AVClass.category` is stored by value, `av_default_get_category` returns it unchecked, and the `get_category` callback's result is not guarded. `--lifecycle-ops` and `--users` are empty for both. No resubmission. |
| lifecycle | None exists: C never allocates, frees, clones or disposes either value. Plain `Copy` newtypes are the complete representation. |
| unsafe boundary | The two types expose no `unsafe fn` and no raw pointer; the only unsafe blocks in the workset are the two shim calls the fix adds, each a by-value FFI call. `scan-unsafe --name AVClassCategory AVClassStateFlags AV_IS_INPUT_DEVICE AV_IS_OUTPUT_DEVICE` reports no raw-pointer sites and no undocumented unsafe. |
| ub_safe / ub_unsafe | No UB is reachable: both are `#[repr(transparent)]` over bindgen's `c_uint` newtypes, every bit pattern is a valid value, and no operation dereferences anything. The FFI-free tests (`unit_tests::{layouts,category_*,flags_*,debug_propagates_sink_errors}`, `ub_safe::log::{any_category_value_is_valid,any_state_flag_bits_are_valid,values_cross_threads}`) pass under Miri. No `ub_unsafe` surface: the workset adds no unsafe public API. |
| equiv | `from_bits` / `contains` use Rust precedence (`&` above `==`), so the masks are what C computes; `Sub` is set difference; `all()` folds the one named flag. Against C: `av_default_get_category` round-trips every named and unnamed value, and `av_opt_set_int`'s gate refuses exactly when `AV_CLASS_STATE_INITIALIZED` is set. No divergence found. |
| internal | `KNOWN` tables carry the 18 and 1 header values in declaration order (`category_constants_carry_the_c_values`, `flags_bits_and_constructors`); `Debug` covers named, unnamed and mixed sets and propagates sink errors. |
| conventions | Anchors `Wraps: AVClassCategory` / `Wraps: AVClassStateFlags` with `Handwritten` (ffibox has no enum primitive), names drop the `AV_CLASS_` prefix the type supplies, no unsafe without a `SAFETY:` comment. |

## Coverage added

C paths belonging to the workset, measured with the `coverage` build
(`gcov -b`, `GCOV_PREFIX` redirected):

- `log.c:246-248` `av_default_get_category`: 100% before and after.
- `opt.c:190-210`, the `AV_CLASS_STATE_INITIALIZED` gate: all lines executed
  before, but the child-to-parent fallback (`199-202`) and the
  "no state flags anywhere" branches were never taken. New `tests/equiv/log.rs`
  tests take every branch: `state_flags_of_the_parent_gate_a_child_option_as_in_c`
  (child class without `state_flags_offset`, parent's word read through
  `AV_OPT_SEARCH_CHILDREN` / `child_next`),
  `state_flags_of_the_child_take_precedence_over_the_parent_as_in_c`, and
  `without_state_flags_nothing_gates_an_option_as_in_c` (direct object and
  parent/child, neither locating flags).

Rust (`llvm-cov` over `--lib`, `equiv` and `ub_safe` filtered to `log::`):
`log.rs` 98.99% lines; in the two types' region the only unexecuted line is
`log.rs:290`, the `" | "` separator between two *named* flags in
`ClassStateFlags`'s `Debug`, unreachable while `INITIALIZED` is the header's
only named flag. Left in place: it is what makes the next flag print right.

## Sanitizer evidence (log suites: `--lib`, `equiv`, `ub_safe`, `ub_unsafe` filtered to `log::`)

- ASan/UBSan (`asan-ubsan` build, `-Zsanitizer=address`): 84 tests pass, no
  `runtime error:` lines.
- TSan (`tsan` build, `-Zsanitizer=thread`): 83 tests pass, no reports.
- BSan (`cargo bsan`, `bsan` build): the type-tier tests and the new
  equivalence tests pass.
- Miri: the FFI-free type tests pass (see above).

## Out of scope, noted for wave 1 (`AVClass`)

`log.c:get_category` guards `avc->category >= AV_CLASS_CATEGORY_NB` but not
the value an `AVClass.get_category` callback returns, which then indexes
`color[16 + AV_CLASS_CATEGORY_NB]` in `colored_fputs` when colour output is
on. A Rust `AVClass` wrapper that lets safe code install a `get_category`
callback returning an arbitrary `ClassCategory` would reach that C
out-of-bounds read; the `ClassCategory` value type itself is open on purpose
and cannot prevent it.
