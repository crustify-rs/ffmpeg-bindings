# Review: `libavutil/opt.c` wave 794, batch 0 (`AVOption`, `AVOptionRanges`)

- Items (2 types): `AVOption` (9 field anchors) and `AVOptionRanges` (3), homed in
  `src/libavutil/libavutil/opt.rs`. Their wrappers are `OptionDef` / `OptionDefRef` /
  `OptionDefMut` and `OptionRanges` / `OptionRangesRef` / `OptionRangesMut`, with
  `OptionRangesBox = CBox<OptionRanges, OptionRangesFree>`.
- Reviewed commit: `2d3ce1afec5`, on the wave tip `a89fdf10d60`. It adds tests only:
  `tests/equiv/opt.rs::option_def` (3 tests) and an extension of
  `tests/ub_safe/opt.rs::lookups`. It also adds the batch's dashboard rows and Notes. The
  public API it reviews is the one earlier batches left (`opt.rs` lines 80-1046).
- Objectives: lifecycle, unsafety, contracts, ub, equiv, internal, conventions, misc,
  coverage, disclose, fix. Wave 794 is not the module's last translation wave
  (`schedule.json` still has 854, 856 and 858 for `libavutil/opt.c`), so this batch owns the
  Miri gate but not the module-end BSan gate.
- Reports: none. No step found a defect, so there is nothing to disclose or fix, and the
  Rust tree is unchanged.

## Verdicts by objective

