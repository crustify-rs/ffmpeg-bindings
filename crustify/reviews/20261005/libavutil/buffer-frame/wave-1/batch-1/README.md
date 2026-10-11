# Review: libavutil/buffer-frame wave 1, batch 1 (the wave's symbols)

- Items (10), all from translation batch 1 (`0ea2c20f1f`):
  - `av_buffer_pool_uninit`, homed in `crustify/rust/ffmpeg/libavutil/buffer.rs`;
  - `av_frame_side_data_desc` and `av_frame_side_data_name`, homed in
    `crustify/rust/ffmpeg/libavutil/frame.rs`;
  - `av_refstruct_alloc_ext`, `av_refstruct_alloc_ext_c`,
    `av_refstruct_pool_alloc`, `av_refstruct_pool_alloc_ext`,
    `av_refstruct_pool_alloc_ext_c`, `av_refstruct_pool_get` and
    `av_refstruct_pool_uninit`, homed in
    `crustify/rust/ffmpeg/libavutil/refstruct.rs`.

  `av_buffer_pool_uninit` (`BufferPoolUninit`), `av_refstruct_pool_alloc`
  (`RefStructPool::new`) and `av_refstruct_pool_uninit`
  (`RefStructPoolUnref`) were emitted by wave-0 type batches. Translation
  batch 1 did not re-emit them. They were reviewed here against wave 1's
  changes (`BufferPool::with_allocator` now returns `UninitBuffer`s, wave-1
  type batch 0; `TypedRefStructPool` is built on the other two).
- Reviewed commit: the wave tip `1a9271f931`.
- Wave 1 is not the sub-campaign's last wave (`schedule.json` lists waves
  0-4 for `buffer-frame`). This review emits no reproducer, so BSan did not
  run.
- Reports:
  - `conventions/refstruct-unref-safety-comment-beyond-scan-lookback` (fixed)
  - `misc/typed-pool-validity-argument-misstates-zero-every-time` (fixed)
  - `wavefront/refstruct-pool-get-return-omits-zero-every-time` (resubmitted)

## Verdicts by step

