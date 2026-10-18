# Review: formats wave-1 symbols (image layout, copies and fills; sample buffers; pixel, sample, channel, colour and csp queries)

Route: **symbol** (56 items). Objective: **review**. Affected base:
`2f6e60458d` (the wave's head, holding translation batches 3, 4 and 5, which
landed all 56 items).

Items and homes:

- `crustify/rust/ffmpeg/libavutil/imgutils.rs` (batch 4): `av_image_alloc`
  (`Image::alloc`), `av_image_check_sar` (`check_sar`),
  `av_image_check_size2` (`check_size_max_pixels`, and the unsafe
  `check_size_max_pixels_with_context` over the raw `log_ctx` seam),
  `av_image_copy` (`copy`), `av_image_copy2` (`copy_from_mut`),
  `av_image_copy_uc_from` (`copy_from_uncacheable`),
  `av_image_copy_to_buffer` (`copy_to_buffer`), `av_image_fill_arrays`
  (`fill_arrays`, `fill_arrays_mut`), `av_image_fill_pointers`
  (`fill_pointers`, `fill_pointers_mut`, `buffer_size_for_linesizes`),
  `av_image_fill_linesizes` (`fill_linesizes`), `av_image_fill_plane_sizes`
  (`fill_plane_sizes`), `av_image_get_linesize` (`linesize`),
  `av_image_get_buffer_size` (`buffer_size`), `av_image_fill_color`
  (`fill_color`, `check_fill_color`), `av_image_fill_black` (`fill_black`,
  `check_fill_black`);
- `crustify/rust/ffmpeg/libavutil/samplefmt.rs` (batches 4 and 5):
  `av_sample_fmt_is_planar`, `av_get_bytes_per_sample`,
  `av_get_alt_sample_fmt`, `av_get_packed_sample_fmt`,
  `av_get_planar_sample_fmt`, `av_get_sample_fmt_name`, `av_get_sample_fmt`,
  `av_get_sample_fmt_string` (methods of `SampleFormat`);
  `av_samples_get_buffer_size` (`samples_buffer_size`),
  `av_samples_fill_arrays` (`SamplesRef::from_buffer`,
  `SamplesMut::from_buffer`), `av_samples_alloc` (`SampleBuffer::new`),
  `av_samples_alloc_array_and_samples` (`SampleBufferWithPointers::new`),
  `av_samples_copy` (`SamplesMut::copy_from`, `copy_within`),
  `av_samples_set_silence` (`SamplesMut::set_silence`);
- `crustify/rust/ffmpeg/libavutil/pixdesc.rs` (batches 3, 4 and 5):
  `av_get_pix_fmt_name`, `av_get_pix_fmt`, `av_get_pix_fmt_string`,
  `av_pix_fmt_count_planes`, `av_pix_fmt_get_chroma_sub_sample`,
  `av_pix_fmt_swap_endianness`, `av_get_pix_fmt_loss`,
  `av_find_best_pix_fmt_of_2` (methods of `PixelFormat`, with `Loss`);
  `av_color_range_name`, `av_color_primaries_name`,
  `av_color_transfer_name`, `av_color_space_name`,
  `av_chroma_location_name`, `av_chroma_location_enum_to_pos`,
  `av_chroma_location_pos_to_enum`, `av_alpha_mode_name`,
  `av_alpha_mode_from_name` (methods of the `pixfmt` enum wrappers);
- `crustify/rust/ffmpeg/libavutil/channel_layout.rs` (batch 3):
  `av_channel_name` (`Channel::name`, `write_name`),
  `av_channel_description` (`description`, `write_description`),
  `av_channel_from_string` (`Channel::from_name`);
- `crustify/rust/ffmpeg/libavutil/csp.rs` (batch 3):
  `av_csp_approximate_trc_gamma`, `av_csp_approximate_eotf_gamma`,
  `av_csp_trc_func_from_id`, `av_csp_trc_func_inv_from_id`,
  `av_csp_itu_eotf`, `av_csp_itu_eotf_inv` (methods of `ColorTransfer`);
- `crustify/rust/ffmpeg/libavutil/pixelutils.rs` (batch 5):
  `av_pixelutils_get_sad_fn` (`SadFn::new`).

## Findings and scope

