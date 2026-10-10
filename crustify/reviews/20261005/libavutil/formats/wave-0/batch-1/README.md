# Review: formats wave-0 symbols (name lookups, image size check, plane copies, csp and SAD callbacks)

Route: **symbol** (11 items). Objective: **review**. Affected base:
`7317de905cd9a976e6a17b3650ba4182f2fe170c` (the wave's head, which landed
translation batch 7 holding all 11 items).

Items and homes:

- `crustify/rust/ffmpeg/libavutil/pixdesc.rs`: `av_color_range_from_name`,
  `av_color_primaries_from_name`, `av_color_transfer_from_name`,
  `av_color_space_from_name`, `av_chroma_location_from_name` (the
  `from_name(&CStr) -> Option<Self>` associated functions of the `pixfmt`
  enum wrappers);
- `crustify/rust/ffmpeg/libavutil/imgutils.rs`: `av_image_check_size`
  (`check_size`, and the unsafe `check_size_with_context` over the raw
  `log_ctx` seam), `av_image_copy_plane` (`copy_plane`),
  `av_image_copy_plane_uc_from` (`copy_plane_from_uncacheable`);
- `crustify/rust/ffmpeg/libavutil/csp.rs`: `av_csp_trc_function`
  (`TrcFunction`), `av_csp_eotf_function` (`EotfFunction`);
- `crustify/rust/ffmpeg/libavutil/pixelutils.rs`: `av_pixelutils_sad_fn`
  (`SadFn`).

## Findings and scope

- **BSan gate (one candidate, not UB; fixed in the tests).** The wave's gate
  failed in the `equiv` binary at `av_strtod`'s `next[0]` on a
  `cpu::parse_caps` thread while `imgutils::check_size_logs_as_c` held a
  Rust log callback installed. The read is byte 3 of the live 4-byte block
  `av_expr_parse` owns, through a pointer uninstrumented glibc `strtod`
  wrote; the report comes and goes with an unrelated libc call in the
  callback or a debugger stop; ASan/UBSan, TSan and the plain build pass.
  `misc/bsan-stale-provenance-in-av-strtod-under-rust-log-callback/`
  has the triage, the gdb evidence and the experiment table. The test now
  runs alone in a child process (`isolated`, the pattern the inline
  `imgutils`/`pixelutils` unit tests use for `av_force_cpu_flags`), so no
  sibling suite logs through its callback under any instrument; it still
  runs under BSan. The rebased tree then showed the same report in the
  `ub_safe` binary (core's `log` callback tests beside `cpu::parse_caps`,
  three runs out of three); its four callback-installing tests got the same
  isolation, documented in the same report.
- **No UB defect** in the safe API. Each wrapper's guards were read against
  the C bodies and the x86 assembly: `copy_plane` and
  `copy_plane_from_uncacheable` place every row of both planes inside their
  slices (`plane_origin`), refuse rows wider than a linesize before C's
  `av_assert0`, return before C for empty planes (where the SSE4 loop
  `dec height; jnz` and a zero `bw` would run away), and hand C the SSE4
  path only on 16-byte aligned rows and linesizes whose last row holds
  `FFALIGN(bytewidth, 64)` bytes (`movntdqa`/`mova` over whole 64-byte
  blocks); `SadFn::call` bounds both blocks and enforces the block-size
  alignment the `aligned` request promised (`psadbw`'s memory operand needs
  `src1` aligned for the `_u` variants, `mova` needs `src2` for `_a`);
  `check_size_with_context` rejects a `log_offset` that would overflow
  `av_vlog`'s `level +=`; the csp functions are pure arithmetic on their
  arguments and the EOTFs touch exactly `c[0..3]`.
- **`ub_unsafe`**: the one public unsafe function the workset adds,
  `check_size_with_context`, has a correct and complete `# Safety` contract
  (NULL, or a live object led by a `const AVClass *` that `av_log`, the
  default callback's parent naming and the installed callback may read
  during the call); its suite covers NULL, class-less and classed contexts
  on several threads. The raw `*mut c_void` seam follows the crate's existing
  convention for parent log contexts (`file.rs`) while no safe handle for an
  `AVClass`-led object exists; nothing to narrow.
- **No equivalence defect.** The landed suites compare every lookup name and
  its near misses, step-by-step plane copies in every orientation (the
  SSE4 path's copies past the width included), every size-check edge and
  its logged line (level, class, text), and every csp/SAD function C hands
  out against the raw call. The documented divergences (misaligned or short
  fast-path planes copied row by row where C faults or overruns) are
  C defects and stay out of Rust.
- **No internal defect**: `plane_origin`, `plane_copy_origins`,
  `uc_call_is_sound`, `copy_rows` and `found` have inline unit tests at every
  edge; Miri passes the FFI-free ones.
- **No Wavefront correction needed**: the findings for the 11 items record
  the borrow-for-the-call string, plane and block arguments, the nullable
  type-erased `log_ctx`, the by-value `double c[3]` of the EOTF, and the
  x86 alignment and run-away conditions, as the C bodies and assembly show.
- **No lifecycle work**: symbol-only route.
- **No convention deviation**: clippy `--workspace --all-targets -D warnings`,
  rustdoc `-D warnings`, every unsafe block documented, one filled anchor per
  item (two for `av_image_check_size`, one per safe variant);
  `crustify scan-unsafe` shows no smell at any site this batch added beyond
  the documented `check_size_with_context` seam (`unsafe_fns_pub_smell`,
  `raw_ptr_pub_smell`, `void_ptr_pub_smell` at `imgutils.rs:55`).
- **Coverage**: `coverage.json`. Rust: 100% of lines, functions and regions
  in all four homes. C: every reachable line of the wrapped routines; the
  untaken lines are the NULL no-op and `av_assert0` arms the wrappers reject
  first, `av_image_check_size2`'s `max_pixels` branch that
  `av_image_check_size` cannot reach, and the `continue` on NULL table
  entries in tables that have none. No new test was needed.

## Validation (with the fix)

`cargo check --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test --workspace` (plain, on the rebased tree: lib 395, equiv 351,
ub_safe 325, ub_unsafe 63, doctests), ASan/UBSan and TSan (`-Zbuild-std`,
all four binaries, no report, no `runtime error:`), BSan (all four binaries,
under `gc_hang_threshold_ms=600000`), Miri on the FFI-free unit tests, the
formerly failing `equiv` pair under BSan three times, and `ub_safe cpu:: log::`
under BSan in a loop.
