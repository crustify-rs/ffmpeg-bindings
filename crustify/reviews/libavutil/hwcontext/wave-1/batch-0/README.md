# Review: libavutil/hwcontext wave 1, batch 0 (the wave's types)

- Items: `AVHWDeviceContext` and `AVHWFramesConstraints`
  (`libavutil/hwcontext.h`), homed in
  `crustify/rust/ffmpeg/libavutil/hwcontext.rs`. Translation batch 0 wrapped
  both (`b9dbecd340`):
  - `DeviceContext{,Ref,Mut}`, lent by `UninitDevice::context`/`context_mut`
    and `DeviceReferenceRef::context`. It has the getters `device_type`,
    `class`, `has_free_callback`, `user_data` and the unsafe `hwctx`; the
    unsafe `into_hwctx`; and Rust hooks over `free`/`user_opaque`
    (`set`/`take_free_callback`, `set`/`take_user_data`, the `release_hooks`
    trampoline). The type batch also made `UninitDevice::init` unsafe and
    `UninitDevice::from_c`/`into_c` public.
  - `FramesConstraints{,Ref,Mut}` and `FramesConstraintsBox = CBox<_,
    FramesConstraintsFree>`. The policy pairs `av_hwframe_constraints_free`
    with a `CNew` that copies C's first step. The box is built by
    `FramesConstraints::new` and by `DeviceReferenceRef::frames_constraints` /
    `frames_constraints_for_config` (`av_hwdevice_get_hwframe_constraints`).
    It has list and size getters, setters and takers, and is `Send + Sync`.
- Reviewed commit: the wave tip `b9dbecd340`.
- Wave 1 is not the sub-campaign's last wave, because `schedule.json` lists
  waves 0-2 for `hwcontext`. This review emits no reproducer, so BSan did not
  run.
- Reports:
  - `wavefront/device-context-releaser-unrecorded`: `hwdevice_ctx_free`
    recorded as the context's dropper.
  - `wavefront/device-context-free-recorded-as-set-with-user-opaque`:
    resubmitted.
  - `misc/device-context-hook-refusal-docs-name-both-fields`: docs fixed,
    regression unit test added.

## Verdicts by step

