# Review: `libavutil/hwcontext.c` wave 952, batch 0 (`AVHWDeviceContext`, `AVHWFramesConstraints` again)

- Items (2 types, homed in `src/libavutil/libavutil/hwcontext.rs`): `AVHWDeviceContext` (fields
  `type`, `av_class`, `free`, `user_opaque`, `hwctx`) and `AVHWFramesConstraints` (`max_width`,
  `max_height`, `min_width`, `min_height`, `valid_sw_formats`, `valid_hw_formats`). Wave 952's
  batches 1 and 2 are still `todo` and belong to this wave's batch 1.
- Reviewed commit: `1b7312582ee`, on the wave tip `3ee33a6864f`. It makes the four `hwctx` casts
  safe through three new unsafe traits, `BackendDeviceContext`, `BackendFramesContext` and
  `StableBackendFramesContext`, keyed on the context's device type. The frames accessors
  (`AVHWFramesContext`, wave 954's item) are included at the user's instruction, so they are
  reviewed here too.
- Objectives: lifecycle, unsafety, contracts, ub, equiv, internal, conventions, misc, coverage,
  disclose, fix.
- Reports: 3, each fixed, each with a `disclosure.md`:
  - [`contracts/fixture-device-impls-claim-every-vaapi-context/`](contracts/fixture-device-impls-claim-every-vaapi-context/),
    with an ASan reproducer
  - [`contracts/uninit-device-from-c-omits-hwctx-type/`](contracts/uninit-device-from-c-omits-hwctx-type/)
  - [`contracts/backend-traits-guard-fields-not-what-they-point-to/`](contracts/backend-traits-guard-fields-not-what-they-point-to/)

## Verdicts by objective

| objective | verdict |
|---|---|
| lifecycle | None. `AVHWDeviceContext`'s lifecycle lives on its `AVBufferRef` (`av_hwdevice_ctx_alloc`/`_create`/`_create_derived`, `av_buffer_ref`, `av_buffer_unref` → `hwdevice_ctx_free`). `AVHWFramesConstraints` is `av_hwdevice_get_hwframe_constraints` / `av_mallocz` (`CNew`) and `av_hwframe_constraints_free`. The ownership store records all of them, and the Rust binds all of them. The commit changes no lifecycle. |
| wavefront findings | No wrong inference. The `hwctx` note ("type-erased: the device type's public hardware context … the pointer itself is never replaced") agrees with the typing by device type, and with `hwcontext.c:181-213`/`266-298`. `chased_casts.csv` has no `erased_field` row for `hwctx`, which holds one struct per backend. |
| unsafety | None in the workset. `ffimap` (`--sites fn.unsafe.pub raw_ptr.fn_sig_arg.safe.pub raw_ptr.fn_sig_ret.safe.pub raw_ptr.field.pub`) lists no `hwctx` accessor. The hwcontext sites left are ffibox trait impls (`:767-809`), the seams `UninitDevice::from_c`/`into_c` and `HwConfig::as_ptr`/`as_mut_ptr`, `UninitDevice::init` (backends dereference caller handles unchecked, as the commit explains), and frames items outside the changeset (`transfer_data_from`, `pool`, `set_pool`). The three traits must stay `unsafe`: an implementation asserts a C layout. |
| contracts | Three findings, all fixed. (1) The test fixtures' `BackendDeviceContext` impls claimed `VAAPI` and `DRM`, which the frames fixture's 4-byte-`hwctx` devices also use. Safe `hwctx::<FakeHw>()` overflows the heap, as an ASan reproducer shows. (2) `UninitDevice::from_c` did not require `hwctx` to be the struct of `type`'s backend, which the safe accessors now rely on. (3) The traits' no-writer obligation covered fields, not memory the getters read through them. Otherwise the new `// SAFETY:` comments hold. The type check happens before any read. Nothing changes `type` or `device_ctx`: neither has a setter, and C never writes them after allocation. The free callbacks run after `device_uninit`/`frames_uninit`, with no other reference left. `ref_from_void_ptr`/`mut_from_void_ptr` give `None` for NULL. |
| ub | No defect in the crate's API beyond (1), which is a defect in test code. I tried other device types, NULL `hwctx`, frames on derived devices (`av_hwframe_ctx_create_derived` allocates the derived frames context on its own device), the free callbacks, reborrows, and shared reads through a frame's own reference. `ffimap`'s references into C memory (`raw_ptr.casted_to.std.borrow` at `:2347`, `:2550`, `:3603`, `:3978`) are the `user_data` getters over the Rust-owned `Box<Hooks>`, which the commit leaves unchanged. There is no `Send`/`Sync` claim in the workset. |
| equiv | No divergence. The accessors add a type check that has no C counterpart, and their `None` is documented. The `# Errors` change (`AVERROR(ENOMEM)`) matches `hwcontext.c`'s only failure for the list setters. |
| internal | No defect. `set_hw`/`get_hw` dispatch on the device type correctly. |
| conventions | No deviation. The traits are Rust-only items with no C counterpart, so they carry no anchor. The tests are in `ub_unsafe`, the compile-fail doctests sit on the traits, and the C-calling tests carry `cfg_attr(miri, ignore = "calls C")`. |
| misc | Nothing besides the reports. |
| coverage | No reachable gap. Every Rust line the commit added runs. In the workset's C, the zero-count paths are `av_hwdevice_get_hwframe_constraints`'s `av_mallocz` failure (`hwcontext.c:593`, depends on the environment), `av_hwframe_constraints_free`'s NULL branch (`:608`, not reachable from Rust, whose releaser always passes a live struct), and `av_hwdevice_ctx_alloc`'s success path, `hwdevice_ctx_free` and `hwdevice_ctx_get_name`, which need a compiled backend. The campaign builds have none. |

## Gates and validation

- Miri gate on the wave tip `3ee33a6864f`, before any change, one
  `cargo +nightly-2026-10-08 miri test -p ffmpeg-bindings <binary> -- --skip compile_fail` per
  binary: `--lib` 505 passed (463 ignored), `--test equiv` 30, `--test ub_safe` 151,
  `--test ub_unsafe` 14. No Miri error. `equiv` first failed on the parallel-build race
  (`extern location for libavutil_sys does not exist`) and passed when rerun alone.
- Miri gate on the landed tree, the same invocations: `--lib` 505 (463 ignored), `equiv` 30,
  `ub_safe` 151, `ub_unsafe` 14 (174 ignored, the new test among them). No Miri error. `equiv` again
  needed a rerun alone.
- BSan: not run as a gate. Wave 952 is not `libavutil/hwcontext.c`'s last translation wave: wave
  954 belongs to it and runs after it. The reproducer is an ASan finding.
- On the landed tree: `cargo +nightly-2026-10-08 fmt --all --check`,
  `cargo check --workspace --all-targets`, `cargo clippy --workspace --all-targets -- -D warnings`
  and `cargo test --workspace` (lib 968, equiv 723, ub_safe 821, ub_unsafe 188, doctests 113 + 116)
  all pass.
- ASan/UBSan and TSan (`CRUSTIFY_BUILD_VARIANT=<variant>
  CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS=-Zsanitizer=<address|thread>
  cargo +nightly-2026-10-08 test -Zbuild-std --target x86_64-unknown-linux-gnu <binary> hwcontext`)
  on the landed tree: lib 84, equiv 44, ub_unsafe 47 and ub_safe 18 pass under both, with no
  report.
- Coverage (`crustify/scripts/coverage/`), wave tip → landed tree: C 12,413 → 12,417 of 18,477
  lines (67.18% → 67.20%), Rust 15,367 of 15,511 (99.07%) on both. Per suite: `ub_unsafe` Rust
  4,383 → 4,394 (the new test), and C 4,917 → 4,924. The `ub_safe` C (9,813 → 9,821) and `equiv` C
  (12,319 → 12,317) figures moved in suites the review did not touch, which is thread timing.
- `ffimap`, wave tip against landed tree: `code.tests.fn` +1 and `code.tests.loc.total` +39. No
  unsafe or raw-pointer counter moved.

## Outside the workset

These were noted, not filed:

- `FramesReferenceBox::from_c` and `DeviceReferenceBox::from_c` are ffibox seams. Their contract is
  the types' invariant ("allocated by `av_hwdevice_ctx_alloc`", or by `av_hwframe_ctx_alloc` on
  such a device), which already pins `hwctx` to the backend of the type.
- The test fixtures still build contexts by hand over private structs (`FFHWDeviceContext`,
  `HWContextType`). That is the only way to get any device in a build without backends.
- `Cargo.lock`'s ffibox entry is rewritten by every build, from the `[patch]` path, and was not
  committed.
