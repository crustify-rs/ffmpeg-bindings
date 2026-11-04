# Review: `libavutil/frame.c` wave 902, batch 1 (`AVRegionOfInterest` again)

- Item (1 type, homed in `src/libavutil/libavutil/frame.rs`): `AVRegionOfInterest`, fields
  `qoffset`, `right`, `left`, `bottom`, `top`, `self_size`. Wave 902's batch 0 (`AVFrame`) is
  still `todo`, so it lists no commits and is not part of this workset.
- Reviewed commit: `3ad70349610`, on the wave tip `1cc3c27b646`. It adds
  `SideDataRef::regions_of_interest` (a `CSlice<'a, RegionOfInterest>` view over a
  `REGIONS_OF_INTEREST` payload), `FrameMut::add_regions_of_interest` and
  `SideDataArray{,Mut}::add_regions_of_interest` (checked stores), and
  `RegionsOfInterestError`.
- Objectives: lifecycle, unsafety, contracts, ub, equiv, internal, conventions, misc,
  coverage, disclose, fix.
- Reports: 3, each fixed, each with a `disclosure.md`:
  - [`conventions/region-of-interest-stand-ins-lack-anchors/`](conventions/region-of-interest-stand-ins-lack-anchors/)
  - [`misc/invalid-qoffset-message-omits-zero-denominator/`](misc/invalid-qoffset-message-omits-zero-denominator/)
  - [`misc/region-of-interest-rectangle-hazards-undocumented/`](misc/region-of-interest-rectangle-hazards-undocumented/)

## Verdicts by objective