| step | finding |
|---|---|
| lifecycle | Not applicable: the worklist has symbols only. The strategies these symbols implement were read against `refstruct.c` and `buffer.c`. `alloc_with_release` pairs `av_refstruct_alloc_ext_c` with a one-shot release trampoline. `RefStructUnref`'s `CAlloc` is the `NO_ZEROING` allocation with no callback, which `av_refstruct_unref` frees unread. The typed pool covers C's five callbacks (`opaque` as the owned `PoolState`; init, reset and free_entry trampolines; `free_cb` dropping the state). `BufferPoolUninit` releases the caller's pool reference. Nothing is missing. |
| wavefront | All 10 records are submitted. One note was wrong: `av_refstruct_pool_get`'s return said reused entries hold what the last user left and fresh ones are zeroed unless `NO_ZEROING`, neither of which holds under `ZERO_EVERY_TIME`. Resubmitted (`wavefront/refstruct-pool-get-return-omits-zero-every-time`). The other records match the C, including the callbacks' threads, mutex state and failure paths. |
| unsafe boundary | The workset adds no public `unsafe fn` and no raw pointer in a public signature. `alloc_with_release`, `PoolCallbacks`, `TypedRefStructPool` and `SideDataType::{descriptor,name}` are all safe. `scan-unsafe --sites` shows no smell site on the workset's lines. In the homes, `refstruct.rs`'s six `raw_ptr_pub_smell` sites are `RefStructOpaque`'s cookie accessors (wave-0 type). `buffer.rs`'s `unsafe_fns_pub_smell` sites are `with_allocator` and `UninitBuffer::assume_init`, wave-1 type batch 0, the type reviewer's. The `CDrop`/`CAlloc`/`CRefClone` impls are seams. The `SAFETY` arguments of the new impls and trampolines hold, except one whose cited premise was wrong (see misc). |
| ub_safe / ub_unsafe | No UB found. Points checked against the C: `free_with` drops `PoolState` only from `pool_free`, when `pool->refcount` reaches zero. Every callback runs while its entry or the pool's own count is still held. That covers reset (the entry's `free_cb`, before `pool_return_entry`), init (inside a `get` whose caller holds a reference) and free_entry (before the decrement in `pool_return_entry` and before `pool_unref` in the uninit flush). So `&S` borrows never outlive the state, even when a callback tears its own pool down. This is now tested (`pool_teardown_in_callbacks`, clean under plain, ASan and TSan). Callbacks are higher-ranked over `T::Mut` and `S: 'static`, so neither an entry handle nor the state borrow escapes. Entries are valid `T` on every path, given the `init_with` zeroing under `ZERO_EVERY_TIME`. `Send`/`Sync` follow `RefStructArc<T>`, and the callbacks and state are `Send + Sync`. `S` can hold its own pool's entries only when `T: Send`. No safe path reaches `RefStructPoolMut` or adopts a pool as a `RefStructArc`. The workset adds no public unsafe API, so it needs no `ub_unsafe` suite. |
| equiv | No divergence. The random-sequence machine compares typed pools with C pools for all 16 flag sets and every callback subset, logging every callback and payload. Release timing is compared with a C `free_cb`, and the side-data lookups with C over 0..=64 and the `u32` edges. The two deliberate differences (`NO_ZEROING` ignored, entries zeroed before `init` under `ZERO_EVERY_TIME`) are documented and masked as unspecified bytes. |
| internal | No defect. Miri runs the trampoline unit tests cleanly. |
| conventions | One deviation: `RefStructUnref`'s `CDrop` `// SAFETY:` block was 10 lines, beyond `scan-unsafe`'s 6-line lookback, and was counted as undocumented. Fixed by splitting it (`conventions/refstruct-unref-safety-comment-beyond-scan-lookback`). Anchors: one `Wraps:` per item. `av_refstruct_alloc_ext_c` is duplicated on purpose (`CAlloc` and `alloc_with_release`), and `Handwritten` is on the hand-written policies and `TypedRefStructPool`. `CBox` is foreign, so `alloc_with_release` is a free function. |
| misc | `TypedRefStructPool`'s validity argument, which `get`'s adoption cites, claimed C zeroes every entry at allocation. Under `ZERO_EVERY_TIME` it does not. Documentation fixed (`misc/typed-pool-validity-argument-misstates-zero-every-time`). |

## Coverage

Details in `coverage.json`. C: every line of the workset's functions and the
static helpers they run is executed, except `av_refstruct_pool_alloc_ext_c`'s
`ff_mutex_init` failure branch (`refstruct.c:382-383`), which glibc never
takes. Rust: `refstruct.rs` 98.55% of lines, `frame.rs` 100%. The missed
`refstruct.rs` lines are the narrow-count refusal, `release_with`'s
impossible `into_mut` failure and the trampolines' NULL-opaque returns, none
of them reachable. The `buffer.rs` misses belong to wave-1 type batch 0. The
new `ub_safe` teardown tests add no lines. They do run C paths no earlier test
took: the pool's uninit nested inside a reset, and returns into an uninited
pool from inside a free_entry callback.

## Validation at the landed tree

Run on the review's tree rebased onto the wave tip `6c1903319b` (the
wave-1 type review's fixes):

- `cargo check --workspace` and `cargo clippy --workspace --all-targets -- -D warnings`
  are clean. `RUSTDOCFLAGS=-D warnings cargo doc -p ffmpeg --no-deps` is
  clean. `rustfmt --edition 2024` is clean on both touched files, which were
  clean at the base.
- `cargo test --workspace` (plain, with trybuild `compile_fail` and the
  doctests): all pass (lib 829, equiv 651, ub_safe 725, ub_unsafe 124).
- ASan/UBSan and TSan over the whole `ffmpeg` crate (`-Zbuild-std`,
  `--lib --tests --skip compile_fail`): every binary passes, with no
  `runtime error:` and no sanitizer report.
- Miri: the FFI-free `refstruct` unit tests (the release and pool
  trampolines, the flag filter and the opaque union) pass.
- BSan: not run. Wave 1 is not the sub-campaign's last wave, and this
  review emits no reproducer.
- `scan-unsafe`: `undocumented_unsafe` goes from 23 to 22 (`refstruct.rs`:
  1 to 0). Every site counter is unchanged.
