# Review: libavutil/formats wave 4, batch 0 (the wave's two symbols)

- Items: `av_csp_primaries_desc_from_id` (`ColorPrimaries::descriptor`) and
  `av_csp_primaries_id_from_desc` (`ColorPrimariesDescriptorRef::identify`),
  route `symbol`.
- Home: `crustify/rust/ffmpeg/libavutil/csp.rs`.
- Reviewed commit: the wave tip `7cf5d6fc5d` (the wave's one batch).
- Wave 4 is the formats sub-campaign's last wave, so this review ran the
  sub-campaign's BSan gate (figures below).
- Reports: one, `misc/identify-doc-promised-exact-rational-sums` (a doc
  defect; the code is unchanged). The review adds one equivalence test.

## Verdicts by step

| step | finding |
|---|---|
| BSan gate | Clean on the wave tip `7cf5d6fc5d` before any change: `lib`, `equiv`, `ub_safe` and `ub_unsafe` each exit 0 under `cargo bsan test` (bsan C build, `gc_hang_threshold_ms=600000`, one invocation per binary in parallel under `timeout`), no report on any wave's items. Re-run clean on the landed tree (figures below). |
| wavefront | Both findings match the C. `av_csp_primaries_desc_from_id`: a nullable, immutable, `'static` borrowed return into `csp.c`'s two `static const` tables (`(unsigned)` range checks on both, NULL for a slot whose `prim.r.x.num` is 0); every caller (`pngenc`, `libjxlenc`, `fflcms2`, `vf_colorspace`, `vf_tonemap_opencl`, swscale `format`/`cms`) reads or copies by value. `av_csp_primaries_id_from_desc`: one non-nullable, read-only, unretained `const AVColorPrimariesDesc *` argument, dereferenced unconditionally. The UB claim (every `int` input is defined) re-derived: `av_sub_q`'s cross products are at most `INT_MAX * 100000`, `av_add_q`'s at most `INT_MAX^2` each, so `int64` never overflows, and `av_reduce` bounds every numerator by `INT_MAX`, so `abs()` is defined. Nothing to resubmit. |
| lifecycle | Not applicable: a symbol-only workset over value types; `descriptor` lends C's table entries as `'static` shared handles, nothing is allocated or freed. |
| unsafe boundary | No public `unsafe fn` or raw pointer on either item; `crustify scan-unsafe --sites` reports no smell counter and no undocumented unsafe site in `csp.rs`. Both unsafe blocks are the FFI calls with specific `SAFETY:` comments (checked against the C: by-value enum in, pointer into a never-written `static const` table out; a read-only dereference of the handle's pointer during the call). |
| ub_safe / ub_unsafe | No UB reachable: `descriptor` passes an enum by value and C range-checks it as `unsigned`; `identify` hands C a live description and C only reads it. The `ub_safe` suite (`primaries_lookup`) already covers every lookup value across threads, every owner form (value, `HeapBox`, the last element of an `av_malloc` array, C's table) at every coordinate edge, reborrows and post-write reads; the four FFI-free descriptor unit tests pass under Miri (`identify_answers_zero_denominators_without_c` included). Whole-crate ASan/UBSan (no `runtime error:`), TSan and BSan pass. The workset adds no public unsafe API, so there is no `ub_unsafe` obligation. |
| equiv | The batch's suite compares the lookup for every value past both ranges and the `unsigned` extremes, the identification of every table entry in place and from copies, a tolerance sweep, and 4800 random single-coordinate edit steps on independently owned descriptions compared after each step; the corrected zero-denominator cases are checked for `None`. One gap found and filled: the suite never placed a coordinate at a denominator above `INT_MAX / 100000`, where C's `av_reduce` stops being exact. Added `reduced_sums_follow_c_not_exact_arithmetic` (3000 random placements around 1/1000 plus three fixtures `1e-13` inside the tolerance that C rounds to exactly 1/1000): the wrapper equals C on every one, and the fixtures show it follows C's reduction rather than exact arithmetic, which its doc had promised (see `misc/`). No divergence from C found. |
| internal | The only private logic is `identify`'s zero-denominator early return (unit-tested without FFI, under Miri). No defect. |
| conventions | `Wraps:` anchors on both; `descriptor` on the `Copy` enum newtype by value like its sibling `luma_coefficients`, `identify` on the `Ref` handle as a getter; Rust names; rustdoc builds with `-D warnings` (the doc rewrite included); clippy `--all-targets -D warnings` clean; the review's additions are rustfmt-clean and the two files keep exactly the base's pre-existing rustfmt hunks (2 in `tests/equiv/csp.rs`, 1 in `csp.rs`, all wave-1 luma code). |
| misc | `identify`'s doc claimed the eight differences are summed "exactly as rationals"; C reduces every term to `int` rationals, so a description `~2e-13` inside the tolerance can identify as nothing. Doc fixed, equiv regression added, report in `misc/identify-doc-promised-exact-rational-sums` with the probe that found it. The same paragraph overstated the zero-denominator divergence (C agrees for a single infinite coordinate); reworded in the same fix. |

## Coverage

Rust (`llvm-cov` over the lib, `equiv` and `ub_safe` binaries, csp tests, at
the reviewed tree): `csp.rs` 1006 lines, 1676 regions and 173 functions,
all 100%. C (`gcov` variant, csp tests): `csp.c` 100% of 271 lines and
180/180 branches; `av_csp_primaries_desc_from_id` 7/7 lines (both tables,
both NULL causes), `av_csp_primaries_id_from_desc` 21/21 (skipped slots,
match, no match). Nothing left to fill. Details in `coverage.json`.

## Validation at the landed tree

The landed tree is the wave tip plus this review's doc fix and test
(no sibling batch in wave 4, so no rebase).

- `cargo check --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `RUSTDOCFLAGS=-D warnings cargo doc -p ffmpeg --no-deps`: clean.
- plain `cargo test --workspace`: 565 lib, 469 equiv, 482 ub_safe (the
  compile-fail programs included), 72 ub_unsafe, 85 doctests, 0 failed.
- ASan/UBSan and TSan (`-Zbuild-std`, the matching C variants, whole
  crate, `--skip compile_fail`): 565 / 469 / 481 / 72 each, no
  `runtime error:` and no sanitizer report.
- BSan (`cargo bsan`, bsan C build, whole crate, one invocation per binary
  under `timeout 7000`, the four in parallel): on the wave tip
  `7cf5d6fc5d`, before any change: lib 565 (97 s), equiv 468 (26 min),
  ub_safe 481 (40 min), ub_unsafe 72 (3 min), each exit 0, no report; on
  the landed tree: 565 (94 s) / 469 (22 min) / 481 (36 min) / 72 (3 min),
  each exit 0, no report. The added test runs in 2.2 s alone under BSan.
- Miri: the four FFI-free descriptor unit tests.
- `crustify scan-unsafe --sites`: no smell counter and no undocumented
  unsafe site in `csp.rs`, before and after the change.
