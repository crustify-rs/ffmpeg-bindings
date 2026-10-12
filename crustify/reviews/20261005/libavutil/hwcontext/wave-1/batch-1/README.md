# Review: libavutil/hwcontext wave 1, batch 1 (the wave's symbols)

- Items (16): `av_hwdevice_ctx_alloc`, `_ctx_create`, `_ctx_create_derived`,
  `_ctx_create_derived_opts`, `_ctx_init`, `_find_type_by_name`,
  `_get_type_name`, `_hwconfig_alloc`, `_iterate_types`;
  `av_hwframe_ctx_alloc`, `_ctx_create_derived`, `_ctx_init`,
  `_get_buffer`, `_map` (translation batch 1, `62b80e7a1d`); and
  `av_hwframe_transfer_data` and `_transfer_get_formats` (translation
  batch 2, `bf7c454c03`). All are homed in
  `crustify/rust/ffmpeg/libavutil/hwcontext.rs`.
- Reviewed commit: the wave tip `b9dbecd340`.
- Wave 1 is not the sub-campaign's last wave: `schedule.json` lists waves
  0-2 for `hwcontext`. This review emits no reproducer, so BSan did not run.
- Reports:
  - `unsafe/hardware-frames-transfer-only-through-unsafe-seams`: fixed.
    Added the safe `FramesReferenceRef::transfer_formats` and
    `HardwareFrame::{download_to, upload_from, copy_from}`, and removed the
    obsolete `unsafe fn hwcontext::transfer_formats`.
  - `unsafe/uninit-frames-raw-seam-contract-incomplete`: contract fixed.
  - `misc/map-to-unmapping-claims-the-target-context`: documentation
    fixed, with a regression test.

## Verdicts by step

| step | finding |
|---|---|
| lifecycle | Not applicable: a symbol workset. The batch's owners were checked against C's release paths. `ContextUnref` (`av_buffer_unref`/`av_buffer_ref`, with count checks) covers device and frames references. `UninitDevice`/`UninitFrames` release contexts that failed or never ran init, as C does. `HwConfig` uses `av_free`. `HardwareFrame`/`MappedFrame` free frames with `av_frame_free`. |
| wavefront | No analysis defect. All 16 records match the code: ownership, nullability, the out slots' NULL-on-failure (and `av_hwframe_ctx_create_derived` leaving its slot untouched on non-unmapping failures), the unmapping's `buf[0]` reinterpretation, and the transfers' unchecked backend calls. The notes of `av_hwframe_transfer_get_formats` and `av_hwframe_transfer_data` named the Rust API this review changed. They were resubmitted with the new names; the C findings are unchanged. |
| unsafe boundary | One defect, fixed (see the report). Batches 1 and 2 ran in parallel. Batch 2 left `transfer_formats(BufferReferenceRef)` and `FrameMut::transfer_data_from` `unsafe` "until frames contexts and their frames are wrapped". Batch 1 wrapped them, but `HardwareFrame` had no transfer at all and `FramesReferenceRef` no listing. The other public `unsafe` sites in the workset are legitimate. `UninitDevice::init`: backends dereference caller-filled `hwctx` in `device_init`. `UninitDevice::from_c` is an adoption seam. `transfer_data_from` remains for hardware frames C hands over as `Frame`s. The raw-pointer sites are `UninitDevice::from_c`/`into_c` (seams), `HwConfig::as_ptr`/`as_mut_ptr` (backend structs are unwrapped) and `UninitFrames::as_mut_ptr` (until wave 2 wraps `AVHWFramesContext`). The last one's contract was incomplete (second report). |
| ub_safe / ub_unsafe | No UB found. The C paths were checked for each of the following: NULL backend transfers (refused through the listing first); the unmapping's `HWMapDescriptor` cast (`map_to` refuses the own context, and every frame of a derived context is a mapping C made); `av_hwframe_map`'s failure `av_assert0`; derived-context allocation and mapping; reference counts (checked before C adds any, results past the limit released); `UninitFrames::init` from safe code alone (format `NONE` fails before any backend code); and the hook trampolines. `HardwareFrame` and the references stay `!Send`/`!Sync`, so no thread path needs TSan evidence beyond the existing suites. |
| equiv | No divergence beyond the documented refusals. Batch 1's `contexts` suite compares every symbol with C over the test backends. This review added `hardware_frame_transfers` for the new safe forms: listings, multi-step upload/download/copy sequences, backend failures, and derived copies. It compares results, images, shapes and event logs after each step. |
| internal | No defect. The private helpers (`count_allows_another`/`count_within_limit`, `within_limit_*`, `may_reference_all`, `HwFrames`) are covered. Added a unit test for `FramesReferenceRef::try_clone` at the count limit, which was uncovered. |
| conventions | No deviation. Each item has one `Wraps:` anchor, with intentional duplicates: `av_hwframe_map` (map, map_to), `av_hwframe_transfer_get_formats` (`FrameRef`, `FramesReferenceRef`), `av_hwframe_transfer_data` (`FrameMut` plus three `HardwareFrame` forms). Methods sit on the handles, owners are boxes, and the hand-written owners carry `Handwritten`. |
| misc | One documentation defect, fixed: `map_to`'s unmapping returns the frame the source was mapped from, in that frame's context, which can be a third context, not `frames`. |
| coverage | See below. |

## Validation

These runs used the landed tree, rebased onto the wave tip `79a4c7744c`
(the type review, which changed only hook docs, one unit test and
wavefront records).

- `cargo check --workspace`, `cargo clippy --workspace --all-targets -- -D
  warnings`, `cargo test --workspace` (plain: lib 953, equiv 711, ub_safe
  808, ub_unsafe 172, doctests) and `RUSTDOCFLAGS=-D warnings cargo doc -p
  ffmpeg --no-deps`: all clean.
- The whole `ffmpeg` crate under ASan/UBSan and under TSan: lib 953, equiv
  711, ub_safe 806 (+2 trybuild, skipped under `-Zbuild-std`), ub_unsafe
  172. No sanitizer report and no `runtime error:`.
- Miri on the 8 FFI-free `hwcontext::unit_tests`: all pass. Every test
  this review added calls C.
- BSan: not run (no reproducer; not the last wave).
- `crustify scan-unsafe`: `unsafe_fns_pub_smell` 24 -> 23 and `unsafe_fns`
  286 -> 285. No new smell, undocumented `unsafe` or raw-pointer site.

## Coverage

Details in `coverage.json`. Rust, `libavutil/hwcontext.rs`, wave tip ->
fixed: regions 99.03% -> 99.16%, functions 97.95% -> 98.06%, lines
99.37% -> 99.46%. C, `libavutil/hwcontext.c`: 78.04% of lines both
times. The transfers' C paths were already run by batch 2's fake contexts.
Every C line still missed falls into one of these groups:

- it needs a compiled-in backend (`av_hwdevice_ctx_create` and the
  new-device derivation; `hw_table` is empty);
- it is an allocation failure smaller than an earlier allocation that
  succeeds (OOM);
- it is an internal helper only real backends call;
- it is unreachable through the wrappers (the own-context unmapping, a
  frame without `buf[0]`).
