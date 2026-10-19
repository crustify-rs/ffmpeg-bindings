# Review: formats wave-2 symbols (descriptor lookups, image lines, bit counters, pixel steps, luma coefficients, channel name appenders)

Route: **symbol** (13 items). Objective: **review**. Affected base:
`ac34a06476` (the wave's head; the items landed in translation batch 1,
`57c0cc8756`).

Items and homes:

- `crustify/rust/ffmpeg/libavutil/pixdesc.rs`: `av_pix_fmt_desc_get`
  (`PixelFormat::descriptor`), `av_pix_fmt_desc_get_id`
  (`PixelFormatDescriptorRef::pixel_format`), `av_pix_fmt_desc_next`
  (`PixelFormatDescriptorRef::next_in_table`, `descriptors()` /
  `Descriptors`), `av_get_bits_per_pixel`
  (`PixelFormatDescriptorRef::bits_per_pixel`),
  `av_get_padded_bits_per_pixel` (`padded_bits_per_pixel`),
  `av_read_image_line2` (`read_image_line<T: LineElement>`),
  `av_read_image_line` (`read_image_line_u16`), `av_write_image_line2`
  (`write_image_line`), `av_write_image_line` (`write_image_line_u16`);
- `crustify/rust/ffmpeg/libavutil/imgutils.rs`: `av_image_fill_max_pixsteps`
  (`max_pixel_steps`, `MaxPixelSteps`);
- `crustify/rust/ffmpeg/libavutil/csp.rs`: `av_csp_luma_coeffs_from_avcsp`
  (`ColorSpace::luma_coefficients`);
- `crustify/rust/ffmpeg/libavutil/channel_layout.rs`:
  `av_channel_name_bprint` (`Channel::append_name`),
  `av_channel_description_bprint` (`Channel::append_description`).

## Findings and scope

- **BSan gate.** The wave's gate (`review-wave-2/bsan-gate.log`) fails in
  `av_channel_layout_from_string` (`channel_layout.c:342`, `:417`), reading
  through an end pointer that uninstrumented glibc `strtol`/`strtoull`
  wrote; the orchestrator reproduced it from a pure-C program. That routine
  is batch 0's (`ChannelLayout::parse`, type route) and no test of this
  workset calls it; nothing in the log touches these 13 items. This
  review ran BSan on the workset's own tests (filters below): lib 15,
  equiv 16 (17 with the new test), ub_safe 14, no report.
- **No UB defect.** The line readers and writers are the only items whose
  safety rests on wrapper arithmetic (`line_plan`, `LineArgs::new`, `row0`,
  `inside`), so beyond reading them against `pixdesc.c:30-196` I wrote an
  independent model: a literal transliteration of C's two loops with every
  `int` operation checked for the cases C leaves undefined (the reader's
  `1ULL << depth`, `data[plane]`, `y * linesize`, `skip = x * step +
  offset`, `8 - depth - (skip & 7)`, the per-iteration `shift` range and
  `shift -= step`, `x * step`, `shift + depth`, every `<<`/`>>` by `shift`
  or `offset`, and `4 * val + c` in `int` or wrapping `unsigned` as each
  reader has it) and every pointer offset tracked in `i64`, which for a
  random descriptor (C's table or one Rust-built component at the edges:
  planes outside `0..4`, steps and offsets at `INT_MIN`/`INT_MAX`, depths
  `-1`, `0`, `31`..`65`, every flag combination), random planes (lengths
  0..700 and ~1 KiB, linesizes negative, zero, positive and beyond `int`)
  and random lines (positions up to `usize::MAX`, widths 0..12 and 4096)
  says whether C's accesses are defined and inside the planes under the
  wrappers' documented row-0 convention. The wrappers accepted exactly
  the lines the model accepts, over 300 000 rounds plain and 300 000
  under ASan/UBSan (`UBSAN_OPTIONS=halt_on_error=1`), with every accepted
  line (≈29 000 a run) executed on exactly sized heap planes of `0xFF`
  bytes (so every palette index is its largest) with no report. The
  probe is kept at `probe/` (`cargo run -- <rounds> <seed>`), and a
  40 000-round version is now `tests/equiv/pixdesc.rs::random_lines::
  lines_are_accepted_exactly_where_c_is_defined_and_in_bounds` (500
  rounds under `CRUSTIFY_FFMPEG_VARIANT=bsan`, ~20 s there; 0.4 s plain). The whole-descriptor
  counters refuse exactly what makes C's `depth << s` / `step << s`,
  sums, `* 8`, `>> log2_pixels` or `steps[comp->plane]` undefined
  (checked against `pixdesc.c:3412-3444`); `max_pixel_steps` checks all
  four slots' planes, as C reads all four; the table lookups hand out
  `'static` shared handles into a `const` table and `pixel_format` /
  `next_in_table` give C only NULL or an address `av_pix_fmt_desc_get_id`
  places inside it; the appenders pass an initialised `AVBPrint` through
  the exclusive handle. ASan/UBSan and TSan over the whole crate: no
  report, no `runtime error:`.
