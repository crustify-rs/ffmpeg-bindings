# Review: rational arithmetic, rescaling, names, escape, time strings

Route: **symbol** (26 items). Objective: **review**. Affected base:
`23dc07587c751c8f7c82729839a4b0065c66bb5f` (`av_make_q`, `av_rescale_rnd`
and `av_ts_make_time_string2` were wrapped earlier and re-checked here).

## Findings and scope

- One equivalence defect, fixed: `Rational::nearest` refused every list with
  two `INT_MIN` denominators, while C compares such a pair (and overflows)
  only when the first of them is the nearest so far. Report, reproducer and
  regression tests in `equiv/nearest-refuses-lists-c-finishes`. The same
  patch corrects `cmp_distance`'s doc, which claimed C always overflows on
  unordered candidates (it multiplies `INT_MIN` by -1, 0 or 1).
- No UB defect in the safe or unsafe public API. The one public unsafe
  function the batch adds, `default_category`, keeps the context seam the
  log module already uses (`default_item_name`, `log_with_context`) for
  `AVClass`-enabled objects whose types are not wrapped yet; its contract
  states alignment, readability, the non-NULL class pointer and the
  no-concurrent-write obligation, and its `ub_unsafe` suite exercises it.
- No Wavefront finding needed correction: every pointer argument and return
  of the 26 symbols (`q_list`, `last`, `dst`/`src`/`special_chars`, `buf`,
  `tb`, `ptr`, the static name) is recorded with the ownership, nullability,
  cardinality and lifetime the C bodies show.
- No lifecycle work: the route is symbol-only and `AVRational` owns nothing.
- No convention deviation: clippy (`--all-targets -D warnings`) and rustdoc
  (`-D warnings`) are clean; every unsafe block carries a specific comment.

### Method

Besides reading each C body against its wrapper's guards, a differential
probe (standalone package, `/tmp/batch1/probe`, 300k pseudo-random and edge
iterations against the asan-ubsan C library) ran three sweeps:

- `safe`: every safe wrapper of the batch; no UBSan report and no ASan
  report.
- `equiv`: wrapper results against the raw C calls wherever the wrapper
  accepts the input; no mismatch.
- `reject`: every input a wrapper refuses, replayed one per process with
  `UBSAN_OPTIONS=halt_on_error=1` (UBSan reports a location once per
  process, so in-process correlation is misleading). Every refused
  `cmp_distance` (266/266), `to_f32_bits`, `gcd`, `add_stable` (increments
  within `int`), `rescale_delta` and `Add` input (150 sampled each) made C
  report an overflow, a division by zero or a sign shift, so those guards are
  exact. `from_f64`'s negative bound is defined but meaningless in C for most
  values and overflows (`-INT_MIN` in `av_reduce`) for `i32::MIN`, so the
  rejection stays. `nearest`: 37 of 150 refused lists finished in C; hence
  the defect above.

## Regression evidence and coverage

Added:

- `tests/equiv/rational.rs`:
  `nearest_walks_two_min_denominators_as_av_find_nearest_q_idx`, six lists
  against every edge rational with a C-driven oracle for the pair C would
  overflow on;
- `rational.rs` unit test
  `nearest_rejects_two_min_denominators_only_when_c_compares_them`.

`coverage.json` records the batch's tests (lib, equiv, ub_safe, ub_unsafe,
filtered to the six homes' items) against the gcov C build and under Rust
`-C instrument-coverage`. Every C function of the worklist is at 100% of
its lines except `av_q2intfloat` (82.6%): its sign normalization is never
reached because the wrapper normalizes in Rust to avoid C's `sign<<31`.
`rational.rs`, `mathematics.rs` and `timestamp.rs` are at 100% of lines;
the batch's items in the shared `avstring.rs`, `avutil.rs` and `log.rs`
(`escape`, `EscapeFlags`, `MediaType::as_str`, `PictureType::to_char`,
`time_base`, `default_category`) are 100% per `llvm-cov --show-functions`.
The one uncovered region in `mathematics.rs` is `rescale_delta`'s second
`checked_sub(a)?`: with `last >= 2a - b` and both bounds halves of an `i64`
that difference cannot overflow (the rescales of `2·in_ts ± 1` overflow
monotonically), so the branch is defensive and unreachable. The header
inlines (`av_make_q`, `av_inv_q`, `av_q2d`, `av_cmp_q`,
`av_ts_make_time_string`) live in the `-sys` shim, outside the gcov build.

## Validation

All commands ran from `crustify/rust` with private target and log paths
under `/tmp/batch1`; the reusable C builds were left immutable
(`GCOV_PREFIX` redirected coverage writes). No binding, shim or C source
changed, so no C rebuild or baseline was needed.

- `cargo check --workspace`, `cargo clippy --workspace --all-targets --
  -D warnings`, `RUSTDOCFLAGS=-D warnings cargo doc -p ffmpeg`: pass.
- `cargo test --workspace`: 910 passed, 0 failed (compile-fail and
  doctests included).
- ASan/UBSan: whole-crate `--lib --tests` under `-Zsanitizer=address`
  against `CRUSTIFY_FFMPEG_VARIANT=asan-ubsan`, `--skip compile_fail`: pass,
  no `runtime error`, no leak.
- TSan: same suites under `-Zsanitizer=thread` against the tsan build: pass.
- BSan: lib, equiv and ub_safe `rational` suites with `cargo bsan test`
  against the bsan build: pass.
- Miri: the C-free unit tests of `rational.rs` and `mathematics.rs`
  (rejections before the call, layout, conversions, handles): pass.
- `crustify scan-unsafe <workdir> --name <26 symbols> --json`: no
  undocumented unsafe; raw sites only at `default_category`'s documented
  context seam and `escape`'s out-slot local.
