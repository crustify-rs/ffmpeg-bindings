# Review: libavutil/buffer-frame wave 3, batch 0 (the wave's types)

- Item: `AVFrame` (`libavutil/frame.h`, 40 field anchors), homed in
  `crustify/rust/ffmpeg/libavutil/frame.rs`: `Frame`/`FrameRef`/`FrameMut`,
  `FrameBox = CBox<Frame, FrameFree>` (`av_frame_alloc`/`av_frame_free`),
  `FrameMut::reset` (`av_frame_unref`), `FrameRef::try_clone`
  (`av_frame_clone`), `FrameFlags`, `DecodeErrorFlags` and `PlaneError`.
  Wrapped by translation batch 0 (`2a232726c8`).
- Reviewed commit: the wave tip `2a232726c8`. The symbol review (batch 1)
  landed while this one ran, and this review is rebased onto it.
- Wave 3 is not the sub-campaign's last wave: `schedule.json` lists waves
  0-4 for `buffer-frame`. BSan therefore ran only on this review's two
  reproducers, each alone.
- Reports:
  - `ub_unsafe/clone-reads-planar-channels-past-data`: reproducer, fixed.
    ASan and BSan SEGV.
  - `ub_unsafe/extended-data-keeps-a-replaced-external-plane`: reproducer,
    fixed. ASan heap-use-after-free; BSan use of a freed allocation.
  - `unsafe/hw-frames-context-contract-allows-written-bytes`: `# Safety`
    fixed.
  - `wavefront/extended-data-length-own-array-unrecorded`: resubmitted.

## Verdicts by step

| step | finding |
|---|---|
| lifecycle | Complete for this wave. `query symbols --lifetime-for AVFrame` lists `av_frame_alloc` (constructor), `av_frame_free` (dropper), `av_frame_unref` (field disposer) and `av_frame_clone` (cloner). All four are implemented and anchored: `CNew`, `CDrop` on a local slot, `reset`, and `try_clone` (no `Clone`, as C fails for frames without planes). `av_frame_ref`, `_move_ref`, `_replace` and `_get_buffer` are wave-4 symbols; their records and wrappers belong to that run. The invariant's owners (`buf`, `extended_buf`, side data, metadata, `opaque_ref`, `hw_frames_ctx`, `private_ref`, `ch_layout`, a separate `extended_data`) each have a release path that was checked against `av_frame_unref`. |
| wavefront | One analysis defect, resubmitted. `extended_data`'s note had the length rule (at least `nb_channels` entries) for the separate block only. It missed that `frame_copy_audio` and `av_frame_get_plane_buffer` read one entry per planar channel even when `extended_data` is the frame's 8-entry `data`. The other 39 field records (ownership, nullability, cardinality, borrow sources) and the four lifecycle records were checked against `frame.c` and are right. |
| unsafe boundary | `scan-unsafe` finds three public `unsafe fn` in the workset: `set_data_ptr`, `set_extended_data_ptr` and `replace_hw_frames_context`. All three are legitimate. The first two lend caller memory to a frame whose data is not reference counted; no lifetime can express that, because the frame is a `'static` owner. The third waits on hwcontext's `AVHWFramesContext` wrapper. The public raw pointers are `opaque()`/`set_opaque()`, a cookie nothing dereferences, as `BufferReferenceRef::opaque` already is. The private raw-pointer helpers are layout projections. The first two contracts turned out to be broken by safe code: see the two `ub_unsafe` reports. `replace_hw_frames_context`'s contract allowed a context whose bytes C still writes, while the safe getter lends them as `&[u8]`: fixed (documentation). |
| ub_safe / ub_unsafe | No safe-only defect. Checked: planes are only ever read through their buffer's window and written only when the buffer is sole and writable; buffer changes prune before the old reference is released (no ABA); `channel_layout_mut` and the channel-count guard keep a separate `extended_data` long enough; `try_clone` on counted frames, under count limits, and across threads; drop order of planes, buffers and handles (trybuild). Two true defects through the unsafe API, called within its stated contracts. **(1)** Planar audio with more than 8 lent channels and `extended_data` still `data`: `try_clone` let `av_frame_copy` take `linesize[]` for channel 8's plane pointer (SEGV at `0x40`). Fixed: `try_clone` refuses such a frame. **(2)** `alloc_extended_data` copied lent planes into the new array, so replacing a plane and freeing its memory, as `set_data_ptr` allows, left a dangling entry that the clone's copy read (heap-use-after-free). Fixed: only a counted frame's planes are copied. Regressions are in `tests/ub_unsafe/frame.rs::frame_type`: `clones_refuse_channels_past_data` and `extended_arrays_keep_no_lent_planes`. |
| equiv | No divergence found. Both fixes act only where C has no defined behaviour, or on a Rust-only helper; the random sequence's C mirror of `alloc_extended_data` follows the new rule (its frames lend no planes, so outcomes are unchanged). New: `tests/equiv/frame.rs::frame_type::clones_copy_external_channels_alike` compares the clone of lent planar audio (1, 2, 8, 9 and 13 channels; `data` or a separate array) with `av_frame_clone`: shapes, every channel's bytes, and zeroed padding on the Rust side. |
| internal | No further defect. New unit tests cover both new branches: `frame_copies_reading_past_extended_data` and `frame_extended_arrays_copy_only_counted_planes`. Also new: `frame_clone_failures_one_allocation_at_a_time`. `frame_allocation_failures_leave_frames_whole`'s doc claimed it failed `av_frame_ref`'s extra block and `extended_data` copy, but its 70 side data entries always failed first (gcov). The new isolated test fails each of those, the layout copy, and `get_audio_buffer`'s array and plane buffer on their own, under LSan. The old doc now says what it covers. |
| conventions | No deviation. Every one of the 40 field anchors has a `Field: AVFrame.<field>` accessor. `Wraps:` anchors are on `Frame`, `FrameFree` (`av_frame_free`), `Frame::new`, `try_clone` and `reset`. Getters are on `FrameRef`, setters on `FrameMut`. All projections use `addr_of!`/`addr_of_mut!`, and every `unsafe` block carries a `// SAFETY:` comment (`scan-unsafe` finds no undocumented one in `frame.rs`). |
| misc | None. |

