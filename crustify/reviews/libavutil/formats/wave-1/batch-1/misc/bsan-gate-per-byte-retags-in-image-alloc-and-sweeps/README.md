# BSan gate: `equiv` and `ub_safe` time out on per-byte retags in `Image::alloc` and the image and sample sweeps

- Files: `crustify/rust/ffmpeg/libavutil/imgutils.rs` (`Image::alloc`,
  `Wraps: av_image_alloc`); `crustify/rust/ffmpeg/tests/equiv/imgutils.rs`,
  `crustify/rust/ffmpeg/tests/ub_safe/imgutils.rs`,
  `crustify/rust/ffmpeg/tests/ub_safe/samplefmt.rs`
- Affected commit: `2f6e60458d` (formats wave-1 head; the gate log is
  `crustify/artifacts/libavutil/formats/review-wave-1/bsan-gate.log`)
- Items: `av_image_alloc`, and the tests of `av_image_copy`, `av_image_copy2`,
  `av_image_copy_uc_from`, `av_image_copy_to_buffer`, `av_image_fill_color`,
  `av_image_fill_black`, `av_samples_alloc`,
  `av_samples_alloc_array_and_samples`, `av_samples_fill_arrays`,
  `av_samples_copy`, `av_samples_set_silence`
- Instrument: BSan only. ASan/UBSan, TSan and the plain build run the same
  tests in seconds; BSan printed no "Undefined Behavior" report in the gate
  or in any run here.
- Verdict: no undefined behaviour. A cost problem: one `&mut u8` (a retag)
  per byte written or read, on allocations whose borrow history then grows
  with every retag, in one library routine and three test helpers. Fixed by
  writing and comparing whole slices, and by thinning the every-format
  image sweeps under BSan.

## Finding

The gate's `equiv` and `ub_safe` binaries hit the 7200 s timeout; the
orchestrator's triage of each test alone (1500 s cap) found four that never
finish and one that takes 867 s:

| binary | test | gate triage |
|---|---|---|
| `ub_safe` | `samplefmt::buffers::many_planar_channels_spill_the_plane_pointers` | timeout, 62 GB |
| `equiv` | `imgutils::images::copies_and_fills_match_c_for_every_format` | timeout, 14 GB |
| `equiv` | `imgutils::images::allocations_match_c` | timeout, 4.4 GB |
| `ub_safe` | `samplefmt::buffers::every_owner_and_view_stays_inside_its_planes` | timeout, 3.8 GB |
| `ub_safe` | `imgutils::images::exact_planes_of_every_format_copy_and_fill` | 867 s |

Where the time goes, found by changing one thing at a time and timing each
test alone under BSan (`<binary> --exact <test> --test-threads=1`,
`BSAN_OPTIONS=gc_hang_threshold_ms=600000`):

1. **`Image::alloc` zero-fills the block byte by byte.** It adopted the
   `av_malloc` block as `HeapVec<MaybeUninit<u8>>` and ran
   `buf.as_mut_slice().fill(MaybeUninit::new(0))`. `[T]::fill` for a `Copy`
   `T` is a loop that forms a `&mut T` per element, so every image
   allocation costs one retag per byte of the block, and each retag's cost
   grows with the block's history. The every-format copy-and-fill sequence
   allocates two images per `(format, size, align)` and ran away on this
   alone: with the test's own per-byte writes already removed and the sweep
   thinned to 18 formats (below), it still ran past 750 s at 20 GB, and
   finished in 71 s once the fill became one `ptr::write_bytes`.
   The library change is the same bytes written, in one write
   (`imgutils.rs`, `Image::alloc`).
2. **The test helpers wrote and compared byte by byte.**
   `equiv/imgutils.rs::noise` filled each C and Rust plane through
   `iter_mut().zip(..)`, one `&mut u8` per byte; it now builds the bytes in
   a `Vec` and writes each plane with one `copy_from_slice`.
   `ub_safe/samplefmt.rs::exercise` checked silence with
   `p.iter().all(|&b| ..)`, one `&u8` per byte of every plane of every
   buffer it exercised; it now compares each plane with one slice `==`.
   That alone took `every_owner_and_view_stays_inside_its_planes` from a
   >1500 s timeout (3.8 GB) to 1.25 s on a thinned sweep, and the full
   sweep (12 formats x 4 channel counts x 4 sample counts x 4 alignments,
   768 iterations, six owners and views each) then runs in the seconds
   recorded below, so that test keeps its full parameters under BSan. The
   4096-channel test is the one case the fix does not cover: its ~20,000
   per-plane slices over one 256 KiB block make each retag cost the
   block's whole history, and it still ran past 880 s at 54 GB; under BSan
   it uses 24 channels (still past the 8 pointers the inline array holds,
   so C's pointer array is still spilled to the heap).
3. **Every-format image sweeps at every size.** With the per-byte loops
   gone, the sweeps over all 242 pixel formats are bounded but still
   minutes to tens of minutes each, which the gate's whole-binary budget
   does not have. Under `CRUSTIFY_FFMPEG_VARIANT=bsan` the three image
   sweeps (`allocations_match_c`,
   `copies_and_fills_match_c_for_every_format`,
   `exact_planes_of_every_format_copy_and_fill`) keep one format of each
   layout class the image routines distinguish (packed and planar YUV,
   chroma subsampled both ways, semi-planar, RGB in both byte orders, a
   palette, a bitstream, float, 16-bit and alpha planes, Bayer, XYZ and a
   hardware format: `BSAN_FORMATS` / `sweep_formats()`), the out-of-range
   values, and two sizes and two alignments. Every other instrument still
   runs the full sweeps.

## Timings, each test alone under BSan, after the fix

| binary | test | before (gate triage) | after |
|---|---|---|---|
| `ub_safe` | `samplefmt::buffers::many_planar_channels_spill_the_plane_pointers` (24 channels under BSan) | timeout, 62 GB | 0.1 s (4096 channels: >880 s, 54 GB, killed) |
| `ub_safe` | `samplefmt::buffers::every_owner_and_view_stays_inside_its_planes` (full sweep) | timeout, 3.8 GB | 19 s |
| `equiv` | `imgutils::images::allocations_match_c` (thinned) | timeout, 4.4 GB | 68 s |
| `equiv` | `imgutils::images::copies_and_fills_match_c_for_every_format` (thinned) | timeout, 14 GB | 71 s |
| `ub_safe` | `imgutils::images::exact_planes_of_every_format_copy_and_fill` (thinned) | 867 s | 62 s |
| `equiv` | `imgutils::images::random_plane_layouts_match_c` (new, 3 rounds a format under BSan) | - | 22 s |

Whole-binary BSan runs of the rebased tree are in the batch README.

## Why it is not a wrapper defect

Nothing here changes what the wrappers or the tests check: the image block
is still zeroed, the planes still hold the same noise, the silence check
still compares every byte. The gate's own log and every run here show no
BSan report; the ASan/UBSan and TSan passes over the full sweeps are clean.
The lesson for the shared test recipes: a `fill`, `iter_mut` or `iter`
over the bytes of a C-owned block is a retag per byte under BSan, in
library code as much as in tests; write and compare such blocks with one
slice operation.