- **`ub_unsafe`**: the workset adds no public unsafe API; nothing to
  evaluate.
- **No equivalence defect.** The landed suites compare every table entry
  (lookup, identity, iteration step by step from NULL and from each entry,
  both bit counters on the entry and on a Rust copy), every format ×
  component × three lines × both orientations × `u16`/`u32` × palette for
  the readers, eight-step write sequences compared after every step, the
  pixel steps with and without C's component array, every colour space
  value, and every channel value through both appenders into growing and
  exactly sized buffers. The random-lines model above is the one gap I
  saw (over-refusal on inputs the sweeps never form, such as negative
  steps on bottom-up planes); it found none. One over-refusal is by
  design and documented: a palette read whose component can index past
  `INT_MAX` is refused even where C's `unsigned` arithmetic would wrap
  rather than overflow (non-bitstream components deeper than 29 bits),
  since C would then need a palette of up to 4 GiB.
- **No internal defect**: `line_plan`, `LineArgs::new`, `row0`, `inside`,
  `c_shl`, the checked counters and the iterator have inline unit tests
  at their edges; Miri passes the 11 FFI-free ones.
- **Wavefront**: the 13 findings record the borrowed-for-the-call
  descriptors, buffers and print buffer, the `'static` table returns, the
  never-dereferenced `desc` of `av_pix_fmt_desc_get_id`, and the
  must-be-a-table-entry `prev` of `av_pix_fmt_desc_next`, as the bodies
  show. The plane arrays (`uint8_t *data[4]`, `int linesize[4]`) and the
  `int[4]` out-arrays of `av_image_fill_max_pixsteps` stay unrecorded,
  the known array-parameter gap. Only cosmetic: the appenders' notes say
  "`&mut BPrintMut`" where the wrappers take `BPrintMut<'_>` by value
  (the ownership they record, a mutable borrow for the call, is right);
  not resubmitted.
- **No lifecycle work**: symbol-only route.
- **Safe boundary / conventions**: every public entry point is safe and
  takes typed handles and slices; no raw pointer on the surface;
  `crustify scan-unsafe` lists no smell at a site this batch added (the
  `unsafe_fns_priv_smell` / `raw_ptr_priv_smell` at `pixdesc.rs:950` is
  wave 1's `static_name`); every unsafe block documented; one filled
  anchor per item (`av_pix_fmt_desc_next` twice, for the step and the
  iterator); clippy `--workspace --all-targets -D warnings` and rustdoc
  `-D warnings` clean.
- **Misc (fixed)**: two doc omissions,
  `misc/line-reader-docs-omit-negative-formats-and-the-ignored-x/`:
  `PixelFormat::descriptor` did not say every negative value is `None`,
  and the line readers did not say that for the 32-bit-word bitstream
  formats (`xv30be`, `v30xbe`) C ignores `x`.
- **Coverage**: `coverage.json`. C: 100% of lines of all 13 routines, and
  all 66 branches of the two line routines. Rust: 100% of lines in the
  batch's sections of all four homes; the only uncovered lines in the
  home files are wave-1 arms and batch-0 test `panic!()`s.

## Validation

On the final tree (the doc fixes and the new equivalence test applied):

| check | result |
|---|---|
| `cargo check --workspace` | ok |
| `cargo clippy --workspace --all-targets -- -D warnings` | ok |
| `RUSTDOCFLAGS=-D warnings cargo doc -p ffmpeg --no-deps` | ok |
| `cargo test --workspace` (plain) | lib 539, equiv 445, ub_safe 451 (compile_fail included), ub_unsafe 72, doctests |
| ASan/UBSan (`-Zbuild-std`, four binaries, `--skip compile_fail`) | lib 539, equiv 445, ub_safe 450, ub_unsafe 72; no report, no `runtime error:` |
| TSan (same) | same counts; no report |
| BSan (`cargo bsan test`, `gc_hang_threshold_ms=600000`, the workset's modules: `descriptor_table image_lines random_lines pixel_steps luma_lookup name_appenders` plus the planner unit tests) | lib 15, equiv 17, ub_safe 14; no report |
| Miri, FFI-free `pixdesc` unit tests | 11 passed |
| random-lines probe, plain and ASan/UBSan, 300 000 rounds each | 0 mismatches, no report |
| `crustify scan-unsafe` | no new site |

The reviewed base `ac34a06476` is the wave head at landing time unless the
types review (batch 0) lands first, in which case this was rebased onto it.
