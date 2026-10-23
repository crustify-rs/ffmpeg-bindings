# Review: libavutil/buffer-frame wave 2, batch 0 (types)

- Item: `AVFrameSideData` (`frame.rs`: `SideData`/`SideDataRef`/`SideDataMut`,
  the hand-written owner `SideDataArray`, its handles `SideDataArrayRef`/
  `SideDataArrayMut`, the iterators, `SideDataFlags`, and
  `SideDataArrayMut::add`/`SideDataArray::add` over `av_frame_side_data_add`)
- Reviewed: wave tip `83bd5f2e64` (type batch 0). The wave's symbol share
  belongs to review batch 1. The schedule runs to wave 4, so wave 2 is not
  the sub-campaign's last wave and BSan ran on no suite. This review emits
  no Rust reproducer to run under it.
- Reports: three.
  - `wavefront/side-data-lifecycle-roles-unsubmitted`:
    `query symbols --lifetime-for AVFrameSideData` was empty. The type run
    recorded no role on `av_frame_side_data_free`/`_add` (both of which it
    bound) or on `_new`/`_clone`/`_remove`/`_remove_by_props`. Roles and
    pointer records were resubmitted. It also notes, from reading the code,
    a C NULL dereference in `av_frame_side_data_new` (OOM with `REPLACE`)
    for wave 3.
  - `conventions/equiv-side-data-clone-safety-comment-detached`: rustfmt
    put a `let x =` line between a `// SAFETY:` comment and its `unsafe`, so
    `scan-unsafe` counted the site as undocumented (23 -> 22 after the fix).
  - `misc/side-data-invariant-omits-av-frame-new-side-data`: the type's
    invariant docs named only `av_frame_side_data_new` as leaving a payload
    uninitialised. `av_frame_new_side_data` (wave 4) does too. Docs fixed.

## Verdicts by step

