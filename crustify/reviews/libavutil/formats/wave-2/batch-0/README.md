# Review: libavutil/formats wave 2, batch 0 (the wave's two types)

- Items: `AVChannelLayout` (`channel_layout::ChannelLayout`, with
  `ChannelLayoutVal`, `ChannelLayoutUninit`, `LayoutDetails`) and
  `AVPrimaryCoefficients` (`csp::PrimaryCoefficients`).
- Homes: `crustify/rust/ffmpeg/libavutil/{channel_layout,csp}.rs`.
- Reviewed commit: the wave tip `693ccef878` (types landed by batch 0;
  batch 1 `c1518f9fe0` landed the wave's symbols).
- Reports: `misc/bsan-stale-stack-shadow-under-glibc-strtol-end-pointers`
  (the gate failure; tests only) and
  `misc/channel-layout-header-constants-unreachable` (fixed: the header's
  three macro families). Every other step below found nothing to report.

## Verdicts by step

| step | finding |
|---|---|
| BSan gate | The gate's three aborts (`lib`'s isolated child, `equiv`, `ub_safe`) are one BSan artifact in C, not a wrapper defect: `av_channel_layout_from_string` reads `endptr` / `end`, uninitialised locals that uninstrumented glibc `strtol` / `strtoull` wrote, whose shadow still carries the freed `av_strdup` block an *earlier* call of the same function left in that stack slot. A pure-C probe reproduces it as a sequence effect (`3c` then `ambisonic 1` fails; either alone passes), so which inputs trip it depends on the caller's frames; the orchestrator's "on any input" is the right class and the wrong scope. Under `CRUSTIFY_FFMPEG_VARIANT=bsan` the suites now keep only the descriptions C resolves before `strtol` (standard names, accepted channel lists), through one shared predicate; nothing changes under any other instrument. See the first misc report. |
| wavefront | Both findings and the six symbol findings match the C: `AVChannelLayout` is by value, not address-sensitive, owns `u.map` only in custom order (`av_calloc` / `av_dynarray2_add` / `av_malloc_array` blocks `av_freep` releases), `opaque` is a cookie only `av_channel_layout_retype` touches (to preserve it); `uninit` is the disposer, `copy` the deep cloner (uninits `dst` first, leaves it mapless-custom on ENOMEM), `custom_init` / `from_mask` / `from_string` / `default` are in-place initialisers that never uninit first; `--lifecycle-ops` lists exactly those six. `AVPrimaryCoefficients` is three `AVCIExy` by value with no routine of its own. Nothing to resubmit. |
| lifecycle | Complete for the routines: `ChannelLayoutVal = CVal<ChannelLayout, ChannelLayoutUninit>`, every initialiser bound as a safe constructor on a fresh zeroed value (so C's overwrite-without-uninit leaks nothing), `try_clone` / `copy_from` over `av_channel_layout_copy`, `clear` over `uninit`, and the Rust-side map transfers (`set_custom_map` / `take_custom_map` over `HeapVec<ChannelCustom>`, whose `av_malloc` block `uninit`'s `av_freep` accepts). What was missing is the header's own constructors: the `AV_CHANNEL_LAYOUT_<NAME>` compound literals, their `AV_CH_LAYOUT_<NAME>` masks and the `AV_CH_<NAME>` channel bits, none of which bindgen can evaluate and none of which wavefront schedules. Added as `ChannelLayout::<NAME>` / `ChannelLayout::native(mask)` / `AMBISONIC_FIRST_ORDER`, `channel_layout::masks::*` and `Channel::mask()`, with `From<ChannelLayout> for ChannelLayoutVal`, three table shims as the C reference, and unit, equivalence and UB tests (second misc report). `PrimaryCoefficients` is the `define_ctype!` value with `new` / `from_raw` / `into_raw` / `zeroed` / `Default`, as its wave-1 siblings. |
| unsafe boundary | No public `unsafe fn` in either home. The raw-pointer sites on the workset are `ChannelLayoutRef::opaque` / `ChannelLayoutMut::set_opaque` (`*mut c_void`), the user cookie C never dereferences or frees, on the terms the wave-1 review accepted for `ChannelCustom`'s; `crustify scan-unsafe --sites all` reports those four (`raw_ptr_pub_smell` / `void_ptr_pub_smell`, two per type) and no `unsafe_fns` or `ref_to_type_wrapper` smell, before and after the fixes, with zero undocumented unsafe sites in both homes. Every field access projects with `addr_of!` / `addr_of_mut!` through the handle pointers under a specific `SAFETY:` comment; `copy_from` refuses the mapless-custom source C would read through, and clears the destination C leaves mapless on ENOMEM. The handles' `Send` / `Sync` proofs hold: `Ref` getters copy fields or lend a getter-only `CSlice`, `Mut` setters need `&mut`. |
| ub_safe / ub_unsafe | No UB reachable. Every safe path keeps a layout in a state `av_channel_layout_uninit` and C's readers accept (custom order with a non-NULL map of `nb_channels >= 1` elements, no map otherwise); `details()` and `into_custom_map_mut()` bound their runs by `nb_channels` and read a NULL or negative-count map as empty; the `compile_fail` programs reject views outliving or aliasing their owner. Whole-crate ASan/UBSan, TSan and BSan (below), and Miri over the 12 FFI-free unit tests, pass. The workset adds no public unsafe API, so there is no `ub_unsafe` obligation. |
| equiv | The suites compare every initialiser, `av_channel_layout_copy` over every destination kind, and setter sequences through C's readers after each step; `set_ambisonic` mirrors the parser's `int` bounds exactly (`(order+1)^2 <= INT_MAX`, extras `< INT_MAX - that`), `set_native` / `native` count as `av_channel_layout_from_mask`, and the documented divergences (`set_native(0)` and `unspecified(n <= 0)` accepted, `copy_from`'s ENOMEM clear) are deliberate and stated. The new constants are compared with the compound literals, masks and bits C evaluates, with `av_channel_layout_check` / `_compare`, and round-tripped through `av_channel_layout_describe` and both parsers. No divergence found. |
| internal | `masked`, `from_parts`, `init_with`, `map_parts`, `reset_to` and `write_name` are each covered by unit tests; the isolated-child allocation-failure test exercises every ENOMEM path the wrapper handles. No defect in the private implementation. |
| conventions | `Wraps:` anchors on both types and the six routines (`uninit` on the policy and `clear`, `copy` on `try_clone` and `copy_from`), `Field:` anchors on every worklist field (`order`, `nb_channels`, `u` with `u.mask` / `u.map`, `opaque`; `r`, `g`, `b`); getters on `Ref`, setters on `Mut`; Rust names (`channel_count`, `LayoutDetails`, `PrimaryCoefficients`); rustdoc builds with `-D warnings`; the review's additions are rustfmt-clean and the files keep exactly the base's pre-existing rustfmt hunks. |
| misc | The two reports above. |

## Coverage

Rust (`llvm-cov` over the lib, `equiv`, `ub_safe` and `ub_unsafe`
binaries at the fixed tree): `channel_layout.rs` 1364 lines 98.97%, 2112
regions 99.29%, 196 functions 99.49%; `csp.rs` 738 / 1280 / 135 all 100%.
The misses are `masks::bit`, a `const fn` only evaluated at compile time,
and `panic!()` arms of test destructurings. C (`gcov` variant, the two
modules' tests): every lifecycle routine 100% of its lines
(`av_channel_layout_from_string` 76/76), `parse_channel_list` 30/31, the
miss being its ENOMEM-only `av_freep` that `av_max_alloc` cannot reach.
Details in `coverage.json`.

## Validation at the fixed tree

- `cargo check --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `RUSTDOCFLAGS=-D warnings cargo doc -p ffmpeg --no-deps`: clean.
- plain `cargo test --workspace`: 542 lib, 448 equiv, 454 ub_safe (the
  compile-fail programs included), 72 ub_unsafe, doctests, 0 failed.
- ASan/UBSan and TSan (`-Zbuild-std`, the matching C variants, whole
  crate): 542 / 448 / 453 / 72 each, no `runtime error:` and no sanitizer
  report.
- BSan (`cargo bsan`, bsan C build, whole crate, `--no-fail-fast`): lib 542
  (47 s), equiv 448 (21 min), ub_safe 453 (35 min), ub_unsafe 72; no
  report. At the reviewed tree the same command fails the lib's isolated
  child and aborts equiv and ub_safe as the gate did.
- Miri: the 12 FFI-free unit tests of the two homes (`Channel::mask`, the
  `masks` composition, the layout checks and the `PrimaryCoefficients`
  value tests).
- `crustify scan-unsafe --sites all`: no counter but `total_stmts` (+2)
  and `code_lines` changes; the two homes' smell sites are the four
  documented `opaque` cookie accessors, as before.