- **BSan gate (timeouts, no UB; fixed).** The wave's gate timed out in the
  `equiv` and `ub_safe` binaries on four tests of this workset that never
  finish under BSan, with no "Undefined Behavior" report. The cost is one
  retag per byte: `Image::alloc` zeroed its `av_malloc` block with
  `[MaybeUninit<u8>]::fill` (a `&mut` per byte), and the test helpers
  wrote (`equiv/imgutils.rs::noise`) and compared
  (`ub_safe/samplefmt.rs::exercise`) planes byte by byte. The library now
  zeroes the block with one `ptr::write_bytes`, the helpers write and
  compare whole slices, and the three every-format image sweeps keep one
  format of each layout class plus two sizes and two alignments under
  `CRUSTIFY_FFMPEG_VARIANT=bsan` only (the 4096-channel sample test uses
  24 channels there, still past the inline pointer array). Timings before
  and after, and the whole-binary runs, are in
  `misc/bsan-gate-per-byte-retags-in-image-alloc-and-sweeps/`.
- **No UB defect** in the safe API. Each wrapper's guards were read against
  the C bodies: the image routines place every row C reads or writes
  inside its slice (`plane_origin`, `CopyPlan::origins`, `fill_args`,
  `copy_to_buffer`'s per-plane check with C's own row widths and
  subsampled row counts, the 1024-byte palette), refuse the rows C
  asserts on and the linesizes it cannot hold in `int`, refuse the widths
  on which `image_get_linesize`'s bitstream round-up overflows and the
  non-positive heights `av_image_fill_plane_sizes` divides by, bound
  `align` to the powers of two `FFALIGN` is defined for, and hand the SSE4
  path of `av_image_copy_uc_from` only aligned, covered rows; `Image` and
  the sample owners adopt exactly the bytes C allocated (`size + align`
  and `av_samples_get_buffer_size`'s size) and free them with `av_free`;
  `Layout::new` proves every plane of a sample view inside its buffer and
  every byte count C computes inside an `int`, and `c_align` sidesteps
  the signed overflow of `FFALIGN(INT_MAX - 31, 32)`; `describe` returns
  `None` where C would index `av_pix_fmt_descriptors` past its end or
  leave the buffer unterminated; the name writers pass NULL with size 0
  for an empty buffer (C's count-only request) and cap the size at
  `UINT_MAX`; the string lookups take `&CStr`; the csp and pixelutils
  handles hold only C's own static functions. ASan/UBSan and TSan pass
  the full sweeps; the gate's BSan run and this review's whole-binary
  BSan runs show no report.
- **`ub_unsafe`**: the one public unsafe function the workset adds,
  `check_size_max_pixels_with_context`, has the same `log_ctx` contract as
  wave 0's `check_size_with_context` (NULL, or a live `const AVClass *`-led
  object `av_log` may read during the call) and additionally rejects a
  `log_offset` that would overflow `av_vlog`'s level; its suite covers
  NULL, class-less and classed contexts. Nothing to narrow: the seam waits
  for a safe logging-context handle, as the crate's other parent-context
  seams do.
- **No equivalence defect.** The landed suites compare every lookup, every
  C-allocated layout (sizes, linesizes, plane ranges, bytes after every
  copy, fill and packing step, in every format and several alignments),
  every sample layout and operation, and the documented divergences
  (`align` not a power of two, non-positive heights, widths C overflows
  on) are the wrappers' refusals of C's undefined or meaningless cases.
  Added: `equiv/imgutils.rs::random_plane_layouts_match_c`, a
  random-sequence probe over caller-laid-out planes (random padding, each
  plane top-down or bottom-up, partial and degenerate sizes) that runs
  `av_image_copy`, `av_image_copy_uc_from`, `av_image_fill_color`,
  `av_image_fill_black`, `av_image_copy_to_buffer` and
  `av_image_fill_arrays` against the wrappers on identical bytes and
  compares every plane after every step: the layouts `av_image_alloc`
  never produces, where row-0 origins and per-plane extents are the
  wrappers' own arithmetic. 12 formats x 24 rounds pass (3 rounds a
  format under BSan). Writing it recorded one thing worth knowing about
  the API, not a defect: for a bottom-up plane the wrappers' row 0 is the
  last row *of the slice*, as the module docs say, so an operation on the
  top `n` rows of a taller bottom-up plane takes the sub-slice ending at
  the image's row 0 (`Laid::top_rows`), where C keeps the same row-0
  pointer and a smaller height.
- **No internal defect**: `Geometry`, `CopyPlan`, `plane_ranges`,
  `split_planes`, `c_align`, `linesize_overflows`, `Layout`,
  `with_plane_ptrs`, `plane_slots` and `c_count` have inline unit tests at
  their edges; Miri passes the FFI-free ones (17 tests).
