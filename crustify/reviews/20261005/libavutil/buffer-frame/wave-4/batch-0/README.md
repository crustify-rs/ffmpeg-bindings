# Review: libavutil/buffer-frame wave 4, batch 0 (the wave's symbols)

Route: **symbol** (18 items). Objective: **review**. Reviewed commit: the
wave tip `c6e25ecf93`, which is the wave's one translation batch.

Items, all homed in `crustify/rust/ffmpeg/libavutil/frame.rs`:

- `av_frame_alloc`, `av_frame_free`, `av_frame_clone`, `av_frame_unref`: the
  wave-3 type run's `Frame::new`, `FrameFree`, `FrameRef::try_clone` and
  `FrameMut::reset`. As the symbol route requires, no second wrapper was
  emitted for them.
- `av_frame_get_buffer` (`FrameMut::alloc_buffers`), `av_frame_copy`
  (`copy_data_from`), `av_frame_copy_props` (`copy_props_from`),
  `av_frame_ref` (`try_clone_from`), `av_frame_replace` (`assign`),
  `av_frame_move_ref` (`move_from`, `take`), `av_frame_make_writable`
  (`make_writable`), `av_frame_apply_cropping` (`apply_cropping`,
  `CropFlags`);
- `av_frame_is_writable`, `av_frame_get_plane_buffer`,
  `av_frame_get_side_data` (`FrameRef::is_writable`, `plane_buffer`,
  `find_side_data`, `FrameMut::find_side_data_mut`);
- `av_frame_new_side_data` (`alloc_side_data_zeroed`,
  `alloc_side_data_from_slice`), `av_frame_new_side_data_from_buf`
  (`add_side_data`), `av_frame_remove_side_data` (`remove_side_data`).

Wave 4 is the buffer-frame sub-campaign's last wave (`schedule.json`: waves
0-4), so this review ran the sub-campaign's BSan gate (below).

Reports:

- `ub_safe/audio-copy-sample-count-rounding-overflow` (reproducer + fix)
- `wavefront/get-buffer-sample-count-rounding-overflow`
- `misc/copy-props-failure-doc-claims-references-copied`
- `misc/bsan-gate-equiv-buffer-mutation-sequences-outrun-the-limit` (gate
  triage, test thinned under BSan; no code defect)

## Verdicts by step

