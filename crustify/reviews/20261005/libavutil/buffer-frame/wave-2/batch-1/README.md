# Review: libavutil/buffer-frame wave 2, batch 1 (the wave's symbols)

- Items (15), all homed in `crustify/rust/ffmpeg/libavutil/buffer.rs`:
  `av_buffer_alloc`, `av_buffer_allocz`, `av_buffer_create`,
  `av_buffer_get_opaque`, `av_buffer_get_ref_count`, `av_buffer_is_writable`,
  `av_buffer_make_writable`, `av_buffer_pool_buffer_get_opaque`,
  `av_buffer_pool_get`, `av_buffer_pool_init`, `av_buffer_pool_init2`,
  `av_buffer_realloc`, `av_buffer_ref`, `av_buffer_replace` and
  `av_buffer_unref`. Translation batch 1 (`0265949f74`) wrapped eight of them.
  The type runs of waves 0 and 1 (`c68b39206c`, `1a9271f931`) had already
  wrapped the other seven (`alloc`, `allocz`, `create`, `pool_init`,
  `pool_init2`, `ref`, `unref`). This review covers all fifteen.
- Reviewed commit: the wave tip `13730d672b`.
- Wave 2 is not the sub-campaign's last wave (`schedule.json` lists waves 0-4
  for `buffer-frame`), so BSan ran only on this review's reproducer.
- Reports:
  - `ub_unsafe/pool-buffer-resized-in-place-under-its-entry` (reproducer,
    fixed)
  - `misc/uninit-view-over-initialised-rust-owner` (fixed; no prepared
    instrument detects it)
  - `wavefront/pool-buffer-in-place-realloc-unrecorded` (resubmitted)

## Verdicts by step

| step | finding |
|---|---|
| lifecycle | Not applicable: the worklist has symbols only. Checked against `buffer.c` anyway. `BufferUnref` (unref/ref) and `BufferPoolUninit` are complete. Every constructor returns its owner (`BufferReferenceBox`, `UninitBuffer`, `BufferPoolBox`). `with_allocator`'s state is released by the `pool_free` trampoline, or on failure. |
| wavefront | One analysis defect, resubmitted. The records of `av_buffer_realloc`, `av_buffer_pool_init(2)` and `av_buffer_pool_get`, and the `AVBufferPool.alloc/alloc2` type fields, missed that `pool_alloc_buffer` keeps an allocator buffer's reallocatable flag. That leaves the pool buffer resizable in place, under its entry. `av_buffer_create`'s Rust mapping was updated for the new `UninitBuffer::from_owner`. All other properties of the 15 records were checked and are unchanged. |
| unsafe boundary | No public raw pointer or `unsafe fn` that a safe form could replace. `scan-unsafe` finds four smell sites in the workset, all deliberate. `BufferPool::with_allocator` keeps a contract that types cannot express: no re-entry into the pool, and size bytes from the buffer's start. `UninitBuffer::assume_init` is the usual initialisation promise. `get_with_opaque` and `opaque` return raw cookies, because the only opaques are crate-private boxes or C bookkeeping. Both `# Safety` texts are correct. `with_allocator`'s now links the new `UninitBuffer::from_owner`. The counters are unchanged apart from the new code (no new smell). |
| ub_safe / ub_unsafe | One true defect, through the unsafe API kept within its contract. A `with_allocator` pool whose allocator returns `alloc_resizable` buffers gets a first buffer that the safe `resize` let C `av_realloc` in place. The pool then recycled the freed block: ASan heap-use-after-free, also reported by BSan (`ub_unsafe/pool-buffer-resized-in-place-under-its-entry`). Fixed by running C's copy path in Rust for any buffer with an opaque. Regression: `tests/ub_unsafe/buffer.rs::resizable_allocator_buffers_resize_by_copy`. Also checked, with no defect: counts and writability under concurrent clones; `make_mut`/`resize`/`assign` on shared, read-only, advanced and NULL windows; `MAX_REFERENCES` on both assignment forms; and the pool across threads. |
| equiv | No divergence found. The workset's sequences (mutators, `get_with_opaque`, pools) pass. The fix moves pool and owner-backed buffers onto a Rust copy path. That path is compared against C by `equiv/buffer.rs::mutation` (owner buffers, after every resize) and by the new `equiv/buffer.rs::pool_resize` (default-pool buffers, filled or not, at every size pair: bytes, count, writability, opaque, whether the block moved, and whether the next request recycled it). |
| internal | No further defect. The mutators' status mapping, the NULL-window paths, the assignment limits and the ENOMEM children still pass. New unit test: `uninit_conversion_refuses_buffers_with_an_opaque`. |
| conventions | No deviation in the workset. Each item has a `Wraps:` anchor; `av_buffer_create`, `av_buffer_realloc` and `av_buffer_replace` carry deliberate duplicates. The new `UninitBuffer::from_owner` adds a fourth `Wraps: av_buffer_create`. |
| misc | One soundness defect that no prepared instrument can show. `UninitBuffer::try_from` exposed a `from_owner` buffer's initialised Rust bytes as `MaybeUninit`, so safe code could hand the owner's `Drop` uninitialised `u8`s. Pool allocators built that way spread it to every recycled buffer. Fixed: the conversion refuses buffers with an opaque, `get` builds its views directly, and the new `UninitBuffer::from_owner` takes owners of `MaybeUninit<u8>`. |

## Validation

- `cargo check --workspace`, `cargo clippy --workspace --all-targets -D
  warnings`, `cargo test --workspace` (plain), and `cargo doc -p ffmpeg` with
  `-D warnings`.
- The whole `ffmpeg` crate under ASan/UBSan (no report, no `runtime error:`)
  and under TSan (no report): lib 842, equiv 657, ub_safe 752, ub_unsafe 132
  tests. Equiv then gained one more test, run plain and under ASan.
- Miri on the FFI-free `buffer` unit tests (7 run, 15 ignored as FFI).
- BSan only on the reproducer: it reports the use after free on the affected
  commit and runs clean on the fix.

## Coverage

Details in `coverage.json`. C: every line of the workset functions in
`buffer.c` runs except three branches, all environment-dependent. Two are
`ff_mutex_init` failures. The third is `av_buffer_create:66-67`, the
`AVBufferRef` allocation failing after the larger `AVBuffer` succeeded, which
no `av_max_alloc` limit can select. Rust: every library line of `buffer.rs`
runs, including the new `realloc_by_copy` and `UninitBuffer::from_owner`.
