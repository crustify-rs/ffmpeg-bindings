# Review: AVBPrint, AVClass, ff_pad_helper_AVBPrint

Route: **type**. Objective: **review**. Affected base:
`b2cff27cd691af1482bd36e8c2edab9d5e25fcf8`.

## Findings and scope

- Three corrected Wavefront findings: the sizeof helper does not own a
  print buffer's storage; a class/name can be temporary; finalization can
  clear an allocated `str` by freeing it as well as transferring it.
  Each report includes the exact `--update` submission.
- Two Rust documentation/test defects: the static-only class description,
  and BPrintBox's no-Clone doctest accidentally cloning a reference.
- No new runtime UB, functional equivalence defect, or missing lifecycle
  representation was confirmed. No public unsafe API was added.

Lifecycle discovery used the type records, field touchers, dependency
closure, and constructor/disposer records, cross-checked against C bodies
and callers. `av_bprint_init` and `av_bprint_init_for_buffer` are the two
initializers; `av_bprint_finalize` disposes/transfers the owned string, not
the descriptor. The existing stationary CBox storage and separate finalizing
versus caller-buffer policies are necessary and complete. There is no C
cloner. The helper has no runtime C users; Class has no dedicated allocator,
destructor, or cloner. AVOption/AVOptionRanges dependencies already use their
safe handles. Object-specific callback contexts remain narrow raw seams for
higher-layer context types not yet wrapped.

The unsafe helper-string setter's contract covers every copy and every
thread. The four class callback handles retain their documented object,
result-lifetime, and allocation contracts. Existing UB tests exercise them;
ffibox adoption seams remain internal-test escape hatches rather than new
unsafe public wrapper APIs.

## Regression evidence and coverage

Added:

- safe, isolated descriptor-allocation failures (both owners, empty and
  nonempty caller buffers), proving no caller bytes are touched on failure;
- C/Rust multi-call equivalence after allocation failure, repeated truncated
  appends, explicit shortening, recovery, repeated heap growth, and finalization;
- local Class handles shared/moved through scoped threads and exclusive
  reborrows, plus an internal temporary descriptor borrowing a heap name;
- corrected no-Clone compile-fail doctest, checked by nightly rustdoc.

`coverage.json` records independent before/after runs, using the prepared
coverage C library plus Rust `-C instrument-coverage`. Rust `bprint.rs`
regions improve **590/591 -> 591/591**; the entire Class production surface
has **196/196** covered source regions. C `av_bprint_alloc` improves
**13/14 -> 14/14** blocks; both initializers and finalize remain **100%**.
Class-related name/category and option/child iteration paths are **100%**.

Remaining coverage is outside this type surface: the default-callback
selection in `av_opt_query_ranges` is not taken by an explicit QueryRangesFn
handle (absent callback is represented by None); unscheduled bprint escape,
get-buffer, and most strftime paths are not exposed by these types. The
whole log.rs file is 1608/1620 regions, with remaining regions in other
symbols' formatting/overflow paths and test fallback branches. The helper
has no C execution path. OOM tests force deterministic av_max_alloc failures;
actual OS allocator exhaustion is intentionally not induced. Timing-sensitive
logging stress tests have nondeterministic execution counts.

## Validation

All commands ran from `crustify/rust`, with agent-private target/log/output
paths under `/tmp/crustify-review-b0`. The reusable C builds were left
immutable (GCOV_PREFIX redirected all coverage writes). `build.json` matches
the orchestrator's version 3; C sources are unchanged from `1019f8f036`.
Configured compilers/instrumentation match: clang 18.1.3 ASan/UBSan and TSan,
matched BSan clang 23.1.3 plus plugin, GCC 13.3 gcov. No bindings, shim, or C
source changed, so no C rebuild/baseline was needed.

- `cargo check --workspace`: pass.
- `cargo clippy --workspace`: pass.
- `cargo test --workspace`: pass on stable, linked with clang's ASan/UBSan
  runtime against `CRUSTIFY_FFMPEG_VARIANT=asan-ubsan`, including trybuild
  compile-fail and doctests.
- Fully instrumented Rust ASan + sanitized C: workspace unit/integration
  suites pass with `cargo +nightly test --workspace --lib --tests
  -Zbuild-std --target x86_64-unknown-linux-gnu -- --skip compile_fail`,
  `RUSTFLAGS=-Zsanitizer=address`, leak checking and UBSan halt-on-error.
  Workset doctests also pass under nightly.
- TSan: all bprint/log tests pass against the TSan C build using nightly
  build-std and `-Zsanitizer=thread`, including the threaded handle tests.
- BSan: all bprint tests, Class unit tests, Class equivalence/safe suites,
  callback unsafe suites, and the new tests pass against the BSan C build
  with `cargo bsan test`. A broader unrelated multi-megabyte logging-symbol
  comparison was stopped for cost; the selected type suites were then run
  separately to completion. The existing >4 GiB caller-buffer test skips
  BSan because its shadow memory is impractical; it passes ASan and TSan.
- Miri: eight safe helper-field tests, five unsafe borrowed-string tests,
  and the new local-Class scoped-thread test pass. Foreign calls cannot run
  under Miri; ASan/UBSan, BSan, and TSan cover those paths instead.
- `crustify scan-unsafe <workdir> --name AVBPrint AVClass
  ff_pad_helper_AVBPrint --json`: no illegal sites; zero nonconforming
  wrappers, wrapped field references, out-of-impl projections, or
  out-of-impl raw dereferences. Remaining raw pointers are documented layout,
  lifecycle-policy, FFI, and higher-layer callback seams.

Nightly-only whole-workspace limitations: three pre-existing trybuild stderr
snapshots differ from nightly diagnostics (all illegal programs still fail
compilation); the stable suite passes. Two unrelated HeapBox/OptionRangesBox
no-Clone doctests still request E0599 while producing E0308 under nightly.
The corresponding BPrintBox defect belongs to this workset and is fixed here.