| objective | verdict |
|---|---|
| lifecycle | Complete. `query symbols --lifetime-for AVOptionRanges` lists `av_opt_freep_ranges` as the dropper and `av_opt_query_ranges` / `av_opt_query_ranges_default` as `alloc` constructors through the out-parameter. C has no cloner or disposer. The Rust matches: `OptionRangesFree::c_drop` passes a local slot to `av_opt_freep_ranges`. `query_ranges` adopts the result with `from_c` only on success (NULL becomes `AVERROR_BUG`). The box is not `Clone` (a compile-fail doctest checks this). `AVOption` has no lifecycle: every table is `static const`, so it is only borrowed. |
| wavefront findings | Correct. `AVOption`: `name`, `help`, `unit` and `default_val.str` are nullable `static`-borrowed strings, and `default_val.arr` a nullable `static`-borrowed scalar. The union discriminant in the type's comment matches `av_opt_set_defaults2` (`opt.c:1774-1836`) case for case. The getters tie each string to the handle's lifetime, which is stricter than `static`. `AVOptionRanges.range`: owned array of owned, nullable pointers, `nb_ranges * nb_components` long and component-major. This matches `opt.h:523-532` and `av_opt_freep_ranges`. |
| unsafety | None. An `ffimap` scan with `--sites fn.unsafe.pub raw_ptr.fn_sig_arg.safe.pub raw_ptr.fn_sig_ret.safe.pub` reports no safe-interface raw pointer in `opt.rs`. Its public `unsafe fn` sites in the workset (line 855) are `CDrop::c_drop` alone, ffibox's trait method. The private `slots` view keeps `*const ffi::AVOptionRange` elements, which no public signature exposes. |
| contracts | None broken. Each getter's `// SAFETY:` rests on the handle pointing at a live table entry. A handle comes only from `ClassRef::options`, `ObjectRef::find` or `ClassRef::find`, or from the unsafe `from_ptr` seams. `OptionDef` has no public constructor (its tuple field is private, and it is not `CZeroable`). No safe path installs an option table: `Class::new` has none, and `AVClass.option` has no setter. The union reads in `default_value` select the member C reads for the same type. `OptionRangesMut::get_mut` takes `&mut self`, so at most one `OptionRangeMut` exists at a time, even if a class callback stored one range twice. |
| ub | None found. The `Send`/`Sync` impls on both families (`ffimap` `impl_send_sync` sites 81 and 786) were checked against C. Option tables are immutable statics. Range lists are `av_malloc` trees that the caller owns outright and frees through `av_free` from any thread. `get` and `slots` stay in bounds: both counts come from `int`, so their product cannot overflow, and a negative count reads as 0. Under `av_opt_query_ranges` without `AV_OPT_MULTI_COMPONENT_RANGE`, C forces `nb_components = 1`, and the Rust view reads no further than that. The opt suites pass under ASan/UBSan and TSan (equiv 44, ub_safe 20, ub_unsafe 47 `opt::` tests each), including the new four-thread read of `cpuflags` inside the log callback. |
| equiv | No divergence. The new `option_def` tests compare every entry of the three fixture tables with the raw struct. They check each default by storing it through C's typed setter, `av_d2q(dbl, INT_MAX)` for `RATIONAL` included (as `av_opt_set_defaults2` does), and each named constant against what `av_opt_set` stores. Constants are taken from `i64` for a `DOUBLE` target as well, as `DEFAULT_NUMVAL` does (`opt.c:423`). The existing `equiv/opt.rs` lookup tests already compare `help`, `unit`, `offset` and `flags` with C (`rust_ident` / `c_ident`). |
| internal | No defect. `opt_c_str`, `RangeQuery` / `RangeTarget`, `slot` / `slots` and `OptionType::{name, base, array, is_array}` behave as documented, and the unit tests cover them. |
| conventions | No deviation. Every field anchor has a getter with its `Wraps` anchor and `#[doc(alias)]`. `default_val`'s members share one getter, which carries the five anchors. `default_val.q` has no stand-in, with a documented reason (`opt.h:454` marks it unused, and nothing in the tree reads it). The new tests are in `tests/equiv/opt.rs` and `tests/ub_safe/opt.rs`. `ub_safe` keeps `#![forbid(unsafe_code)]`. The Miri-ignored tests carry `#[cfg_attr(miri, ignore = "calls C")]`, and Miri runs the one that does not call C (`every_entry_reads_as_c_stores_it`). |
| misc | None in the workset. |
| coverage | No gap. All four workloads were run against the `coverage` variant, then `gcov`: every line of `av_opt_query_ranges` (`opt.c:2536`), `av_opt_query_ranges_default` (`:2554`) and `av_opt_freep_ranges` (`:2626`) runs. Rust, from `-C instrument-coverage`: no line of `opt.rs` 80-1046 (both types' wrappers) has a zero count. |

## Gates and validation

All runs were on the tree this review lands, whose Rust is identical to the wave tip
`a89fdf10d60`.

- `cargo check --workspace --all-targets`, `cargo +nightly-2026-10-08 fmt --all --check`,
  `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`
  (lib 964, equiv 721, ub_safe 816, ub_unsafe 181, doctests 111 + 113): all pass.
- Miri gate, one `cargo +nightly-2026-10-08 miri test -p ffmpeg-bindings <binary> -- --skip
  compile_fail` per binary: `--lib` 501 passed (463 ignored), `--test equiv` 30 passed,
  `--test ub_safe` 151 passed, `--test ub_unsafe` 14 passed. There was no Miri error. Each
  ignored test carries its `cfg_attr(miri, ignore = …)` reason.
- ASan/UBSan and TSan
  (`CRUSTIFY_BUILD_VARIANT=<variant> RUSTFLAGS=-Zsanitizer=<address|thread>
  cargo +nightly-2026-10-08 test -Zbuild-std --target x86_64-unknown-linux-gnu`, with
  `LD_LIBRARY_PATH` set to the variant's `lib*`): the `opt::` tests of equiv, ub_safe and
  ub_unsafe pass with no report.
- BSan: not run. Wave 794 is not the module's last wave, and this batch emitted no
  reproducer.
- The dashboard rows reuse wave 794's coverage (68.58% / 99.25%): the Rust tree, the tests
  and the C are unchanged, so the figures are the ones measured at that landing.

## Outside the workset

These were noted but not filed, since neither is in this batch's code:

- The `coverage` variant in `crustify/artifacts/build.json` is built with
  `--toolchain=gcov`, and the dashboard's C figures come from `gcov`. The testing spec's
  Coverage section asks for clang source-based coverage read by `llvm-cov`. This
  campaign-wide method predates this wave.
- The `[patch]` source of ffibox (`/opt/venv/share/ffibox`) is now version 0.1.1, while
  `Cargo.lock` records 0.1.0, so every build rewrites the lock. This review did not commit
  that change.