| step | finding |
|---|---|
| lifecycle | None missing in the Rust tree. `AVHWFramesConstraints` has its constructor (`av_hwdevice_get_hwframe_constraints`, both forms), its dropper (`av_hwframe_constraints_free` as `CDrop`) and a Rust `CNew` that matches C's start (`av_mallocz`, max sizes `INT_MAX`). C has no copy, so the box has no `Clone`. `AVHWDeviceContext` is only ever reached through an `AVBufferRef`. Its owners are batch 1's `UninitDevice`/`DeviceReferenceBox` (`ContextUnref` = `av_buffer_unref`/`av_buffer_ref`), and the context lends handles only. Both are right: no by-value `Foo` can be built, since neither type is `CZeroable`, and no `CVal` exists. |
| wavefront | Two analysis defects, both resubmitted. (1) The context's only releaser, the static `hwdevice_ctx_free` (the `AVBuffer` free callback), was unsubmitted (`lifetime: null`, both arguments pending). It now records the dropper role on `data`. `--lifetime-for` cannot surface it because `data` is declared `uint8_t *`. (2) The `free` note said every backend that sets `free` also sets `user_opaque`. DRM, OpenCL, D3D12VA and Vulkan set `free` alone. `AVHWFramesConstraints`' records (lists owned, `NONE`-terminated, nullable; sizes plain) and its two lifecycle roles are right. |
| unsafe boundary | No defect. `scan-unsafe --sites` lists two `unsafe_fns_pub_smell` sites in the workset, `DeviceContextRef::hwctx` (`:1982`) and `DeviceContextMut::into_hwctx` (`:2062`). Both must stay unsafe: `hwctx` is a `void *` whose type only the device type implies, and the backend headers (`hwcontext_<api>.h`) are outside the campaign. Their `# Safety` texts state the type obligation, and a no-writers obligation for reads, or the device header's content rules for writes. Both are falsifiable and complete. `UninitDevice::init` (`:1205`) stays unsafe for a real reason: d3d11va's `device_init` calls through `hwctx->device` unchecked (`hwcontext_d3d11va.c`), and Vulkan's goes through its loader. `UninitDevice::from_c`/`into_c` are adoption seams. `release_hooks`' four `raw_ptr_derefs_outside_impl` are the trampoline's field projections, each documented. Counters are unchanged by the fix. |
| ub_safe / ub_unsafe | No defect. The free callback is higher-ranked (`for<'c>`), so its handle cannot escape (trybuild). User data borrows block replacement (trybuild). The hooks are recognised by one static fn-pointer value, foreign pairs are never read, and a callback that takes, replaces or re-installs hooks leaves no dangling box. Miri checks this on Rust storage (`hooks_on_rust_storage_without_c`, clean under Stacked Borrows), and ASan checks it through C. Release on another thread needs only the `Send` (+ `Sync`) bounds that the hooks already carry (TSan, `ub_unsafe` scoped threads). The constraint lists are terminated by every writer. `replace_list` frees through `av_free` with the counted length. `set_list` keeps the old list on ENOMEM. The `ub_unsafe` suites stay inside the `# Safety` contracts of `from_c`, `init`, `hwctx` and `into_hwctx`. |
| equiv | No divergence. Getters match field reads on C-built contexts. `av_hwdevice_ctx_init` results and release order match (uninit, then `free`, then frees). Constraint queries match over a backend × config × failure matrix, and edit sequences are compared after each step. C frees Rust-set lists. `has_free_callback` differs from `ctx->free != NULL` on purpose, when only user data is installed. The misc fix now documents that. |
| internal | No defect. `list` stops at the first `NONE`. `replace_list` hands back exactly the counted elements of the old block. `release_empty_hooks` frees the box only once both slots are empty. The new test pins the case of a backend `free` set alone. |
| conventions | No deviation. Each type has one `Wraps:` anchor. Every accessor has a `Field:` anchor; repeated anchors follow the getter/setter/taker precedent. `FramesConstraintsFree` is `Handwritten` with its `Wraps:` anchors. Getters are on `Ref` and setters on `Mut`, and all layout access goes through `addr_of!`/`addr_of_mut!`. Tests are in the prescribed homes. `cargo doc -D warnings` and clippy `--all-targets -D warnings` are clean. |
| misc | One documentation defect, fixed. The setters' docs read as refusing only when both `free` and `user_opaque` are foreign, but either one alone refuses them. `has_free_callback` claimed "something runs" while ignoring user data. |
| coverage | Every line in the workset's range runs, except the trampoline's unreachable NULL-handle branch (`:1928`). The C routines the workset calls run fully: `av_hwdevice_get_hwframe_constraints`, `av_hwframe_constraints_free`, `av_hwdevice_ctx_init`, `av_hwdevice_hwconfig_alloc`. Details in `coverage.json`. |

## Validation

These runs used the fixed tree. The fix changes rustdoc, one unit test and
the wavefront store. The base tree was validated the same way before any
change, with the same results minus the new test.

- `cargo check --workspace`, `cargo clippy --workspace --all-targets -- -D
  warnings`, `cargo test --workspace` (plain), and `cargo doc -p ffmpeg
  --no-deps` with `-D warnings`: all clean. lib 948, equiv 708, ub_safe 808
  (with trybuild), ub_unsafe 172.
- The whole `ffmpeg` crate under ASan/UBSan and under TSan: lib 948, equiv
  708, ub_safe 806 (trybuild skipped under `-Zbuild-std`), ub_unsafe 172.
  No sanitizer report and no `runtime error:`.
- Miri on `hwcontext::unit_tests::device_context::hooks_on_rust_storage_without_c`:
  passes. The workset's other unit tests call C.
- BSan: not run, because this review has no reproducer and this is not the
  last wave.
- `crustify scan-unsafe`: identical counters on the base and fixed trees.
