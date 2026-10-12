# Review: libavutil/hwcontext wave 2, batch 0 (the wave's types)

Route: **type** (1 item). Objective: **review**. Reviewed commit: the wave
tip `126a47005b`, which is the wave's type translation batch. The symbol
review (batch 1) landed `53d9eda85e` and `e81926af2a` while this one ran.
This review is rebased on them.

Item: `AVHWFramesContext` (`libavutil/hwcontext.h`), homed in
`crustify/rust/ffmpeg/libavutil/hwcontext.rs` as `FramesContext`,
`FramesContextRef` and `FramesContextMut`. It is lent by `UninitFrames`
(`context`, `context_mut`), by `FramesReferenceRef::context`, and by the
free callback's handle. The batch also added the Rust hooks over
`free`/`user_opaque`/`pool` (`FramesHooks`, `release_frames_hooks`) and
`DeviceReferenceRef::create_frames`. Its 12 field anchors cover every
field.

Wave 2 is the hwcontext sub-campaign's last wave (`schedule.json`: waves
0-2), so this review ran the sub-campaign's BSan gate (see below). The
symbol review ran BSan only on its own reproducers.

Reports:

- `unsafe/frame-hw-frames-context-seam-left-untyped` (reproducer + fix)
- `wavefront/frame-hw-frames-ctx-copy-through-backend-unrecorded`
- `misc/bsan-gate-lib-constraints-enomem-runaway` (gate triage: one test
  thinned, one known false positive skipped; no code defect)

## Verdicts by step

