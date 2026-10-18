# Review: formats wave-3 symbols (channel layout initialisers, readers, describers, compare, retype, subset, standard table, uninit)

Route: **symbol** (18 items). Objective: **review**. Affected base:
`0cc48b6804` (the wave's head; the items landed in translation batches 1,
`0cc48b6804`, and 2, `0e2797fe0a`; the five initialisers and `uninit`
were wrapped by the wave-2 `AVChannelLayout` type run and carried into
this wave's worklist).

Items and homes, all in `crustify/rust/ffmpeg/libavutil/channel_layout.rs`:

- initialisers and lifecycle: `av_channel_layout_custom_init`
  (`ChannelLayout::custom`), `av_channel_layout_from_mask`
  (`ChannelLayout::from_mask`), `av_channel_layout_from_string`
  (`ChannelLayout::parse`), `av_channel_layout_default`
  (`ChannelLayout::default_for`), `av_channel_layout_copy`
  (`ChannelLayoutRef::try_clone`, `ChannelLayoutMut::copy_from`),
  `av_channel_layout_uninit` (`ChannelLayoutUninit`,
  `ChannelLayoutMut::clear`);
- readers: `av_channel_layout_check` (`ChannelLayoutRef::is_valid`),
  `av_channel_layout_ambisonic_order` (`ambisonic_order`),
  `av_channel_layout_channel_from_index` (`channel`),
  `av_channel_layout_index_from_channel` (`index_of`),
  `av_channel_layout_index_from_string` (`index_by_name`),
  `av_channel_layout_channel_from_string` (`channel_by_name`),
  `av_channel_layout_describe` (`write_description`, `description`),
  `av_channel_layout_describe_bprint` (`append_description`),
  `av_channel_layout_compare` (`PartialEq` for `ChannelLayoutRef` and
  `ChannelLayout`), `av_channel_layout_subset` (`subset`);
- conversions and the table: `av_channel_layout_retype`
  (`ChannelLayoutMut::retype`, `retype_lossless`, `canonicalize`),
  `av_channel_layout_standard` (`standard_layouts`, `StandardLayouts`).

## Findings and scope

- **BSan gate.** Run on the wave tip (`0cc48b6804`) and on the landed
  tree, one `cargo bsan test` per test binary in parallel under
  `timeout 7200` with `gc_hang_threshold_ms=600000`; see Validation.
- **One UB defect, fixed:**
  `ub_safe/parse-ambisonic-int-max-order-overflow/`. `ChannelLayout::parse`
  passed every description to `av_channel_layout_from_string`, whose range
  check of an `ambisonic <order>` computes `order + 1` in `int` before
  testing it: for an order `strtol` narrows to exactly `INT_MAX`
  (`2147483647`, `0x7fffffff`, `017777777777`, ` +2147483647`,
  `-2147483649`, `6442450943`, in the first segment or in any `+`-chained
  one after a valid order) that is a signed overflow at
  `channel_layout.c:341` (UBSan), then `:346`, after which C returns
  success with an ambisonic layout of no channels. The fix mirrors
  `strtol(s, &end, 0)` (`c_strtol`, checked against glibc on 20 000
  random strings) and C's walk of the chain (`ambisonic_order_overflows`)
  and refuses those descriptions with `AVERROR(EINVAL)`, C's own answer
  for the next order down, before the call; documented in `parse`'s
  `# Errors`. Regression: `tests/ub_safe/channel_layout.rs::layout::
  parse_refuses_the_ambisonic_order_c_cannot_add_one_to` and three unit
  tests. A 300 000-round random fuzz of the C parser under ASan/UBSan
  (alphabet of the forms it accepts) reports only those two lines, so
  no other parser input is undefined. The reproducer exits 1 on the
  affected commit and 0 on the landed tree.
- **No other UB.** Read against `channel_layout.c`: the readers' guards
  (`walkable`, `ambisonic_computable`, `names_terminated`,
  `searchable_for`) refuse exactly the states C reads through (a NULL or
  negative-count custom map), overflows on (`nb_channels - popcount`,
  the ambisonic square at `highest_ambi >= 46340^2`, which a custom map
  cannot reach since its ACN prefix is at most 1024 long) or reads names
  as strings from; `subset`, `is_valid` and the retype family rely on
  C's own NULL check or add one; `standard_layouts` passes an index
  through a provenance-free pointer C never dereferences; the
  initialisers only ever hand C a fresh zeroed value and own what C
  stored on every return path, including the custom-with-NULL-map state
  `from_string` leaves on ENOMEM. ASan/UBSan and TSan over the whole
  crate: no report, no `runtime error:`. `ub_unsafe`: the workset adds
  no public unsafe API; nothing to evaluate.
- **No equivalence defect.** The landed suites compare every reader on
  33 start layouts (every order, map shape and extreme count), every
  standard layout and 33 descriptions, after each of 29 steps of a
  mixed setter/retype sequence, and every pair under `==`; retype on 33
  starts × 5 orders × 4 flag sets; subset on every start and every bit;
  the table walked, resumed and copied. The one divergence is the fix
  above, by design and documented. Six descriptions added to both
  suites' lists keep C's `long`→`int` narrowing (`4294967297` is order
  1, `0x100000002` order 2), C's own refusal at `2147483646`, and three
  parser branches no input took (a byte other than `+` after the order,
  `strtoull`'s `errno`, a missing `)`) compared with C.
- **No internal defect**: the guards and `compares_map_names` have inline
  tests at their edges; this review adds `c_strtol`,
  `ambisonic_order_overflows` and an inconsistent-count reader test
  (C's native index walk running off the mask); Miri passes the FFI-free
  ones.
- **Wavefront**: one finding corrected,
  `wavefront/from-string-ambisonic-order-overflow-unrecorded/`: the
  `str` note of `av_channel_layout_from_string` recorded no undefined
  input; resubmitted with the hazard, the spellings and the wrapper's
  refusal. The other seventeen were checked against the bodies and
  stand.
- **No lifecycle work**: symbol-only route; the type's lifecycle
  (`ChannelLayoutUninit`, `CVal`) was reviewed with wave 2.
- **Safe boundary / conventions**: every public entry point of the
  workset is safe and takes typed handles, `&CStr`, slices or a
  `BPrintMut`; no raw pointer on the wave-3 surface (`opaque` /
  `set_opaque` are wave-2 field cookies); `crustify scan-unsafe` lists
  no smell at a wave-3 site (`raw_ptr_pub_smell` at lines 755, 841,
  1879, 1913 are the `opaque` accessors; `raw_ptr_priv_smell` at 959,
  1441, 1810 are wave-1/2 helpers), no undocumented unsafe block in the
  home; one filled anchor per item (`copy`, `describe` and `uninit`
  twice, `retype` three times, for their distinct contracts); clippy
  `--workspace --all-targets -D warnings` and rustdoc `-D warnings`
  clean; rustfmt diffs of the three touched files are the base's own.
- **Misc (fixed)**:
  `misc/index-by-name-doc-overstated-the-unterminated-name-guard/`: the
  doc of `index_by_name` / `channel_by_name` said the lookup refuses a
  map name with no NUL "the search would meet", where the guard refuses
  any unterminated name in the map; reworded.
- **Coverage**: `coverage.json`. C: 100% of lines of all 18 routines
  and their five static helpers (`parse_channel_list` 96.8%: its ENOMEM
  arms); the untaken branches left are out-of-memory arms, a
  NULL-with-size buffer the wrapper never passes, an order C does not
  know, and a custom map of zero channels, none reachable from the safe
  API. Rust: 100% of the wave-3 sections and of the fix; the home's 15
  uncovered lines are a wave-2 `const fn` and wave-2 test `panic!()`
  arms.

## Validation

On the final tree (the fix, the doc rewording and the new tests applied):

| check | result |
|---|---|
| `cargo check --workspace` | ok |
| `cargo clippy --workspace --all-targets -- -D warnings` | ok |
| `RUSTDOCFLAGS=-D warnings cargo doc -p ffmpeg --no-deps` | ok |
| `cargo test --workspace` (plain) | lib 563, equiv 462, ub_safe 478 (compile_fail included), ub_unsafe 72, doctests 83 |
| ASan/UBSan (`-Zbuild-std`, four binaries, `--skip compile_fail`) | lib 563, equiv 462, ub_safe 477, ub_unsafe 72; no report, no `runtime error:` |
| TSan (same) | same counts; no report |
| BSan gate on the wave tip `0cc48b6804` (per binary, in parallel) | lib 558 (76 s), equiv 462 (1541 s), ub_safe 476 (2524 s), ub_unsafe 72 (218 s); no report |
| BSan gate on the landed tree (per binary, in parallel) | lib 563 (99 s), equiv 462 (1603 s), ub_safe 477 (2604 s), ub_unsafe 72 (275 s); no report |
| Miri, FFI-free `channel_layout` unit tests (`c_strtol_reads_as_glibc_does`, `ambisonic_order_overflow_is_found_where_c_reads_the_order`, `map_names_are_compared_only_after_an_at_with_text`) | 3 passed |
| reproducer, asan-ubsan build | exit 1 on `0cc48b6804`, exit 0 on the landed tree |
| C parser fuzz, 300 000 rounds under ASan/UBSan | only `channel_layout.c:341`/`:346` |
| `crustify scan-unsafe` | no new site |