| step | finding |
|---|---|
| wavefront | The field records match `side_data.c`. `data` is borrowed from `buf`, nullable only with size 0, and uninitialised after `_new`. `buf` is an owned counted reference, never NULL, and its struct is not written through the field. `metadata` is an owned dictionary, NULL when empty. The type's comment is accurate. No lifecycle role was recorded on any symbol (report, resubmitted). |
| lifecycle | Complete for this wave. An entry is never owned alone: C allocates and frees entries only inside the array routines, and `sizeof` is outside the ABI. So the owner is the array, and its release is `av_frame_side_data_free` (`Drop for SideDataArray`). The constructor is `av_frame_side_data_add` (`add`). `NEW_REF` is expressed by passing a `try_clone`. The array has no C cloner, since every C copy loop is the caller's own. The entry cloner (`av_frame_side_data_clone`), the uninitialising constructor (`_new`) and the partial droppers (`_remove`, `_remove_by_props`) are wave-3 symbols, as the wave-1 review left `av_buffer_realloc` and the like to wave 2. |
| unsafe boundary | `scan-unsafe --sites all` over `frame.rs`: the workset's public unsafe API is `SideDataArray::from_raw_parts`, and the only public raw pointers are it and `into_raw_parts`. These are the owner's adoption and surrender seams, `pub` like `CBox::from_c`/`into_c` because higher layers and integration tests hand C-built arrays across. The `# Safety` text was checked against `av_frame_side_data_free` and the entry invariant: a non-negative count, an `av_malloc`-family block of distinct entries, nothing else using it, and `_new` payloads written. It is complete and unambiguous. The other sites are `define_ctype!` seams and private projections. One lexically undocumented block in the equiv test (report). |
| ub_safe | No defect. Checked: `data()`'s `'a` against `set_buffer`/`replace_buffer`, `set_metadata` and the array's drop (the borrows are tied to the entry handle, and the compile-fail doctests pin them). `data_mut`'s sole-and-writable check covers clones of the entry's reference, entries cloned into other arrays (count 2) and `from_read_only` buffers (`READONLY`). Its ordering is acq_rel unref against a seq_cst load. `replace_buffer` takes the new reference's own window, which stays initialised by `BufferReference`'s invariant (pool, `alloc` and `resize` growth give `UninitBuffer`). Iterators and `get_mut` never lend two handles to one entry, since entries are distinct and each index is yielded once. Every `Send`/`Sync` proof was checked against the buffer and dictionary proofs. Free callbacks run during `add(UNIQUE)`/drop: `from_owner`'s `T: 'static` cannot reach the borrowed array, so re-entry ends in a `RefCell` panic, which aborts in the `extern "C"` callback, or a lock deadlock, never aliasing. No safe constructor exists for a `SideData` value. Its fields are private and there is no `CZeroable`, policy or `CVal` path. |
| ub_unsafe | No defect within `from_raw_parts`'s contract. The existing suite covers NULL/0, exact-size payloads, C shrinking the array between round trips, arrays sharing buffers dropped in either order, `NEW_REF` references kept by the caller, and moves across threads. |
| equiv | No divergence. `add` is a direct call: C's `UNIQUE`-then-fail removal, `REPLACE` in place with metadata freed, refusal without `REPLACE` (the buffer comes back) and `MULTI` appends all pass through unchanged, and the random-sequence suite compares them after every step. The getters read the fields C reads. `len()` maps a NULL `data` to 0, which C never pairs with a nonzero size. |
| internal | No defect. `from_raw_parts` reads a negative count as empty, which matches C's loops. The owner's handles project the two variables C updates. `Drop` passes them by address. |
| conventions | One report (above). Anchors: one `Wraps: AVFrameSideData`; `Field:` anchors on all five fields (`type` on `kind`/`set_kind`; `buf` and `metadata` on the getter and every setter); `Wraps: av_frame_side_data_free` plus `Handwritten` on the owner; `Wraps: av_frame_side_data_add` on `add`. Getters are on `Ref` and setters on `Mut`, and projections use `addr_of!`/`addr_of_mut!`. |
| misc | One report (above). |

## Coverage

One coverage run (`CRUSTIFY_FFMPEG_VARIANT=coverage` with `GCOV_PREFIX`, plus
`-C instrument-coverage`, over the lib, `equiv`, `ub_safe` and `ub_unsafe`
binaries filtered to `side_data`, at the wave tip):

- Rust: the side data block of `frame.rs` (the `SideData` `define_ctype!`
  through `SideDataIterMut`) has 275 executable lines, all executed, and no
  unexecuted region (`llvm-cov export` segments).
- C (`gcov -b` over `side_data.c`): `av_frame_side_data_free`, `_remove`,
  `_remove_by_props`, `_get_c`, `free_side_data_entry`,
  `replace_side_data_from_buf`, `ff_frame_side_data_add_from_buf` and
  `add_side_data_from_buf_ext` reach 100%. `av_frame_side_data_add` is at
  93% (line 250, `NEW_REF`'s unref after a failed addition). The safe API
  never passes `NEW_REF`. The remaining misses are in wave 3's routines:
  `av_frame_side_data_new`'s failure unref (224), and
  `av_frame_side_data_clone`'s OOM paths and `remove_side_data_by_entry`
  (reached only when `av_dict_copy` fails).
- The review adds no test: the review changes are a test-statement rewrite
  and docs, and no gap is reachable.

## Validation of the landed tree

Plain `cargo check`/`clippy`/`clippy --all-targets -D warnings`/`test
--workspace` and `cargo doc -p ffmpeg` with `-D warnings`. ASan+UBSan and
TSan over the whole `ffmpeg` crate (lib, `equiv`, `ub_safe`, `ub_unsafe`;
2459 passed, no report). Miri on the three FFI-free side data unit tests.
`scan-unsafe` was run again: no count changed except
`undocumented_unsafe` (23 -> 22). No BSan (see above).
