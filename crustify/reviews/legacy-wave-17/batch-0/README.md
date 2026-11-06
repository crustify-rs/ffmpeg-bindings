# Review: libavutil/formats wave 3, batch 0 (the wave's one type)

- Item: `AVColorPrimariesDesc` (`csp::ColorPrimariesDescriptor`, with
  `ColorPrimariesDescriptorRef` / `ColorPrimariesDescriptorMut`), field
  anchors `wp` and `prim`.
- Home: `crustify/rust/ffmpeg/libavutil/csp.rs`.
- Reviewed commit: the wave tip `e0a75b5d7f` (the type landed by batch 0
  `0545d10678`; batches 2 `3d65561a6e` and 1 `e0a75b5d7f` landed the wave's
  symbols).
- Reports: none. No step below found a defect to report; the review adds one
  equivalence test (below) and this record.

## Verdicts by step

| step | finding |
|---|---|
| BSan gate | Clean on the wave tip: `lib`, `equiv`, `ub_safe` and `ub_unsafe` each exit 0 under `cargo bsan test` (bsan C build, `gc_hang_threshold_ms=600000`, one invocation per binary in parallel under `timeout`), no report. Re-run clean on the landed tree (figures below). |
| wavefront | The batch-0 finding matches the C: two by-value fields (`wp`, an `AVWhitepointCoefficients` = `AVCIExy`; `prim`, an `AVPrimaryCoefficients`), 64 bytes of `AVRational`s with no pointer, so no `ptr` block to submit and `pending: []`; `--taking` / `--returning --api-only` list only `av_csp_primaries_id_from_desc` and `av_csp_primaries_desc_from_id`, `--lifetime-for` nothing, `--users --api-only` the same two; `non_opaque_in` (swscale `csputils` / `format` / `cms`, `vf_colorspace`, png, `dovi_rpu*`) reads fields or copies by value, and `AVDOVIDmLevel9` / `AVDOVIDmLevel10` embed it by value. Nothing to resubmit. |
| lifecycle | C has no allocator, disposer, cloner or constructor for the type; its only producer hands out `const` pointers into `csp.c`'s static tables, and all-zero is the tables' own unspecified entry. The Rust side is complete for a value type: `new` / `from_raw` / `into_raw` / `zeroed` / `Default` / `From` both ways / `Copy`, the whole-value `get` / `set`, owners through the generic `av_malloc` family (`HeapBox<ColorPrimariesDescriptor>`, `alloc_cell_array_zeroed`) and by-value embedding in a `#[repr(C)]` parent (now exercised, see equiv). `csp.h` has no macro constructor for it. |
| unsafe boundary | No public `unsafe fn` or raw pointer on the type's surface; `crustify scan-unsafe --sites all` reports no smell counter in `csp.rs` and zero undocumented unsafe sites there. Every field access projects with `addr_of!` / `addr_of_mut!` through the handle pointers under a specific `SAFETY:` comment; the `Send` / `Sync` proofs hold (`Ref` getters only lend getter-only `CieXyRef` / `PrimaryCoefficientsRef`, `Mut` setters and exclusive field handles need `&mut`, `&Mut` reaches only `as_ref`). |
| ub_safe / ub_unsafe | No UB reachable: every coordinate is an unchecked `int` pair, as in C, and no path reads one as a divisor; the exclusive field handles borrow the parent handle exclusively (the four `compile_fail` programs reject a handle outliving its value, a write while shared, a setter on a shared field handle and two exclusive field handles at once). Whole-crate ASan/UBSan, TSan and BSan pass, and the eight FFI-free descriptor unit tests pass under Miri (`--skip equality`: `Rational`'s `==` is `av_cmp_q`). The workset adds no public unsafe API, so there is no `ub_unsafe` obligation. |
| equiv | The suite compares C's tables entry by entry, rebuilds every gamut through the coordinate handles with `av_csp_primaries_id_from_desc` reading the Rust-owned `av_malloc` block in place after each of the 16 steps, moves the white point past the 1/1000 tolerance and back, and checks `==` against `av_cmp_q` on every coordinate (`0/0` unequal to itself, as `av_cmp_q`'s `INT_MIN`; swscale's private `ff_q_equal` treats NaN as equal, but no public C routine does, and the docs state the semantics). Added: `embedded_in_a_c_parent_is_read_by_c_in_place_after_each_step`, the by-value embedding C itself uses (`AVDOVIDmLevel10` mirrored in `#[repr(C)]`, sizes and the field offset asserted), written through `as_mut()` on the field and identified by C through the parent's pointer after every step, with the neighbouring scalars checked untouched. While writing it: C's table holds identical entries for SMPTE 170M and SMPTE 240M, so `av_csp_primaries_id_from_desc` identifies both as 170M (the first within tolerance); the suite compares against C's own identification, not the index. No divergence found. |
| internal | The type has no private helper of its own (`gamma` belongs to the wave-1 getters); `from_ptr` adoption of raw storage and NULL rejection are unit-tested. No defect. |
| conventions | `Wraps: AVColorPrimariesDesc` on the type, `Field: AVColorPrimariesDesc.wp` / `.prim` on each getter, exclusive field handle and setter; getters on `Ref`, setters on `Mut`; Rust names (`ColorPrimariesDescriptor`, `white_point`, `primaries`); rustdoc builds with `-D warnings`; clippy `--all-targets -D warnings` clean; the batch's additions and this review's are rustfmt-clean, the three files keeping exactly the base's pre-existing hunks (5 / 1 / 2, all in wave-1 luma tests). |
| misc | Nothing to report. (Not a defect, noted for a later ergonomics pass if wanted: the white point and primaries exclusive handles cannot be held at once; a `split`-style accessor over the two disjoint fields would be sound, as the siblings `PrimaryCoefficientsMut` and `CieXyMut` lack one too.) |

## Coverage

Rust (`llvm-cov` over the lib, `equiv`, `ub_safe` and `ub_unsafe` binaries,
csp tests, at the reviewed tree): `csp.rs` 944 lines, 1592 regions and 165
functions, all 100%. C (`gcov` variant, csp tests): the type has no C routine
of its own; its two readers, which wave 4 wraps, execute 100% of their lines
(`av_csp_primaries_id_from_desc` 21/21, `av_csp_primaries_desc_from_id` 7/7).
Nothing left to fill. Details in `coverage.json`.

## Validation at the landed tree

The landed tree is this review rebased onto the symbol review's fixes
(`c3ad0048d6`), which add five lib, one ub_safe and no equiv tests of
their own; every pass below was re-run after that rebase.

- `cargo check --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `RUSTDOCFLAGS=-D warnings cargo doc -p ffmpeg --no-deps`: clean.
- plain `cargo test --workspace`: 563 lib, 463 equiv, 478 ub_safe (the
  compile-fail programs included), 72 ub_unsafe, 83 doctests, 0 failed.
- ASan/UBSan and TSan (`-Zbuild-std`, the matching C variants, whole
  crate): 563 / 463 / 477 / 72 each, no `runtime error:` and no sanitizer
  report.
- BSan (`cargo bsan`, bsan C build, whole crate, one invocation per binary
  under `timeout`): on the wave tip `e0a75b5d7f`, before any change: lib
  558 (79 s), equiv 462 (24 min), ub_safe 476 (41 min), ub_unsafe 72
  (4 min), each exit 0, no report; on the landed tree: 563 (2 min) / 463
  (22 min) / 477 (37 min) / 72 (4 min), each exit 0, no report. The added
  test runs in 0.5 s alone under BSan (its sibling setter-sequence test
  0.7 s).
- Miri: the 8 FFI-free descriptor unit tests.
- `crustify scan-unsafe --sites all`: no smell counter and no undocumented
  unsafe site in `csp.rs`, before and after the added test.