| step | finding |
|---|---|
| BSan gate | No BSan report on either tree. On the wave tip, before any change, `lib`, `ub_safe`, `ub_unsafe` and the seven `-sys` binaries passed. `equiv` reached its two-hour limit with 655 of its 656 selected tests reported `ok`. The one still running was wave 2's `buffer::mutation::sequences_mutate_count_and_release_as_c`, which takes about 13 minutes per seed under BSan and runs 16 seeds. It now runs one seed (seed 2) under BSan only. On the landed tree every binary passed (`equiv` 656 in 4127 s). See `misc/` and the BSan section below. | |
| wavefront | 14 of the 18 records match the C and the wrappers. `av_frame_get_buffer`'s note listed its `int` hazards (align past 2^27, non-power-of-two and negative aligns) but not the default-alignment sample rounding that overflows at `INT_MAX - 31` samples. The notes of `av_frame_ref`, `av_frame_clone` and `av_frame_make_writable`, which call it themselves, did not name it either. That gap is the cause of the UB defect below. The four notes were corrected and resubmitted. |
| lifecycle | Not applicable: a symbol route. The four lifecycle routines keep their single wave-3 anchors (`Wraps: av_frame_alloc/free/clone/unref`). Wave 4 added no second wrapper and no second release or clone strategy. |
| unsafe boundary | The wave adds no public `unsafe fn` and no public raw pointer. `crustify scan-unsafe` shows no wave-4 site among the pub smells (the four in `frame.rs`, `set_data_ptr`, `set_extended_data_ptr`, `replace_hw_frames_context` and the `opaque` cookie, are the wave-3 type run's and were reviewed there). The one wave-4 site among the private smells is `FrameRef::reaches(p: *const u8, ..)`, which takes a plane pointer the frame holds, as the wave-3 `locate` does. The `SAFETY` comments of the new blocks were checked against `frame.c`. The review's changes move no smell counter. They add 3 private fns, 1 field projection and 1 unsafe statement, and the undocumented count stays at 22 crate-wide, none of them in `frame.rs`. |
| ub_safe / ub_unsafe | One true defect, reachable from safe code. `alloc_buffers` guarded C's `FFALIGN(nb_samples, 32)` overflow at `INT_MAX - 31` samples, but C also runs `av_frame_get_buffer(dst, 0)` from `av_frame_ref` (non-refcounted source), and so does the wrapper's own copy helper. `make_writable`, `try_clone_from` and `try_clone` (and `assign`'s copy path, which needs unsafe lent planes) reached the overflow (UBSan `samplefmt.c:137`). Fixed: a shared predicate gives C the line size it would round to (`alloc_buffers`, `make_writable`), and the paths where C allocates inside `av_frame_ref` refuse with `EINVAL`. The regressions are a `ub_safe` test, which halts on the UBSan report in a child process, and a unit test. Checked and clean: the cropping model (`size_t` wrap, `ctz` truncation, the `1 << shift` bound, palette planes, `offsets[]` reads, pointer moves inside their reference); `av_frame_copy`'s reach model against `image_copy`/`av_samples_copy` (row width failures, `AV_CEIL_RSHIFT(INT_MIN)`, the palette's unconditional `memcpy`, `av_assert0` aborts, `extended_data` entry counts, `int` products); `get_video_buffer`'s `int` sums for every align the wrapper passes; the separate `extended_data` copies in `av_frame_ref`/`av_frame_replace` (bounded by the invariant's channel count); `assign`'s handling of C's stale `nb_extended_buf`; `copy_props_from`'s NULL-window and count refusals; `plane_buffer`'s bound; side data appends with failing allocations. The wave adds no public unsafe API, so there is no `ub_unsafe` suite to add. |
| equiv | No divergence beyond the documented ones (planes released before `alloc_buffers`, frames reset before `try_clone_from`/`move_from`, refusals where C is undefined). The review's new refusal for non-refcounted sources at `INT_MAX - 31` samples returns `EINVAL`, which is what C returns after its overflow, unless C can allocate a 2 GiB copy (lent planes and a raised `av_max_alloc`). It is documented on `try_clone`, `try_clone_from` and `assign`. No C reference can be run on that input without the UB, so it has no equivalence test. |
| internal | No further defect: `Rows::fit` (forward and backward rows, `usize` overflow), `image_reach`, `c_ctz`, `crop_offsets`, `release_planes` (repairs of C's failure states), `zero_copied_buffers`, `new_side_data` (the payload is written before the entry is lent). The new helpers are pinned by `frame_copies_at_a_sample_count_c_cannot_round`. |
| conventions | One filled anchor per item, duplicated only where two wrappers represent one item (`av_frame_get_side_data`, `av_frame_move_ref`, `av_frame_new_side_data`). Rust names. Getters on `FrameRef`, setters and routines on `FrameMut`. `clippy --all-targets -D warnings` and rustdoc `-D warnings` are clean. The two changed Rust files keep the base's rustfmt state: only the review's lines were formatted. No deviation found. |
| misc | The BSan gate triage above (`misc/bsan-gate-equiv-buffer-mutation-sequences-outrun-the-limit`). `copy_props_from`'s `# Errors` said a failed side data copy leaves "its other properties copied". C returns before it replaces the private and user data references. The doc now says so, and the allocation-limit unit test asserts it. |

## Coverage

Details in `coverage.json`. C: `frame.c` 96.46% of 480 lines. The misses are
hardware paths (no hardware context is wrapped yet), `av_frame_make_writable`'s
own failure returns (the wrapper composes C's steps, and the C routine runs
only as the equivalence reference), and allocation failures that no
`av_max_alloc` limit isolates. Rust: `frame.rs` 99.61% of lines, every
function. The wave-4 region misses 4 of 553 lines: two unreachable defensive
arms, and the zero-swap failure arm, which needs the second of two equal
allocations to fail. Every line the review added runs.

## BSan gate

Run per the sub-campaign policy: one invocation per test binary, in parallel,
each under `timeout 7200` (`cargo bsan test --no-run` binaries, the bsan C
build), with `BSAN_OPTIONS=gc_hang_threshold_ms=600000`. `lib` ran with
`--test-threads=1`. `compile_fail` and the 48 known `strto*` end-pointer
false positives listed in
`../../../options/wave-2/batch-0/misc/bsan-gate-strto-end-pointer-false-positives/skip-list.txt`
were skipped.

- Wave tip `c6e25ecf93`, before any change: `lib` 873 passed (493 s),
  `ub_safe` 781 passed (3892 s), `ub_unsafe` 129 passed (217 s), and the
  seven `-sys` binaries passed. `equiv` was stopped at 7200 s. Its log has
  an `ok` for 655 of the 656 selected tests, and no failure or BSan report.
  The remaining test is
  `buffer::mutation::sequences_mutate_count_and_release_as_c` (above).
- Landed tree (the tip plus this review; the wave branch did not move):
  `lib` 874 passed (530 s), `ub_safe` 782 passed (3424 s, the new
  regression and its child included), `ub_unsafe` 129 passed (226 s), and
  the seven `-sys` binaries passed. These binaries were built from the final
  tree: the later BSan thinning of the buffer test leaves their checksums
  unchanged. `equiv`, rebuilt with that thinning: 656 passed (4127 s; the thinned buffer test included).

## Validation at the landed tree

- `cargo check --workspace`, `cargo clippy --workspace --all-targets -- -D
  warnings`, `RUSTDOCFLAGS=-D warnings cargo doc -p ffmpeg --no-deps` (also
  with `--document-private-items`): clean.
- Plain `cargo test --workspace`: 884 lib, 675 equiv, 790 ub_safe (the
  compile-fail programs included), 141 ub_unsafe, the `-sys` crates and
  the doctests, 0 failed. The wave tip has one test fewer in each of `lib`
  and `ub_safe`.
- ASan/UBSan and TSan (`-Zbuild-std`, the matching C variants, `--lib
  --tests -- --skip compile_fail`): 884 / 675 / 788 / 141 each, with no
  `runtime error:` and no AddressSanitizer, LeakSanitizer or ThreadSanitizer
  report. The later BSan-only change to `equiv/buffer.rs` was rerun under
  plain and ASan (`buffer::`, 11 passed).
- Reproducer: exits 1 on `c6e25ecf93` (UBSan `samplefmt.c:137:22`,
  `halt_on_error=1`) and 0 on the fixed tree (`make_writable: Err(-12)`),
  ASan/UBSan build. The regression test fails on the unfixed wrapper (its
  child exits 1 on the same report) and passes on the fixed one.
- A random-sequence probe (not kept) ran 72k rounds of
  `apply_cropping`/`copy_data_from`/`make_writable`/`try_clone_from`/`assign`/`copy_props_from`/`plane_buffer`/`try_clone`
  over random formats, geometry, line sizes, crops, buffers and plane
  offsets under ASan/UBSan with `halt_on_error=1`. It was clean on the
  fixed tree.
- Miri: the FFI-free frame unit tests (`frame_rows_fit_forward_and_backward`,
  `frame_c_ctz_refuses_truncated_zeroes`). The review's new tests call C and
  are ignored under Miri.
- `crustify scan-unsafe`: no smell counter moved against the tip (+3
  private fns, +1 field projection, +1 unsafe statement), and 22
  undocumented sites crate-wide, as before.
