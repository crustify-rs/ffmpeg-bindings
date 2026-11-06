# The test fixtures' `BackendDeviceContext` impls claim a type the frames fixture also uses

- Objective: `contracts`
- Where: `tests/common/hwdevice.rs:129-138` (`unsafe impl BackendDeviceContext for FakeHw`,
  `DEVICE_TYPE = VAAPI`) and `:223-227` (`FakeDrmHw`, `DRM`), against
  `tests/common/hwbackend.rs:897-928` (`alloc_device`, which makes `VAAPI` and `DRM` devices). At
  the wave tip `3ee33a6864f` (wave 952 batch 0, commit `1b7312582ee`).
- Reproducer: this directory (`cargo run`, under ASan as below).
- Fixed: yes, in this review's Rust-tree commit, with a UB regression test in `tests/ub_unsafe`.

## What is wrong

Wave 952 made `DeviceContextRef::hwctx` and `DeviceContextMut::into_hwctx` safe. They now compare
the context's `type` with `T::DEVICE_TYPE`, and they trust `T`'s
`unsafe impl BackendDeviceContext` for the layout of whatever `hwctx` points to. Such an impl
asserts that `T::C` is "the struct C allocates `hwctx` for … in every context of that type".

The fixtures implement the trait for `FakeHwctx` twice: `FakeHw` for `VAAPI` and `FakeDrmHw` for
`DRM`. Their `// SAFETY:` comment justifies this from the contexts `fake_device` and
`fake_device_of` make. That is not every context of those types. `tests/common/hwbackend.rs` is
included by the same three test crates: the lib's unit tests (`hwcontext.rs:4074`, `:4090`),
`tests/equiv` and `tests/ub_unsafe`. Its `alloc_device` makes `VAAPI` (kind A) and `DRM` (kind B)
devices whose `hwctx` is its own `DeviceHwctx`, a single `c_int` of 4 bytes. `FakeHwctx` is at
least 48 bytes, with `value: i64` at offset 16.

As a result, in those crates, safe code that holds a test backend device can call
`device.as_ref().context().hwctx::<FakeHw>()`. The type check passes (`VAAPI == VAAPI`), and every
getter of the handle then reads past the 4-byte allocation. The impls' claim is false, and the
justification does not hold on every path that reaches the block.

Before wave 952, each call site vouched for the type in its own `unsafe` block, and no landed test
made this call. The defect is that the unsafe impls make the call safe.

## Evidence

`src/main.rs` makes a kind-A device exactly as `tests/ub_unsafe/hwcontext.rs`'s `device` does,
with the fixtures included by `#[path]`. The one `unsafe` call is the fixtures' own, within
`DeviceReferenceBox::from_c`'s contract. Everything after it is safe code: `hwctx::<FakeHw>()`,
then `value()`.

```bash
cd crustify/reviews/legacy-wave-953/batch-0/contracts/fixture-device-impls-claim-every-vaapi-context
B=/target/ffmpeg-bindings/crustify/builds
LD_LIBRARY_PATH=$B/asan-ubsan/libavutil CRUSTIFY_BUILD_VARIANT=asan-ubsan \
  CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS=-Zsanitizer=address \
  cargo +nightly-2026-10-08 run -Zbuild-std --target x86_64-unknown-linux-gnu
```

The command is run from inside the repository, so `.cargo/config.toml` puts the `-sys` crates in
campaign mode. `LD_LIBRARY_PATH` is needed because the rpath is the safe crate's build script's,
which applies only to that crate's own binaries. On the wave tip it aborts with:

```text
==802111==ERROR: AddressSanitizer: heap-buffer-overflow on address 0x7311f9de0050 at pc 0x59d8daf97561 bp 0x7ffc789de1c0 sp 0x7ffc789de1b8
READ of size 8 at 0x7311f9de0050 thread T0
    #0 0x59d8daf97560  (…/fixture-device-impls-claim-every-vaapi-context+0x23f560)
    …
0x7311f9de0050 is located 12 bytes after 4-byte region [0x7311f9de0040,0x7311f9de0044)
allocated by thread T0 here:
    #0 0x59d8daf62577  (…/fixture-device-impls-claim-every-vaapi-context+…)
    #1 0x7681fafc1a23  (/target/ffmpeg-bindings/crustify/builds/asan-ubsan/libavutil/libavutil.so.61+0x2bda23)
SUMMARY: AddressSanitizer: heap-buffer-overflow
```

That is an 8-byte read at offset 16 (`FakeHwctx.value`) of the 4-byte `DeviceHwctx` that
`av_mallocz` returned in `alloc_device`. Without ASan, the program prints what it read and exits 1.
On the fixed tree it prints `not lent: FakeHw does not claim this device's type` and exits 0.

## Fix

- `hwdevice.rs`'s fake backend now claims device types that no other fixture gives a device:
  `FakeHw` is `VDPAU` (also `FAKE_TYPE.type_` and `fake_device`), and `FakeDrmHw` is renamed
  `FakeOpenClHw`, for `OPENCL`. `BARE_TYPE` contexts, which have no `hwctx`, keep the types
  their tests chose.
- Each `// SAFETY:` comment now states what makes the claim hold for every context of the type:
  the builds compile no hardware backend, and the other fixture's devices are of other types.
  `fake_device_of` and `alloc_device` assert the first (`DeviceType::supported()` is empty), so a
  build that gains a backend fails the fixtures rather than confusing types.
- The tests that built `FAKE_TYPE` contexts as `VAAPI`/`DRM`, or asserted those types, follow:
  the lib's `builder_getters_read_the_allocated_fields` and
  `configurations_of_another_device_type_are_refused`; `ub_unsafe`'s `adopted_contexts_release_once`,
  `bare_contexts_read_none` and `another_device_types_struct_reads_none`; and `equiv`'s
  `getters_read_what_c_stores`, `c_device` and `rust_device`.
- Regression test: `tests/ub_unsafe/hwcontext.rs`,
  `frames_context::…::the_fake_device_structs_are_not_lent_over_test_backend_devices`. It asks
  both kinds' devices for `FakeHw` and `FakeOpenClHw`, and expects `None`. It passes under
  ASan/UBSan.

## Rerun

```bash
git show 3ee33a6864f:tests/common/hwdevice.rs | sed -n 126,138p
git show 3ee33a6864f:tests/common/hwbackend.rs | sed -n 155,163p   # DeviceHwctx: one c_int
cargo test -p ffmpeg-bindings --test ub_unsafe the_fake_device_structs_are_not_lent
```
