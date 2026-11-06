# Review: `libavutil/log.c` wave 800, batch 0 (the logging symbols again)

- Items (8 symbols, homed in `src/libavutil/libavutil/log.rs`): `av_vlog`, `av_log`,
  `av_log_once`, `av_log_format_line`, `av_log_format_line2`, `av_log_default_callback`,
  `av_default_item_name`, `av_default_get_category`.
- Reviewed commit: `c0f53d14a81`, on the wave tip `3df5deb2cbe`. It adds `LogContext<'a>`.
  `Record::context` now returns `Option<LogContext<'a>>`. The five context-taking functions
  take `impl Into<LogContext<'a>>`. The one new public `unsafe fn` is
  `LogContext::as_object`.
- Objectives: lifecycle, unsafety, contracts, ub, equiv, internal, conventions, misc,
  coverage, disclose, fix.
- Reports: 3, each fixed, each with a `disclosure.md`:
  - [`contracts/log-context-requirements-omit-category-bound/`](contracts/log-context-requirements-omit-category-bound/)
  - [`contracts/equiv-record-context-test-claims-objectref-contract/`](contracts/equiv-record-context-test-claims-objectref-contract/)
  - [`wavefront/ownership/av-default-item-name-not-static-and-stale-log-notes/`](wavefront/ownership/av-default-item-name-not-static-and-stale-log-notes/)

## Verdicts by objective

| objective | verdict |
|---|---|
| lifecycle | Not applicable. The batch bound symbols of a non-foundational module, none with a lifecycle role in the store (`lifetime: null` on all eight), and `LogContext` is a borrow with nothing to release. |
| wavefront findings | One finding. `av_default_item_name`'s return and inner class were recorded as `static`, which `vs2av_log`'s stack class disproves. Five context notes still described a raw `void *`. Resubmitted. The other findings (nullability, mutability, the `arg:` borrows, `av_log_once`'s `state`) agree with `log.c` and with the wrappers. |
| unsafety | None. An `ffimap` scan with `--sites fn.unsafe.pub raw_ptr.fn_sig_arg.safe.pub raw_ptr.fn_sig_ret.safe.pub raw_ptr.field.pub` finds one public `unsafe fn` in the workset, `LogContext::as_object` (`log.rs:1233`). It must stay unsafe: no type can check that a context's options describe it. The only raw pointer on `log.rs`'s safe interface is `Class::as_c_ptr` (`log.rs:504`), which belongs to the `AVClass` wrapper, not this batch. |
| contracts | Two findings. `LogContext`'s invariant omitted the bound on `get_category`'s result. A test's `// SAFETY:` claimed `ObjectRef`'s contract for fixtures that break it. `as_object`'s `# Safety` asks for exactly `ObjectRef`'s contract, which it hands out. `Record::context`'s claim about C's context is now the full list `av_log` relies on. The overflow check reads the level offset under the contract's alignment and liveness clauses. |
| ub | None found. `LogContext` is `!Send` and `!Sync`, covariant, and from a `Record` it cannot outlive the callback (compile-fail cases cover both). `item_name`'s `&'a CStr` is tied to the borrow, or to the class for `default_item_name`. Logging in a record's context from inside the callback only re-enters `av_vlog`, which reads. The `ffimap` UB smells in the workset (`casted_to.std.string` at `log.rs:1747`, the `LogContext::from_ptr` read) were checked. `Send`/`Sync` sites in `log.rs` (443-457) are the `AVClass` wrapper's, not this batch's. The `log::` tests pass under ASan/UBSan and TSan. |
| equiv | No divergence. `LogContext::item_name` follows `format_line`'s `cls->item_name ? … : av_default_item_name`. `category` is documented as the callback's or the class's value, not the default callback's colour category (`log.c:259-270` adds a version gate there). `None` for a class-less context matches C, which logs it as the general log. |
| internal | No defect. `LogContext::from_ptr`, `as_ptr`, `addr`, `ptr_eq`, `Debug` and `shift_overflows` behave as documented. |
| conventions | No deviation. `LogContext` stands in for no C item, so it carries no anchor. The eight items' anchors and aliases are unchanged and in the current form. Tests sit where the conventions place them, and `ub_safe` keeps `#![forbid(unsafe_code)]`. |
| misc | None beyond the reports. |
| coverage | No reachable gap. Against the `coverage` variant (gcov for C, `-C instrument-coverage` for Rust, the wave-800 method), every line of `log.rs` in the workset runs; the one zero-count line, 329, is in `ClassStateFlags`' `Debug`. In `log.c` the only workset line that does not run is 411, the repeat counter printed when stderr is a terminal, which depends on the environment. |

## Gates and validation

- Miri gate on the wave tip `3df5deb2cbe`, before any change: one
  `cargo +nightly-2026-10-08 miri test -p ffmpeg-bindings <binary> -- --skip compile_fail`
  per binary. `--lib` 501 passed (463 ignored), `--test equiv` 30 passed, `--test ub_safe`
  151 passed, `--test ub_unsafe` 14 passed. No Miri error. (`--lib` first failed on a
  build race between the parallel invocations, `extern location ... does not exist`, and
  passed rerun alone on the same tree.) The review changes no code Miri runs: one doc
  comment in `log.rs`, and an `equiv` test that calls C and is Miri-ignored.
- BSan: not run. Wave 800 is not yet the module's end: translation waves 796 and 798 of
  `libavutil/log.c` are still `todo` in `schedule.json` and will land after it. The
  module-end gate belongs to the review that follows the last of them. This batch emitted
  no UB reproducer.
- On the landed tree: `cargo +nightly-2026-10-08 fmt --all --check`,
  `cargo check --workspace --all-targets`, `cargo clippy --workspace --all-targets -- -D
  warnings`, and `cargo test --workspace` (lib 964, equiv 722, ub_safe 817, ub_unsafe 182,
  doctests 111 + 113) all pass.
- ASan/UBSan and TSan (`CRUSTIFY_BUILD_VARIANT=<variant> RUSTFLAGS=-Zsanitizer=<address|thread>
  cargo +nightly-2026-10-08 test -Zbuild-std --target x86_64-unknown-linux-gnu`, with
  `LD_LIBRARY_PATH` set to the variant's `lib*`): the `log::` tests of lib (24), equiv
  (28), ub_safe (21) and ub_unsafe (37) pass with no report under both.
- Coverage, wave tip and landed tree, same scripts: C 9,594 / 13,844 (69.30%), Rust
  14,289 / 14,405 (99.19%), identical. Per suite only `ub_safe`'s Rust moved, 11,510 →
  11,509 lines, from thread timing in a test the review did not touch.
- `ffimap` (`8cb4031`), wave tip against landed tree: every counter is equal except
  `code.tests.loc` (+9 total, +7 unsafe), the rewritten test.

## Outside the workset

These were noted, not filed:

- The dashboard's Overview lines (tests, LoC) and its missing `Coverage stats` table were
  last kept before the current coverage method, and no recent batch fills them.
- `Cargo.lock`'s ffibox entry is rewritten (0.1.0 → 0.1.1) by every build, from the
  `[patch]` path, and was not committed.
