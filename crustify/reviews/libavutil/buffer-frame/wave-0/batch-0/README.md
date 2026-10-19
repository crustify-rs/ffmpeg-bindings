# Review: libavutil/buffer-frame wave 0, batch 0 (types)

- Items: `AVActiveFormatDescription`, `AVFrameSideDataType`,
  `AVSideDataDescriptor`, `AVSideDataProps` (`frame.rs`); `AVBuffer`,
  `AVBufferPool` (`buffer.rs`); `AVRefStructOpaque`, `AVRefStructPool`
  (`refstruct.rs`)
- Reviewed: wave tip `adfecd6c81` (type batches `7526d99dbd`, `1edb133adb`,
  `36bcce3d6a`, `b32232d00c`, `f27390c8ed`). The symbol share of the wave
  (`av_buffer_default_free`, the `av_refstruct_*` references) belongs to
  review batch 1. Its fix `edd9025228` (landed before this review) also
  gates `RefStructPoolUnref::c_up_ref`, this workset's pool clone, on
  `COUNT_IS_WIDE`. I checked it: on narrow targets it refuses the up-ref at
  run time (`try_clone` -> `None`); on this 64-bit build nothing changes.
- Reports: three.
  - `unsafe/buffer-pool-allocator-free-callback-under-pool-lock`:
    `with_allocator`'s contract did not bar the allocator's buffers' free
    callbacks from re-locking the pool. Reproducer included (a deadlock).
  - `wavefront/buffer-pool-entry-free-callback-lock-unrecorded`: the same fact
    was missing from the `AVBufferPool.alloc`/`alloc2` findings, now
    resubmitted.
  - `conventions/equiv-frame-safety-comment-detached-from-unsafe`.

## Verdicts by step

