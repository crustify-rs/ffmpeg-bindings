# Review: libavutil/buffer-frame wave 3, batch 1 (the wave's symbols)

- Items (8), all homed in `crustify/rust/ffmpeg/libavutil/frame.rs`:
  `av_frame_side_data_add`, `av_frame_side_data_clone`,
  `av_frame_side_data_free`, `av_frame_side_data_get` (static inline),
  `av_frame_side_data_get_c`, `av_frame_side_data_new`,
  `av_frame_side_data_remove` and `av_frame_side_data_remove_by_props`.
  Translation batch 1 (`356733671e`) wrapped six of them:
  `SideDataArrayRef::find`, `SideDataArrayMut::{find_mut, remove,
  remove_by_props, add_clone, alloc_zeroed, alloc_from_slice}`, and the
  owner twins. The wave-2 type run (`83bd5f2e64`) had already emitted
  `_add` (`SideDataArrayMut::add`) and `_free` (`SideDataArray`'s `Drop`).
  This review covers all eight.
- Reviewed commit: the wave tip `4f959b26a4`.
- Wave 3 is not the sub-campaign's last wave: `schedule.json` lists waves
  0-4 for `buffer-frame`. BSan therefore ran only on this review's
  reproducer.
- Reports:
  - `ub_safe/side-data-clone-replace-leaks-partial-metadata`: reproducer,
    fixed. Found by LSan.
  - `wavefront/side-data-clone-replace-metadata-leak-unrecorded`:
    resubmitted.
  - `unsafe/side-data-add-entry-returns-raw-pointer`: fixed.

## Verdicts by step

| step | finding |
|---|---|
| lifecycle | Not applicable: the worklist has symbols only. Checked anyway. `SideDataArray`'s `Drop` (`av_frame_side_data_free`) releases every entry, buffer reference and dictionary. C has no array-wide clone, and entries are never owned on their own. Every constructor path is in place: `new`, `add`, `alloc_*`, `add_clone`, and `from_raw_parts` for arrays C built. |
| wavefront | One analysis gap, resubmitted. The `av_frame_side_data_clone` record omitted that the in-place `REPLACE` path leaks a metadata copy that fails part way. It also omitted that both metadata copies use flags 0, which merges keys equal up to ASCII case. The other seven records were checked against `side_data.c` and `frame.h` and are right. That includes `_new`'s recorded NULL-dereference defect, which the wrappers avoid. |
| unsafe boundary | The workset's public API has no `unsafe fn` and no raw pointer. The only raw seams on these owners are `SideDataArray::from_raw_parts` / `into_raw_parts`, the wave-2 interop seams for arrays C owns, and their `# Safety` text is correct. `scan-unsafe` found one smell in the workset: the private `add_entry` returned the new entry as `*mut AVFrameSideData` (`raw_ptr_priv_smell`). It is now a consuming `into_added`. The remaining side-data smell sites are the hand-written handles' pointer fields and the wave-2 type's private `entry(i)`. |
| ub_safe / ub_unsafe | One true defect, through the safe API. `add_clone` with `REPLACE` over an entry of the same non-`MULTI` kind leaks the partly copied metadata dictionary when copying the source's metadata fails after its first entry. This is a C bug (`side_data.c:275-277`), reachable through the safe `set_max_alloc` or real out-of-memory conditions. LSan reports 36 bytes in 4 allocations. Fixed by running that path in Rust, in C's order, freeing the partial copy. Regression: `tests/ub_safe/frame.rs::side_data_array_routines::in_place_clones_free_a_partial_metadata_copy`, which fails under ASan on the affected commit. No `ub_unsafe` suite: the workset adds no public unsafe API. Also checked, with no defect: lookups and removals at every count; payload initialisation before an entry is lent; the `alloc_*` replacement guard; `MAX_REFERENCES` on clones; clones whose entry already shares the source's buffer; borrows across removal and replacement (compile-fail cases); and threads. |
| equiv | No divergence found in the landed wrappers. The fix moves `add_clone`'s in-place replacement out of C, so the new `tests/equiv/frame.rs::side_data_array_routines::in_place_clones_match_c` compares that path with C's. It covers sources whose metadata C's copy merges (keys differing only in case, exact duplicates) or keeps, destination entries with and without metadata, entries already sharing the source's buffer, and an unnamed kind. In an isolated child it also runs every `av_max_alloc` limit from 0 to 64 bytes for sources whose copy C cannot leave part way. The existing random-sequence and limit tests still pass. |
| internal | No further defect. `into_new_entry`'s uninitialised view, `copy_into`'s length check (unreachable: C allocates exactly `size` bytes) and the count conversions in `find` hold. |
| conventions | No deviation. Each item has one `Wraps:` anchor: `_new` on both `alloc_*` (an intended duplicate), and `_free` on `SideDataArray` with `Handwritten`. The `add_clone` docs now state C's case-merging metadata copy. |
| misc | None. |

## Validation

- `cargo check --workspace`, `cargo clippy --workspace --all-targets -- -D
  warnings` and `cargo test --workspace` (plain): lib 862, equiv 665,
  ub_safe 777, ub_unsafe 139, doctests 101, all passing.
  `RUSTDOCFLAGS=-D warnings cargo doc -p ffmpeg` is clean.
- The whole `ffmpeg` crate under ASan/UBSan: no report and no
  `runtime error:`. Lib 862, equiv 665, ub_safe 775 (compile-fail
  skipped), ub_unsafe 139. Under TSan the same counts pass with no
  report.
- The regression test fails under ASan when `frame.rs` is swapped back to
  the affected commit (LSan in the isolated child: 2688 bytes in 288
  allocations), and passes on the fix.
- BSan: only on the reproducer, alone, on the fixed tree. No report; BSan
  does not check for leaks.
- Miri: not run. The changed code calls C on every path, and the FFI-free
  helpers (`zero_into`, `copy_into`) are unchanged.
- `scan-unsafe` before and after: `raw_ptr_priv_smell` 65 -> 64,
  `raw_ptr_rets` 263 -> 262, `unsafe_blocks` 1513 -> 1516 (the fields the
  new in-place path writes). No new undocumented unsafe; `frame.rs` has none.

## Coverage

Details are in `coverage.json`. Rust: all 232 instrumented lines of the
workset region of `frame.rs` run, including both failure returns of the
new in-place path. C: every line of the workset's `side_data.c` functions
runs, except two that the wrappers cannot reach:

- `av_frame_side_data_add:250`, the `NEW_REF` cleanup. The safe API does
  not offer that flag.
- `remove_side_data_by_entry:97`. Its only caller, `_clone`, removes the
  entry it just appended, which is always the last.
