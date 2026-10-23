# Review: libavutil/formats wave 0, batch 0 (the wave's twelve types)

- Items: `AVAlphaMode`, `AVChannel`, `AVChannelOrder`, `AVChromaLocation`,
  `AVColorPrimaries`, `AVColorRange`, `AVColorSpace`,
  `AVColorTransferCharacteristic`, `AVComponentDescriptor`,
  `AVMatrixEncoding`, `AVPixelFormat`, `AVSampleFormat`.
- Homes: `crustify/rust/ffmpeg/libavutil/{pixfmt,channel_layout,pixdesc,samplefmt}.rs`.
- Reviewed commit: the wave tip `17c012afcd` (types landed by batches 0-6:
  `41844f9750`, `33cbfb768a`, `eb9a1ebc0c`, `889a1ec70f`, `653bf3f6d0`,
  `0c753e50ee`, `b39657734d`).
- Reports: `misc/missing-default-on-none-valued-enums` (fixed) and
  `wavefront/matrix-encoding-underlying-type` (finding resubmitted). Every
  other step below found nothing to report.

## Verdicts by step

| step | finding |
|---|---|
| BSan gate | The wave's gate failure (`review-wave-0/bsan-gate.log`) is in the `equiv` binary only and reproduces only when the `cpu::` and `imgutils::` modules run in parallel; its report site is C `av_strtod` under `av_parse_cpu_caps`, and the suspected cause is `tests/equiv/imgutils.rs`'s process-wide log callback. None of the report touches a type of this workset (the lone type test it names, `channel_layout::matrix_encoding::option_default_and_range_span_the_named_values`, passes alone and in every combination it was tried in); it belongs to the symbol review (batch 1). This workset's modules pass BSan on all four binaries (see evidence). |
| wavefront | Eleven of the twelve findings match the C: every enum is field-less with no lifecycle, the open value set is justified by the public fields and `-1`/`AVERROR` returns each cites, and `AVComponentDescriptor` is a plain five-`int` value embedded in the static descriptor table and copied by value by its readers. `AVMatrixEncoding`'s finding called the encoding `c_int`; it is `c_uint` (no negative enumerator; bindgen emits `c_uint`). Resubmitted, see the report. |
| lifecycle | None exists: C never allocates, frees, clones or disposes any of the enums, and `AVComponentDescriptor` lives only by value as `AVPixFmtDescriptor.comp[4]` (static table) with no routine taking its pointer. The `Copy` newtypes and the `CZeroable`, `Copy` layout type with `new()` / `from_raw` / `into_raw` are the complete representations. |
| unsafe boundary | No type exposes an `unsafe fn` or a raw pointer: `from_raw` on each enum is safe because every bit pattern is a value C may hold, and `ComponentDescriptor::from_raw` takes the C struct by value. `crustify scan-unsafe --sites all` reports no smell counter in the four homes; the only unsafe sites are `pixdesc.rs`'s ten field projections and whole-value read/write (all through `addr_of!`/`addr_of_mut!` on the handle pointers, each with a specific `SAFETY:` comment), the `define_ctype!` seam, and the five `from_name` FFI calls that belong to the symbol route. |
| ub_safe / ub_unsafe | No UB reachable: the enums are `#[repr(transparent)]` over bindgen's `c_int`/`c_uint` newtypes (layout tests), `Channel::ambisonic` / `ambisonic_index` and the two `index()` methods bound-check before casting, and `ComponentDescriptor`'s `Send`/`Sync` impls (value, `Ref`, `Mut`) cover plain data with getters on `Ref` only. 86 FFI-free unit tests pass under Miri. No `ub_unsafe` surface: the workset adds no unsafe public API. |
| equiv | Every enumerator and every `AV_PIX_FMT_NE` alias (273 + 104) has a constant with the C value; `Default`s match `get_frame_defaults` (now asserted for all seven frame-stored pixfmt types in one place, plus `format` = -1); `Channel::ambisonic` is C's `AMBI<n>` parse and `MatrixEncoding::COUNT` / `SampleFormat::COUNT` are the `_NB` counts. No divergence found. |
| internal | `name()` tables and `Debug` cover named, aliased and unnamed values; `ComponentDescriptor`'s `PartialEq`/`Hash` are field-wise and consistent. No defect. |
| conventions | `Wraps:` anchors with `Handwritten` on the eleven enums, `Wraps:` plus five `Field:` anchors on the descriptor; names drop the `AV_`/`AVCOL_`/`AV_PIX_FMT_` prefixes; accessors on `Ref`/`Mut` project with `addr_of!`; rustdoc builds with `-D warnings`. |
| misc | Four enums (`PixelFormat`, `SampleFormat`, `Channel`, `MatrixEncoding`) lacked the `Default` the other eight carry although C has the same unset value for each (`NONE`). Added, with unit and C-backed equivalence regressions; see the report. |

## Coverage

Rust (`llvm-cov` over `--lib`, `equiv` and `ub_safe` filtered to the four
modules, before the fix): `channel_layout.rs`, `pixdesc.rs` and
`samplefmt.rs` 100% of lines and regions; `pixfmt.rs` 551/552 lines, the
one miss being the big-endian arm of `native_endian`, unreachable on this
little-endian host. The fix adds four `Default` impls, each executed by its
new unit and equivalence tests.

C: no C function belongs to this workset (enums have no code and
`AVComponentDescriptor` has no lifecycle routine), so there is no C path to
measure; the descriptor's C readers (`av_read_image_line2`,
`av_get_padded_bits_per_pixel`) are exercised by `tests/equiv/pixdesc.rs`
over every component of every table descriptor.

## Validation at the fixed tree

- `cargo check --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `RUSTDOCFLAGS=-D warnings cargo doc -p ffmpeg --no-deps`: clean.
- plain `cargo test --workspace`: 395 lib, 351 equiv, 325 ub_safe, 63 ub_unsafe,
  53 doctests, 0 failed.
- ASan/UBSan and TSan (`-Zbuild-std`, the matching C variants): the same counts,
  no `runtime error:` and no sanitizer report.
- BSan (`cargo bsan`, bsan C build): whole-crate lib 395, ub_unsafe 63 and
  ub_safe 324 pass; the four modules' equiv tests (68, the new ones included)
  pass. The whole equiv binary still fails at the gate's site (`av_strtod`
  under `cpu::` while `imgutils::` runs in parallel), which the fix neither
  touches nor changes; it is the symbol review's.
- Miri: the 86 FFI-free unit tests of the four modules pass.