| step | finding |
|---|---|
| wavefront | Every item has a submitted record (`pending: []`). The enums carry field-less comments that match the headers. `AVBuffer.refcount` and `AVBufferPool.refcount` are marked `refcount`. `AVRefStructPool.refcount` is correctly left unmarked: it counts outstanding entries, and the up-ref (`av_refstruct_ref`) bumps the RefStruct header in front of the pool. One defect: `AVBufferPool.alloc`/`alloc2` omitted that the result's free callback runs under the pool mutex (resubmitted; see the report). `AVRefStructOpaque` shows `kind: struct`: the known union defect (core wave 0 batch 7), not filed again. |
| lifecycle | Complete for wave 0. `AVBufferPool`: `av_buffer_pool_init` (`new`), `av_buffer_pool_init2` (`with_allocator`, which also covers init's custom `alloc`), `av_buffer_pool_uninit` (`BufferPoolUninit`); C has no up-ref, so the box is a sole owner. `AVRefStructPool`: `av_refstruct_pool_alloc` (`new`), `av_refstruct_ref` (`Clone`), `av_refstruct_pool_uninit` (`Drop`); `av_refstruct_pool_alloc_ext(_c)` are wave-1 symbols. `AVBuffer` has no owner of its own (each `AVBufferRef`, wave 1, holds a count). The four enums, `AVRefStructOpaque` and `AVSideDataDescriptor` are values that no C routine allocates, frees or copies. `av_refstruct_replace` on pools (as libavcodec shares them) is `Option::<RefStructPoolArc>::clone_from` in Rust, so it is not a gap. |
| unsafe boundary | `scan-unsafe` flags two kinds of site in the workset. (1) `unsafe_fns_pub_smell` `buffer.rs` `with_allocator`: legitimately unsafe until `AVBufferRef` (wave 1) has a safe owner, since its allocator returns a raw `*mut AVBufferRef`. Its `# Safety` was incomplete (report filed, fixed). (2) `raw_ptr_pub_smell`/`void_ptr_pub_smell` x6 `refstruct.rs`: `RefStructOpaque`'s constructors and accessors. It is the user's cookie, which FFmpeg copies and never dereferences or frees, so a raw pointer is the honest type (as `ChannelCustom::opaque`). Safe callback constructors (wave 1) can keep it internal. `SideDataDescriptor::from_raw`'s `# Safety` (`name` NULL or a never-freed, never-written string) is exactly what `name()`'s `&'static CStr` needs. |
| ub_safe | No defect. The `define_ctype!` handle seams are `pub(crate)`. `BufferPoolBox`/`RefStructPoolArc` have no safe allocator (no `CNew`/`CAlloc`/`CAllocZeroed`, and `RefStructPool` is not `CZeroable`), so `RefStructArc<RefStructPool>` cannot be built safely. `RefStructPoolArc::get_mut`/`try_into_box` never succeed. `SideDataDescriptor` is not `CPlainElem`, so no generic path reads arbitrary bytes into its `name`. Each `Send`/`Sync` proof was checked against `buffer.c`/`refstruct.c` (mutex-guarded free lists, atomic counts, fields written only before publication). |
| ub_unsafe | `with_allocator`: the incomplete contract above. Within the fixed contract, the existing seven suites and the new one hold. `SideDataDescriptor::from_raw`: no defect. |
| equiv | No divergence. The constants match the C enumerators (38 side data types, 7 AFD codes, 5 props bits, 4 pool flags). The pool constructors and drops call the C routines one to one. |
| internal | No defect. Unit tests cover the policies' raw seams, NULL adoption, the trampolines and the allocation failures. |
| conventions | One report. The anchors are complete: one `Wraps:` per item, `Field:` on both accessors of each anchored field, and `Handwritten` where ffibox has no primitive. The duplicate `Wraps: av_refstruct_ref` / `av_refstruct_unref` anchors are intentional (the pool policy and the payload policies). |
| misc | Nothing. |

## Coverage

C, through the `coverage` build (`GCOV_PREFIX` redirected, `gcov -b` over the
workset's tests): `buffer.c`'s pool routines (`av_buffer_pool_get`,
`pool_alloc_buffer`, `pool_release_buffer`, `av_buffer_pool_uninit`,
`buffer_pool_free`, `buffer_pool_flush`) and `refstruct.c`'s pool lifecycle
(`av_refstruct_pool_alloc`, `refstruct_pool_uninit`, `pool_unref`,
`pool_free`) ran every line. Two gaps remain:

- `av_buffer_pool_get`'s recycled path when `buffer_create` cannot allocate
  the new `AVBufferRef` (`buffer.c:401`, false branch). This is now taken by
  `buffer.rs::unit_tests::recycled_entry_stays_free_when_its_reference_cannot_be_allocated`
  (an isolated child under `av_max_alloc(16)`). It checks that C fails the
  request without calling the allocator, keeps the entry listed, and reuses
  the same data once the limit lifts.
- `ff_mutex_init` failures (`buffer.c:268-269`, `290-291`,
  `refstruct.c:382-383`) depend on the environment. They are unreachable here.

Rust (`-C instrument-coverage`, the lib, `equiv`, `ub_safe` and `ub_unsafe`
binaries filtered to the three modules): `refstruct.rs` 100% lines and
regions; `frame.rs` 100% lines (5 regions are `fmt` `?` error arms); `buffer.rs`
96.5% lines. Its misses are test allocator closures that the tests never call
on purpose.

## Validation

- `cargo check --workspace`, `cargo clippy --workspace --all-targets -- -D
  warnings`, `RUSTDOCFLAGS=-D warnings cargo doc -p ffmpeg --no-deps`: clean.
- `cargo test --workspace`: all pass (lib 810, `ub_unsafe` 120).
- ASan/UBSan and TSan crate runs (`--lib --tests`, `compile_fail` skipped), on
  the wave tip before the fixes and again after: all pass, with no
  `runtime error:` lines and no sanitizer reports.
- BSan: not a sub-campaign's last wave (buffer-frame has waves 0-4), so there
  is no gate. The only reproducer is a deadlock, which no sanitizer reports
  (TSan checked), so BSan was not run.
- Miri: the new tests all call C, so there is nothing new to run.
- `scan-unsafe` before/after: the only counter change is
  `undocumented_unsafe` for `tests/equiv/frame.rs`, 1 -> 0.
