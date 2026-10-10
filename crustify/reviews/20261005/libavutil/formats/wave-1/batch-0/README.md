# Review: libavutil/formats wave 1, batch 0 (the wave's four types)

- Items: `AVCIExy` (`csp::CieXy`, alias `WhitepointCoefficients`),
  `AVLumaCoefficients` (`csp::LumaCoefficients`), `AVChannelCustom`
  (`channel_layout::ChannelCustom`, with `ChannelName` and `NameTooLong`),
  `AVPixFmtDescriptor` (`pixdesc::PixelFormatDescriptor`, with
  `PixelFormatFlags`).
- Homes: `crustify/rust/ffmpeg/libavutil/{csp,channel_layout,pixdesc}.rs`.
- Reviewed commit: the wave tip `2f6e60458d` (types landed by batches 0-2:
  `b5a98ab3b9`, `0bbd0f7f6b`, `a60ec0cb84`).
- Reports: `misc/channel-custom-eq-without-hash` (fixed),
  `misc/bsan-quadratic-pixdesc-descriptor-equiv-tests` (fixed, tests
  only) and `internal/descriptor-unit-tests-assume-literal-address-identity`
  (fixed, tests only). Every other step below found nothing to report.

## Verdicts by step

| step | finding |
|---|---|
| BSan gate | The gate failed on the 7200 s timeout of the `equiv` and `ub_safe` binaries, with no BSan report. The orchestrator's triage names symbol-route tests only (`imgutils::images`, `samplefmt::buffers`); the one test of this workset it lists, `csp::cie_xy::heap_box_owner_lives_reads_writes_clones_and_frees`, passes alone in 6 s and merely never got scheduled. This workset's own share of the budget was two `pixdesc::pixel_format_descriptor` equiv tests at 215 s and 1432 s alone, by the per-byte-retag mechanism the symbol review's `batch-1/misc/bsan-gate-per-byte-retags-in-image-alloc-and-sweeps` names for its own tests; see the second misc report (now 7 s and 215 s). The whole lib (501) and `ub_unsafe` (72) binaries and the workset's `equiv` (16) and `ub_safe` (62) modules pass under BSan. |
| wavefront | All four findings match the C: `AVCIExy` and `AVLumaCoefficients` are pointer-free `AVRational` pairs/triples with no lifecycle routine (`--lifecycle-ops` empty), embedded by value in `csp.c`'s static tables and copied by value by every reader; `AVChannelCustom` is the by-value element of `AVChannelLayout.u.map` whose storage `av_channel_layout_{custom_init,from_string,retype,copy,uninit}` own (wave 3), with `opaque` a user cookie no FFmpeg path dereferences or frees (`channel_layout.c` touches only `AVChannelLayout.opaque`); `AVPixFmtDescriptor` lives only in `pixdesc.c`'s `static const` table or in caller storage, its `name`/`alias` string literals (`borrowed: static`, `mutable: false`). Nothing to resubmit. |
| lifecycle | Complete: no C routine allocates, frees, clones or disposes any of the four on its own, so each is the `define_ctype!` value with `Ref`/`Mut` handles, `CZeroable` (every all-zero state is one C itself stores), `Copy`, `new`/`from_raw`/`into_raw`, and Rust-side heap storage through the generic `av_malloc` policies (`HeapBox`, `HeapVec`, `alloc_cell_array_zeroed`), exercised by the ub_safe suites. The map that holds `AVChannelCustom`s is `AVChannelLayout`'s to allocate and free (wave 3). |
| unsafe boundary | One public `unsafe fn`, `PixelFormatDescriptor::from_raw`, whose `# Safety` contract (NULL or never-freed, never-written NUL-terminated `name`/`alias`) is exactly the type invariant the `&'static CStr` getters and setters rest on, and which no safe path can break (`new`/`Default`/zeroed owners, setters taking `&'static CStr`, `get()` copies). Two raw-pointer sites, `ChannelCustomRef::opaque` / `ChannelCustomMut::set_opaque` (`*mut c_void`), are the user cookie itself: nothing dereferences it on either side, storing or reading an address is safe, and no wave will replace it with a handle (there is no pointee type to wrap); it follows the crate's existing cookie convention (`log::Message::context`). `crustify scan-unsafe --sites all` reports no `ref_to_type_wrapper` or private-unsafe smell in the three homes; every field access projects with `addr_of!`/`addr_of_mut!` on the handle pointers under a specific `SAFETY:` comment (clippy's `undocumented_unsafe_blocks` is denied and clean). Three `SAFETY:` comments longer than the lexical scanner's six-line look-back were reflowed so the scanner agrees with clippy. |
| ub_safe / ub_unsafe | No UB reachable. The getters read plain fields; `ChannelName` copies the 16-byte field and stops at its end (an unterminated field, which only a raw write leaves, reads as 16 bytes where C would overrun); `set_name` refuses 16+ bytes and `set_name_truncated` writes `av_strlcpy`'s bytes and NUL exactly; the component runs are exactly four elements over the embedded array; the `Send`/`Sync` impls cover plain data (the descriptor's strings are `'static` by invariant). Whole-crate ASan/UBSan and TSan, workset BSan (above), and Miri over the 41 FFI-free unit tests of the three homes pass. The `ub_unsafe` suite for `from_raw` (literal, NULL and leaked strings; outliving the raw struct and its frame; threads; C's table entries) passes under every instrument. |
| equiv | The suites already observe every field through C: gamut identification after every coordinate write (`av_csp_primaries_id_from_desc`), the luma table read through the handles, twin custom maps written by `av_strlcpy` and by the setters and read through `av_channel_layout_describe`/`_check`/`_index_from_string`/`_channel_from_index` after each step, and descriptors rebuilt field by field under `av_get_bits_per_pixel`, `av_get_padded_bits_per_pixel`, `av_read_image_line2` and `av_write_image_line2`. `ChannelCustom`'s `PartialEq` (id, name to its NUL, pointer address) is a Rust value equality; C's own `av_channel_layout_compare` compares custom layouts by id only, which is the layout's (wave 3) comparison to wrap, not the element's. No divergence found. |
| internal | `ChannelName::from_field`, `write_name`, `DescriptorFields`, the flag `Debug`s and `gamma` are each covered by unit tests, Miri included where FFI-free; no defect in the private implementation. Two `pixdesc` unit tests asserted that a second occurrence of a string literal has the address the setter stored, which Rust does not promise and Miri does not provide (both fail under `-Zmiri-seed=0..5` on the reviewed tree; one failed in this review's unseeded pass); they now compare the stored pointer with the getter's and the contents with the literal. See the internal report. |
| conventions | `Wraps:` anchors on the four types, `Field:` anchors on every field accessor (`x`, `y`; `cr`, `cg`, `cb`; `id`, `name`, `opaque`; `name`, `nb_components`, `log2_chroma_w`, `log2_chroma_h`, `flags`, `comp`, `alias`); getters on `Ref`, setters on `Mut`; names drop the `AV` prefix and read as Rust (`component_count`, `PixelFormatFlags`); rustdoc builds with `-D warnings`. |
| misc | `ChannelCustom` was the only `Eq` value type of the subsystem without `Hash` (its siblings `Channel`, `ChannelName`, `ComponentDescriptor`, `PixelFormatDescriptor` have one); added over the fields its equality compares, with a unit regression. Two `pixdesc` equivalence tests spent 215 s and 1432 s under BSan walking plane bytes through slice iterators; rewritten as whole-slice operations with the same assertions (7 s and 215 s). |

## Coverage

Rust (`llvm-cov` over the lib, `equiv`, `ub_safe` and `ub_unsafe` binaries
at the reviewed tree): `csp.rs` 507/507 lines, 850/850 regions, 98/98
functions; `channel_layout.rs` 713/713 lines, 1118/1118 regions, 120/120
functions; `pixdesc.rs` 917/917 lines, 1354/1354 regions, 154/154
functions. The fix adds one `Hash` impl, executed by its new unit test.
Details in `coverage.json`.

C: no C function belongs to this workset (the four are by-value structs
with no lifecycle routine), so there is no C path to measure; the C
readers the equivalence tests observe through belong to the symbol route.

## Validation at the fixed tree

- `cargo check --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `RUSTDOCFLAGS=-D warnings cargo doc -p ffmpeg --no-deps`: clean.
- plain `cargo test --workspace`: 502 lib, 418 equiv, 416 ub_safe (the
  compile-fail programs included), 72 ub_unsafe, 54 doctests, 0 failed.
- ASan/UBSan and TSan (`-Zbuild-std`, the matching C variants, whole
  crate): 502 / 418 / 415 / 72, no `runtime error:` and no sanitizer
  report; the lib rerun after the last unit-test edit likewise.
- BSan (`cargo bsan`, bsan C build): lib 502 and `ub_unsafe` 72 whole;
  the workset's five `equiv` modules (16 tests, 224 s, down from 1416 s)
  and `ub_safe` modules (62 tests, 6 s); no report.
- Miri: the 42 FFI-free unit tests of the three homes (the new `Hash`
  test included), and the two fixed tests under seeds 0-5.
- `crustify scan-unsafe --sites all`: in the three homes the only smell
  sites on this workset's items are the two documented `opaque` cookie
  accessors (`raw_ptr_pub_smell` / `void_ptr_pub_smell`); the others
  (`static_name`, `name_buffer`) are the symbol route's. The fixes change
  no counter but `ref_to_type_wrapper_sanctioned` (+1, the `Hash` impl's
  `&self`), and the workset's files have zero undocumented unsafe sites
  (the crate total went 25 to 22 with the three reflowed comments).