- **No Wavefront correction needed**: the 56 findings record the
  borrow-for-the-call strings and buffers, the static name and function
  returns, the type-erased nullable `log_ctx`, the in/out `loss_ptr` and
  the out-parameters, as the C bodies show. The plane and pointer arrays
  (`uint8_t *data[4]`, `uint8_t **audio_data`) stay unrecorded, the known
  array-parameter gap (formats wave-1 batch 4 noted it); the ownership
  C hands over through `pointers[0]` / `audio_data[0]` is in the code
  docs and the owners.
- **No lifecycle work**: symbol-only route.
- **No convention deviation**: clippy `--workspace --all-targets -D warnings`,
  rustdoc `-D warnings`, every unsafe block documented, one filled anchor
  per item (two where a safe variant pair exists); `crustify scan-unsafe`
  shows no smell at a site this workset added beyond the documented
  `check_size_max_pixels_with_context` seam (`unsafe_fns_pub_smell`,
  `raw_ptr_pub_smell`, `void_ptr_pub_smell` at `imgutils.rs:536`, the
  pair of wave 0's `imgutils.rs:74`); the private sites it lists
  (`row0_ptrs*`, `copy_args`, `fill_args`, `with_plane_ptrs`,
  `plane_slots`, `static_name`, `SampleBufferWithPointers::pointers` and
  its `as_c_planes` seam) are the raw plane-pointer arrays C takes,
  built and consumed inside one call.
- **Coverage**: `coverage.json`. Rust: 100% of lines in
  `channel_layout.rs`, `csp.rs`, `pixdesc.rs` and `pixelutils.rs`; the six
  uncovered lines of `imgutils.rs` and the one of `samplefmt.rs` are
  unreachable arms (a bitstream plane whose widest component is chroma,
  which no descriptor has; error paths after checks that already passed).
  C: every reachable line of the wrapped routines; the untaken lines are
  dead in this version (`av_image_alloc`'s palette padding memset),
  descriptor cases that do not exist (components deeper than 32 bits,
  float chroma, component-less software formats, NULL table slots), and
  failure arms the wrappers reject first. No coverage test was needed
  beyond the random probe.

## Validation

On the final tree: `cargo check --workspace`,
`cargo clippy --workspace --all-targets -- -D warnings`,
`RUSTDOCFLAGS=-D warnings cargo doc -p ffmpeg`, `cargo test --workspace`
(plain), ASan/UBSan and TSan (`-Zbuild-std`, all four binaries, no report,
no `runtime error:`), BSan (all four binaries whole, under
`gc_hang_threshold_ms=600000`; times below), Miri on the FFI-free unit
tests, the formerly runaway tests alone under BSan (table in the misc
report), and `crustify scan-unsafe`.

| check | result |
|---|---|
| `cargo check --workspace` | ok |
| `cargo clippy --workspace --all-targets -- -D warnings` | ok |
| `RUSTDOCFLAGS=-D warnings cargo doc -p ffmpeg --no-deps` | ok |
| `cargo test --workspace` (plain) | lib 501, equiv 419, ub_safe 416 (compile_fail included), ub_unsafe 72, doctests 54 + 17 |
| ASan/UBSan (`-Zbuild-std`, all four binaries) | lib 501, equiv 419, ub_safe 415, ub_unsafe 72; no report, no `runtime error:` |
| TSan (`-Zbuild-std`, all four binaries) | same counts; no report |
| BSan, whole binaries (`cargo bsan test`, the four run in parallel on one host, `gc_hang_threshold_ms=600000`) | lib 501 in 71 s; ub_unsafe 72 in 171 s; ub_safe 415 in 1807 s; equiv 419 in 2461 s; no report. The `equiv` tail after ~25 min is one types-batch test (`pixdesc::pixel_format_descriptor::descriptors_rebuilt_through_setters_read_like_c_s_table`), which the gate at `2f6e60458d` also finished; the symbol tests of this workset are all done within the first 20 min |
| Miri, FFI-free unit tests of `imgutils.rs` and `samplefmt.rs` | 17 passed |
| `crustify scan-unsafe` | one unsafe block added (`Image::alloc`'s `write_bytes`, documented); undocumented sites 25 -> 24 (`conventions/`) |

The reviewed base `2f6e60458d` is the wave head; the wave branch had not
moved when this landed, so no rebase was needed.
