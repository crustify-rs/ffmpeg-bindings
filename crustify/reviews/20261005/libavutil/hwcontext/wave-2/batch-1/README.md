# Review: libavutil/hwcontext wave 2, batch 1 (the wave's symbols)

- Items (2): `av_hwdevice_get_hwframe_constraints` and
  `av_hwframe_constraints_free` (`libavutil/hwcontext.c`), homed in
  `crustify/rust/ffmpeg/libavutil/hwcontext.rs`. Translation batch 1 of
  wave 2 added no code (`3897b00e38`). The wave-1 type batch had already
  bound both routines when it wrapped `AVHWFramesConstraints`
  (`b9dbecd340`), and the wave-1 type review reviewed them as that type's
  lifecycle:
  - `DeviceReferenceRef::frames_constraints` (NULL configuration) and
    `frames_constraints_for_config(&HwConfig)`. The second refuses a
    configuration of another device type. Both return
    `Option<FramesConstraintsBox>`.
  - `FramesConstraintsFree`, the `CDrop` (with a Rust `CNew`) behind
    `FramesConstraintsBox = CBox<FramesConstraints, FramesConstraintsFree>`.
- Reviewed commit: the wave tip `126a47005b`.
- Wave 2 is the sub-campaign's last wave, since `schedule.json` lists
  waves 0-2 for `hwcontext`. This wave has two review batches. Under the
  orchestrator's rule for that case (2026-10-12), batch 0 (the types) runs
  the workspace-wide BSan gate, so this batch ran BSan only on the one test
  it added.
- Reports: none. No defect was found in any step. This review adds one
  equivalence test (see "equiv" below).

## Verdicts by step

| step | finding |
|---|---|
| lifecycle | Not applicable: a symbol workset. Both routines are the lifecycle of `AVHWFramesConstraints`. `av_hwdevice_get_hwframe_constraints` is the constructor of a caller-owned structure, and the result is adopted with `FramesConstraintsBox::from_c` (NULL gives `None`). `av_hwframe_constraints_free` is the box's `CDrop`, and nothing else exposes it, as the symbol route requires for lifecycle routines a type run already emitted. C has no copy routine, so the box has no `Clone`. |
| wavefront | No analysis defect. `ref` is borrowed for the call, non-null and read-only. Every backend's `frames_get_constraints` reads the context and writes only the constraints (CUDA, D3D11VA, D3D12VA, AMF, VideoToolbox, VAAPI, VDPAU, QSV, OpenCL, Vulkan). `hwconfig` is nullable and read-only. The return is owned, nullable, and its lists are `av_malloc`. The constructor role is `{alloc: true, init: false}`. The free takes an owned `**` slot, as a dropper. |
| unsafe boundary | No defect. Both wrappers are safe methods on `DeviceReferenceRef` taking `&self`. They take a typed `&HwConfig` and return a typed owner. Each FFI call sits in a documented two-line block. `crustify scan-unsafe --sites all` reports no smell counter in the workset's lines (`hwcontext.rs` 2355-2740). The only public `unsafe fn` there is `CDrop::c_drop` (`unsafe_fns_pub_seam`, ffibox's trait seam). The only raw pointers there are `define_ctype!`'s `pub(crate)` seams. `HwConfig::as_ptr`/`as_mut_ptr` are raw seams of a wave-1 item: the backend headers (`hwcontext_<api>.h`) are outside the campaign. |
| ub_safe / ub_unsafe | No UB found. `DeviceReferenceRef` only exists for initialised contexts, which `UninitDevice` withholds until `init`. It is `!Send`/`!Sync`, so no safe path runs two queries on one device at once. A configuration of another device type is refused before C runs. One of the same type has that backend's `device_hwconfig_size`, because there is one `hw_table` entry per type. A zeroed configuration (VA-API `config_id` 0) is an invalid-handle error in the driver, not memory UB. Every backend allocates its lists with `av_malloc_array`/`av_reallocp_array`, independent of the device, so a result may outlive its device (`ub_unsafe` `constraints_outlive_the_device`). `AVPixelFormat` is a bindgen newtype, so any `int` a backend writes is a valid `PixelFormat`. The workset adds no public `unsafe` API, so there is no `ub_unsafe` obligation of its own. |
| equiv | No divergence. The only difference from C is the documented refusal of a configuration of another device type, where C would hand the backend a struct it misreads. The suite compared single queries over a backend × list × status × configuration matrix, each on a fresh device. It did not compare multi-call sequences on one device. This review added `equiv/hwcontext.rs::frames_constraints::query_sequences_on_one_device_match_c`. It runs twelve steps on one C device and one Rust device: plain queries; queries under one configuration that is rewritten between them (zeroed, CUDA, VULKAN, NONE, giving an empty list); and backend failures and positive statuses in between. Every result stays live while later ones are made. The first result is then edited while the others stay as they were. The configuration and the device are released before the results, and the results are released middle-out. After every step it compares results, views of every live result, the backend's event counts, and the sizes the backend found at entry. |
| internal | No defect. `frames_constraints*` have no private helpers. The refusal compares `HwConfig::device_type` with `kind()`. `FramesConstraintsFree::c_drop` passes a local slot and never NULL. |
| conventions | No deviation. `av_hwdevice_get_hwframe_constraints` has two `Wraps:` anchors, one per variant (NULL configuration, typed configuration). `av_hwframe_constraints_free` has one, on the `Handwritten` policy. The methods are on the device reference's shared handle. The names drop the C prefixes. The new test follows the module's `c_*`/`rust_*` pairing, is rustfmt-clean, and passes clippy `--all-targets -D warnings`. |
| misc | None. |
| coverage | The workset's Rust lines all run. In C, both functions run every line. The one branch never taken is `av_hwframe_constraints_free`'s `*constraints == NULL`, which no Rust path reaches, because the policy always passes a live structure. Details in `coverage.json`. |

## Validation

These runs used the tree this review lands (the wave tip plus the new
test).

- `cargo check --workspace`, `cargo clippy --workspace --all-targets -- -D
  warnings`, `cargo test --workspace` (plain: lib 966, equiv 717, ub_safe
  808 with trybuild, ub_unsafe 182, doctests) and `RUSTDOCFLAGS=-D
  warnings cargo doc -p ffmpeg --no-deps`: all clean.
- The whole `ffmpeg` crate under ASan/UBSan and under TSan: lib 966, equiv
  717, ub_safe 806 (trybuild skipped under `-Zbuild-std`), ub_unsafe 182.
  No sanitizer report and no `runtime error:`.
- BSan: the new test alone (`equiv --exact
  hwcontext::frames_constraints::query_sequences_on_one_device_match_c
  --test-threads=1`) passes. This batch emitted no reproducer, and the
  workspace gate belongs to batch 0.
- Miri: not applicable. Both wrappers and every test of them call C.
- `crustify scan-unsafe`: counters identical before and after the change
  (24 undocumented `unsafe` sites crate-wide, none in the workset or the
  new test).