## Validation

- `cargo check --workspace`, `cargo clippy --workspace --all-targets -- -D
  warnings`, `cargo test --workspace` (plain), and `cargo doc -p ffmpeg`
  with `-D warnings`.
- The whole `ffmpeg` crate under ASan/UBSan (no report, no `runtime
  error:`) and under TSan (no report): lib 864, equiv 665, ub_safe 774 (+2
  trybuild, skipped under `-Zbuild-std`), ub_unsafe 141. Two frame tests
  were added after those runs: `frame_clone_failures_one_allocation_at_a_time`,
  and the doc rewording. The frame tests of lib, equiv and ub_unsafe were
  rerun under ASan after them (LSan included). The final numbers on the
  rebased tree are in the commit message.
- Miri on the FFI-free frame unit tests: 35 run, 22 ignored as FFI.
- BSan, each reproducer alone: both report on the affected commit and run
  clean on the fix.
- `scan-unsafe`: no counter changes apart from one new private fn
  (`copy_reads_past_extended_data`) and its lines.

## Coverage

Details in `coverage.json`. Rust: `frame.rs` regions 99.46%, functions
100%, lines 99.78% (2693 lines). The six missed lines are allocation
failures in `copy_into_zeroed_buffers` that can only follow
`av_frame_clone`'s identical, successful allocations, and
`push_extended_buffer` at an `INT_MAX` count. C: `av_frame_alloc`, `_free`,
`_unref`, `_clone`, `_copy` and `_get_buffer` run 100% of their lines.
`av_frame_ref` rose from 81.67% to 90.00% and `get_audio_buffer` from
81.58% to 94.74%. Every line still missed is hardware-only, unreachable
through `av_frame_ref` (PANSCAN skip, `force_copy`, a layout mismatch after
the copy), or a small allocation failing after a larger one succeeded.