| step | finding |
|---|---|
| wavefront | The `AVHWFramesContext` record is complete and matches the C code (allocation, init, `hwframe_ctx_free`'s order, every field's ownership; `free`/`user_opaque` claimed only when both are unset). `hwframe_ctx_free` is the dropper on its `data` argument. `--lifetime-for AVHWFramesContext` stays empty, the known limitation (`../../wave-1/batch-0/wavefront/device-context-releaser-unrecorded`). `AVFrame.hw_frames_ctx`'s note (buffer-frame wave 3) missed that C copies any frame without `buf[0]` that holds a context through the context's backend. It was resubmitted (`wavefront/`). |
| lifecycle | No gap. Construction: `alloc_frames` (`av_hwframe_ctx_alloc`) -> `UninitFrames` -> `init` (`av_hwframe_ctx_init`), `create_frames` for the common case, and `FramesReferenceRef::derive`. Release: `ContextUnref` (`av_buffer_unref`), whose last reference runs `hwframe_ctx_free` and the hooks trampoline. Clone: `try_clone`/`Clone` (`av_buffer_ref`, count-checked). C never releases a pool the caller sets; the hooks own and drop it. There is no by-value owner, as there should not be (the struct lives only in an `AVBufferRef`'s data). |
| unsafe boundary | One defect: `unsafe/frame-hw-frames-context-seam-left-untyped`. The wave wrapped the context, but `frame.rs` kept the seams waiting for it: a byte view getter, an untyped `unsafe` setter and an untyped take. The setter's contract also let a contract-honouring caller make the safe `try_clone` reach C's NULL `transfer_data_from` call (ASan SEGV, reproducer). Fixed: typed and safe accessors, and copies of frames without `buf[0]` that hold a context are refused. The workset's own four `unsafe fn`s are justified: `hwctx`/`into_hwctx` (a type-erased struct whose type only the device type implies), `pool` (most backends' pool buffers are surface handles, not bytes) and `set_pool` (the backend takes any buffer for a surface). Their `# Safety` sections state that obligation precisely. The workset has no public raw pointer. `crustify scan-unsafe`: `unsafe_fns_pub_smell` 27 -> 26 (`replace_hw_frames_context`). No other smell counter moved. The undocumented count stays at 24 crate-wide, none of them in the files touched here. |
| ub_safe / ub_unsafe | No UB in the workset's own API beyond the defect above. Checked: the trampoline (pool cleared before C's dangling `pool` could be read, hooks re-entered or replaced by the callback, empty-hook release), `set_pool`/`take_pool` against a C pool and against the hooks' own pool, `user_data` borrows against the only two places that lend a `FramesContextMut` (an `UninitFrames` held exclusively, and the release of the last reference), last releases on other threads (`Send` closure, `Send + Sync` data, `Send` pool), `device()`/`device_context()` lifetimes in the free callback, and `init` being safe while `UninitDevice::init` is not (every safe setter writes a value C validates or a backend takes as it is). The `ub_unsafe` suites for `hwctx`, `into_hwctx`, `pool` and `set_pool` call each within its contract, edge cases included. |
| equiv | No divergence beyond the documented ones (`has_free_callback` reports only a Rust callback when Rust's hooks hold the pair; `init` releases the context on failure). The review adds `equiv/hwcontext.rs::frame_hw_frames_context`, which compares the frames context's reference count and which frames hold it with raw C frames through attach, `av_frame_clone`, `av_frame_ref`, take/replace and release. |
| internal | No further defect. `hwframe_pool_prealloc` reporting an `av_frame_alloc` failure as success is C's behaviour, followed and documented by the batch. |
| conventions | One filled anchor per field; the duplicates (`free`, `user_opaque`, `pool`, `hwctx`) are intended, one per accessor. Getters are on `FramesContextRef`, setters on `FramesContextMut`, Rust names, `addr_of!` projections, a `// SAFETY:` comment on every block. `clippy --all-targets -D warnings` and rustdoc `-D warnings` (also with private items) are clean. The touched files keep the base's rustfmt state: only the review's lines were formatted. No deviation found. |
| misc | The BSan gate triage (`misc/bsan-gate-lib-constraints-enomem-runaway`). Wave 1's `constraints_keep_what_they_have_on_enomem` ran away under BSan (more than 32 CPU minutes, 40 GB) on a 64 Ki-element list. It now uses 512 elements, still above its 1 KiB limit, and takes 1 s. The formats-era `channel_layout_c_initialisers_fill_a_fresh_layout` trips the documented `strtol` end-pointer false positive (`channel_layout.c:417`), even when run alone; the gate skips it. |

## Coverage

Details in `coverage.json`. Rust: `hwcontext.rs` 99.35% of lines, and the
workset's range (lines 2746-3489) misses only `release_frames_hooks`'s
NULL-handle arm, which is unreachable. `frame.rs`: every line the review
added runs. C: `hwcontext.c` 78.27% -> 79.67%. The review's
`alloc_fails_whole_when_the_backend_struct_does_not_fit` uses a test backend
whose frames struct is larger than the context to reach
`av_hwframe_ctx_alloc`'s failure on that struct and its fail path
(`av_hwframe_ctx_alloc` 74.19% -> 93.55%). The two remaining misses there
are allocations smaller than the context, which no `av_max_alloc` limit
isolates. `hwframe_ctx_free`, `av_hwframe_ctx_init`,
`hwframe_pool_prealloc` and `av_hwframe_get_buffer`: 100%.

## BSan gate

I ran it per the sub-campaign policy: one invocation per test binary, in
parallel, each under `timeout 7200` (`cargo bsan test --no-run` binaries,
the bsan C build), with `BSAN_OPTIONS=gc_hang_threshold_ms=600000`. `lib`
ran with `--test-threads=1`. The gate skipped `compile_fail` and the 48
known `strto*` end-pointer false positives in
`../../../options/wave-2/batch-0/misc/bsan-gate-strto-end-pointer-false-positives/skip-list.txt`,
plus `misc/bsan-gate-lib-constraints-enomem-runaway/extra-skips.txt` (one
more instance of that class). BSan reported nothing outside those false
positives on either tree.

- Wave tip `126a47005b`, before any change: `equiv` 697 passed (4688 s),
  `ub_safe` 799 passed (2983 s), `ub_unsafe` 170 passed (242 s), and the
  seven `-sys` binaries passed. `lib` first stopped at the
  `channel_layout.c:417` false positive. With it skipped, 953 tests passed.
  The runaway hwcontext test was stopped after 32 minutes, which made it
  fail (see `misc/`).
- Landed tree (the symbol review's `e81926af2a` plus this review; the wave
  branch did not move afterwards): `equiv` 699 passed (4417 s; the new
  `frame_hw_frames_context` test and the symbol review's test included),
  `ub_safe` 800 passed (2773 s), `ub_unsafe` 169 passed (272 s; one test
  moved to `ub_safe`), and the seven `-sys` binaries passed. `lib`, rebuilt
  after the runaway test was thinned: 957 passed (609 s), the review's new
  unit tests and the thinned test included. That change touches only a
  `#[cfg(test)]` unit test, so the `equiv`, `ub_safe` and `ub_unsafe`
  binaries above, built before it, link the same library code.

## Validation at the landed tree

- `cargo check --workspace`, `cargo clippy --workspace --all-targets -- -D
  warnings`, `RUSTDOCFLAGS=-D warnings cargo doc -p ffmpeg --no-deps` (also
  with `--document-private-items`): clean.
- Plain `cargo test --workspace`: 968 lib, 718 equiv, 809 ub_safe
  (including the compile-fail programs), 181 ub_unsafe, the `-sys` crates
  and the doctests, 0 failed.
- ASan/UBSan and TSan (`-Zbuild-std`, the matching C variants, `--lib
  --tests -- --skip compile_fail`): 967 / 718 / 807 / 181 each, with no
  `runtime error:` and no AddressSanitizer, LeakSanitizer or
  ThreadSanitizer report. The coverage test added afterwards was rerun
  under both (`--lib -- hwcontext`: 84 passed, clean).
- Reproducer: exits 1 on `126a47005b` (ASan SEGV at `hwcontext.c:490`,
  through the safe `FrameRef::try_clone`), ASan/UBSan build. On the fixed
  tree it does not compile, because the seam it uses is gone. The
  regression unit test runs the same scenario through the safe API. It
  passes on the fixed tree, and its process dies with SIGSEGV when the
  refusal is removed.
- Miri: the FFI-free hooks tests
  (`frames_context::hooks_release_without_c_or_a_pool`,
  `device_context::hooks_on_rust_storage_without_c`) pass.
  `frames_context::hooks_on_rust_storage_without_c` creates a C buffer pool,
  so Miri cannot run it. The review's new tests call C.
- `crustify scan-unsafe` against the tip: `unsafe_fns_pub_smell` -1 and
  `unsafe_fns` -1; +4 unsafe blocks (the typed setter's conversions and the
  getter's view), +1 private fn, +1 field projection. Undocumented sites:
  24 crate-wide, as before.
