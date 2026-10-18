# Review: libavutil/buffer-frame wave 1, batch 0 (types)

- Items: `AVBufferRef` (`buffer.rs`: `BufferReference`, `BufferReferenceBox`,
  `BufferUnref`, `UninitBuffer`), `AVRegionOfInterest` (`frame.rs`:
  `RegionOfInterest`)
- Reviewed: wave tip `1a9271f931` (type batch 0). The symbol share of the wave
  belongs to review batch 1. Wave 1 is not the sub-campaign's last wave (the
  schedule runs to wave 4), so BSan ran on no suite. This review emits no Rust
  reproducer to run under it.
- Reports: two, on one finding.
  - `misc/region-of-interest-self-size-hazards`: the type's docs and its
    `CZeroable` SAFETY comment said that C readers reject only a zero
    `self_size`, and do so with an error. In fact an undersized nonzero
    `self_size` makes them read past the side data payload (ASan, C probe
    included), and `vf_addroi` aborts on 0. Docs fixed. No Rust path reaches
    C with a region yet.
  - `wavefront/region-of-interest-reader-checks-incomplete`: the same facts,
    plus the encoders' unchecked `qoffset` arithmetic, were missing from the
    type's record. Resubmitted.

## Verdicts by step

| step | finding |
|---|---|
| wavefront | `AVBufferRef`'s field records (`data` borrowed from `buffer`, nullable only with size 0; `buffer` an owned counted reference, never NULL) match `buffer.c`. So do the lifecycle roles of `av_buffer_alloc`/`allocz`/`create` (constructors), `av_buffer_ref` (up-ref cloner, unchecked relaxed increment) and `av_buffer_unref` (dropper, acq_rel decrement). `AVRegionOfInterest`'s comment was incomplete (report, resubmitted). |
| lifecycle | Complete for this wave. `AVBufferRef`: `av_buffer_unref` (`CDrop`), `av_buffer_ref` (`CDupClone`, count-guarded), `av_buffer_alloc` (`UninitBuffer`), `av_buffer_allocz`, and `av_buffer_create` in three ownership variants (`HeapVec` with the default free, a `Send` Rust owner, a read-only Rust owner). `av_buffer_realloc` (which can also construct from NULL), `av_buffer_make_writable`, `av_buffer_replace` and `av_buffer_pool_get` are wave-2 symbols. The struct's size is outside the ABI, so no by-value owner is missing. `AVRegionOfInterest` has no C lifecycle routine: it is a value type. |
| unsafe boundary | `scan-unsafe --sites all`: in the workset only `with_allocator` and `UninitBuffer::assume_init` are `unsafe_fns_pub_smell`. Both are legitimate. `with_allocator` cannot express in types its no-re-entry rule (a self-deadlock on a default mutex) or the size-from-the-buffer's-start rule (`AVBuffer` is opaque). `assume_init` is the promotion of written bytes, and `fill`/`copy_from_slice` are its safe forms. I checked both `# Safety` texts against `pool_alloc_buffer`, `buffer_pool_flush` and `buffer_pool_free`. The re-entry obligation is complete: a pool reference held by `F` or by an owner `T` forms a cycle that leaks rather than frees. The other sites are trait seams (`c_drop`/`c_dup`) and the allocator trampoline's `Box<F>` deref. There is no undocumented unsafe block in the homes or their tests. |
| ub_safe | No defect. I checked: `data()`'s `'a` against `replace` and `UninitBuffer::try_from` (`as_ref` reborrows); `data_mut`'s sole-and-writable check against the C count's ordering (acq_rel release vs seq_cst load); the `Send`/`Sync` proofs against every C writer of a shared `AVBuffer`; `from_owner`/`from_read_only` with non-`Sync` owners (Rust never touches `T` between `as_mut`/`as_ref` and the release); empty and dangling windows; that `CBox::with_policy`/`new_uninit` give no safe path around `BufferUnref` (`with_policy` is unsafe, no `CAlloc`); and the count guard under concurrent clones. A pool allocator only takes sole, writable references (`UninitBuffer`), so the pool never takes over a shared buffer's free callback. |
| ub_unsafe | No defect within `with_allocator`'s and `assume_init`'s contracts. The existing suites cover every element of both. |
| equiv | No divergence beyond the documented one (`try_clone` refuses at `u32::MAX / 2` references, where C would wrap later). |
| internal | No defect. |
| conventions | No deviation. Every item has one `Wraps:` anchor, and the duplicates are intentional: `UninitBuffer` (a second owner form) and three `av_buffer_create` variants. All three `AVBufferRef` fields and all six `AVRegionOfInterest` fields carry `Field:` anchors, and `Handwritten` marks the hand-written policies and owner. Getters are on `Ref` and setters on `Mut`, and projections use `addr_of!`/`addr_of_mut!`. |
| misc | One report (above). |

## Coverage

One coverage run (`CRUSTIFY_FFMPEG_VARIANT=coverage`, `-C instrument-coverage`,
the lib, `equiv`, `ub_safe` and `ub_unsafe` binaries filtered to `buffer` and
`region_of_interest`):

- Added `buffer.rs::unit_tests::copies_stop_at_the_reference_limit`. It
  raises a real buffer's count through a mirror of `AVBuffer`'s private
  layout, which it first checks against `av_buffer_get_ref_count`. With that it
  covers `try_clone`'s refusal at `MAX_REFERENCES` through the handle and the
  box, and counts above `INT_MAX` (negative `int`s from C). The translator
  had left that branch uncovered, since reaching it honestly needs 2^31 live
  references.
- Rust: `buffer.rs` 99.06% lines (746 lines, 7 missed), 98.94% regions. In
  the merged line view only three lines have a zero count: a test allocator
  closure that is never called on purpose
  (`construction_failure_drops_the_allocator`). The report's other misses are
  per-instantiation arms of the generic `with_allocator` (its init2-failure
  path, taken by another instantiation). In `frame.rs` the
  `RegionOfInterest` block (lines 915-1250) runs all 103 of its executable
  lines. The rest of `frame.rs` is other waves' items, outside this run's
  filter.
- C (`gcov -b` over `buffer.c`): `av_buffer_alloc`, `allocz`, `ref`, `unref`,
  `is_writable`, `get_ref_count`, `default_free` and `buffer_create` all run
  at 100%. Three lines remain uncovered. `av_buffer_create` (75%): the
  `AVBufferRef` allocation failing after the 48-byte `AVBuffer` succeeded,
  which `av_max_alloc` cannot select since the reference is smaller.
  `buffer_replace` (84.6%): its `src` branch, reached only by wave 2's
  `av_buffer_make_writable`/`av_buffer_realloc`. `av_buffer_pool_init`/`init2`:
  `ff_mutex_init` failures, which depend on the environment.
  `AVRegionOfInterest` has no C path of its own.

## Validation of the landed tree

Plain `cargo check`/`clippy`/`clippy --all-targets -D warnings`/`test
--workspace`, `cargo doc -p ffmpeg` with `-D warnings`, ASan+UBSan and TSan
over the whole `ffmpeg` crate (lib, `equiv`, `ub_safe`, `ub_unsafe`), and Miri
on the workset's FFI-free unit tests. No BSan (see above).