| objective | verdict |
|---|---|
| lifecycle | None. `AVRegionOfInterest` has no lifecycle routine (`query types --name AVRegionOfInterest --lifecycle-ops` lists none). Its storage is side-data bytes, which `AVFrameSideData`'s array routines allocate and free, and the stores use those routines. |
| wavefront findings | No pointer finding to check: the type has no pointer field. Its note now also records the rectangle and offset hazards in the third report. It was submitted with `--update`, and only that record was spliced in. |
| unsafety | None. An `ffimap` scan with `--sites fn.unsafe.pub raw_ptr.fn_sig_arg.safe.pub raw_ptr.fn_sig_ret.safe.pub raw_ptr.field.pub` finds no site on a line the commit added (checked by `git blame`). The new API is safe and takes and returns no raw pointer. |
| contracts | None. The one new `unsafe` block, the view's `CSlice::from_raw_parts` (`frame.rs:1529` at the tip), meets ffibox's contract: `len / ROI_SIZE` initialised elements that live for `'a`. Its `// SAFETY:` holds on every path. The kind, size, alignment and first `self_size` are checked before the block. `SideData`'s invariant gives initialised bytes kept alive by the entry's reference. The bytes are written only through `data_mut`, which needs the buffer's sole, non-read-only reference and an exclusive borrow. Every bit pattern is a valid region. I tried to break it through a shared buffer (`buffer().try_clone()`, a cloned frame or array), through `set_buffer`/`replace_buffer` and removals while a view lives, and across threads. Each either fails to compile or leaves the buffer unwritable. |
| ub | None found. Also checked: the view's lifetime, which the two new compile-fail cases cover; `Send`/`Sync` of the view (`CSlice<T: Sync>`, and `RegionOfInterest` is plain data); and `new_side_data`'s uninitialised window, which `write_regions` fills completely and cannot panic in, because the length is computed from the same slice. The batch's region-of-interest tests pass under ASan/UBSan and TSan (lib 9, ub_safe 8, equiv 5). |
| equiv | No unintended divergence. The stores write what `vf_addroi` writes: every `self_size` = `sizeof`, 28-byte stride, native fields. `c_read` in `tests/equiv/frame.rs` reads them as `libx264`/`vf_showinfo` do. Refusing `0/0`, out-of-range offsets, empty payloads, and views of misaligned or `self_size != 28` payloads departs from C on purpose, and the docs and tests say so. `av_frame_side_data_new`'s `REPLACE`/`UNIQUE`/non-`MULTI` handling comes through unchanged (`NotAdded`). |
| internal | No defect. (`InvalidQoffset`'s public message is filed under `misc`, second report.) `regions_payload_len`, `write_regions`, `payload_bytes` and `qoffset_is_storable` (`i64` arithmetic, so `i32::MIN` cannot overflow) behave as documented. `TooLarge` is unreachable for a real slice (28-byte elements), but harmless. |
| conventions | One finding (first report): the typed store and view lacked the `Wraps` anchors that the byte-level stand-ins of the same C items carry. The tests sit where the conventions place them, the compile-fail cases are under `tests/ub_safe/compile_fail/`, and the C-calling tests carry `cfg_attr(miri, ignore = "calls C")`. |
| misc | Two findings: the error message, and the incomplete hazard documentation (third report). The latter covers the rectangle hazards in `libx264`, `libx265`, `libvpxenc`, `d3d12va_encode`, `vaapi_encode` and `qsvenc`, and the in-range offset product in `vaapi_encode` and `qsvenc` besides `d3d12va_encode`. None of those encoders is in the campaign build, so it is documentation only, with no reproducer. |
| coverage | No reachable gap. Measured with the wave-800 scripts on the `coverage` variant, every Rust line the commit added runs. The workset's one zero-count line is `frame.rs:5990`, `new_side_data`'s `None` on an `av_frame_new_side_data` allocation failure, which depends on the environment. The C paths the stores take (`av_frame_new_side_data`, `av_frame_side_data_new` with and without `REPLACE`) run. |

## Gates and validation

- Miri gate on the wave tip `1cc3c27b646`, before any change, one
  `cargo +nightly-2026-10-08 miri test -p ffmpeg-bindings <binary> -- --skip compile_fail` per
  binary: `--lib` 505 passed (463 ignored), `--test equiv` 30, `--test ub_safe` 151,
  `--test ub_unsafe` 14. No Miri error. A first attempt was stopped and rerun from scratch
  because review edits had been made to the tree while it was still compiling.
- Miri gate on the landed tree, the same invocations: `--lib` 505 passed (463 ignored), `--test
  equiv` 30, `--test ub_safe` 151, `--test ub_unsafe` 14. No Miri error. (`equiv` first failed on
  the parallel-build race, `extern location for libavutil_sys does not exist`, and passed rerun
  alone.)
- BSan: not run as a gate. Wave 902 is not `libavutil/frame.c`'s last translation wave: wave 904
  belongs to it and runs after it. This batch emitted no UB reproducer.
- On the landed tree: `cargo +nightly-2026-10-08 fmt --all --check`,
  `cargo check --workspace --all-targets`, `cargo clippy --workspace --all-targets -- -D warnings`
  and `cargo test --workspace` (lib 968, equiv 723, ub_safe 821, ub_unsafe 182, doctests
  113 + 113) all pass.
- ASan/UBSan and TSan (`CRUSTIFY_BUILD_VARIANT=<variant>
  CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS=-Zsanitizer=<address|thread>
  cargo +nightly-2026-10-08 test -Zbuild-std --target x86_64-unknown-linux-gnu`, filtered to
  `region_of_interest`, `regions_payload` and `write_regions`) on the wave tip: lib 9, ub_safe 8,
  equiv 5 pass with no report under both. The review changes no code these tests run, only doc
  comments, an error string and a unit assertion.
- Coverage, wave tip and landed tree, same scripts: C 9,594 / 13,844 (69.30%), Rust
  14,417 / 14,533 (99.20%), identical. Per suite only `ub_safe`'s Rust moved, 11,633 → 11,632
  lines, from thread timing in a suite the review did not touch.
- `ffimap` (`8cb4031`), wave tip against landed tree: `code.loc.total` +1, `code.tests.loc.total`
  +5, and the anchored-wrapper counts `fn.ffibox.wrapper.non_trait` +2 and
  `ty.ffibox.wrapper.fields.total` +1, the three new anchors. No unsafe or raw-pointer counter
  moved.

## Outside the workset

These were noted, not filed:

- `SideDataMut::set_kind` and the byte-level stores can still produce a `REGIONS_OF_INTEREST`
  entry with any bytes. The type's docs say so, and no libavutil reader parses the payload. The
  built readers are in `libavfilter`, whose wrappers must use the checked view.
- `FrameMut::new_side_data`, and `SideDataArrayMut::into_new_entry` on its append path, run
  `init` after C has added the entry. A panicking `init` would leave an uninitialised payload in the array. Every in-tree
  `init` (`zero_into`, `copy_into`, `write_regions`) is panic-free, so nothing reaches it today.
- `Cargo.lock`'s ffibox entry is rewritten (0.1.0 → 0.1.1) by every build, from the `[patch]`
  path, and was not committed.
