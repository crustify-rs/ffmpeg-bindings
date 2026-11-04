Subject: ffmpeg-bindings: hwcontext test fixtures' BackendDeviceContext impls read past a 4-byte hwctx

Hi Marius,

Wave 952 of `ffmpeg-bindings` made `DeviceContextRef::hwctx` and
`DeviceContextMut::into_hwctx` safe. They check the context's device type against
`T::DEVICE_TYPE`, and trust `T`'s `unsafe impl BackendDeviceContext` for the struct's layout. The
wrappers are sound, but the test fixtures' impls are not.

`tests/common/hwdevice.rs` implements the trait for its `FakeHwctx` (48+ bytes) as `VAAPI` and as
`DRM`. Its justification covers only the contexts that file makes. The same three test crates
include `tests/common/hwbackend.rs`, whose `alloc_device` makes `VAAPI` and `DRM` devices with a
4-byte `DeviceHwctx`. In those crates, safe code reads past that allocation:

```rust
let device = unsafe { DeviceReferenceBox::from_c(hwbackend::device(Kind::A)) }.unwrap();
let hw = device.as_ref().context().hwctx::<hwdevice::FakeHw>().unwrap(); // type matches
hw.value(); // 8-byte read at offset 16 of a 4-byte block
```

ASan/UBSan (`asan-ubsan` variant):

```text
ERROR: AddressSanitizer: heap-buffer-overflow … READ of size 8
… is located 12 bytes after 4-byte region
allocated by thread T0 here: … libavutil.so.61
```

No landed test makes this call, and only test code is affected. Still, it is a reachable false
`unsafe impl` that turns a type check into an out-of-bounds read.

The review's patch moves the fake backend to device types no other fixture uses (`VDPAU`,
`OPENCL`). It states, and asserts, that the builds compile no hardware backend, and adds a
`ub_unsafe` regression test. The standalone reproducer is available on request.

Found by [Crustify](https://github.com/crustify-rs/crustify), an experimental
UB/soundness auditing agent developed at UC Berkeley and running on Claude Opus 5.5, then
manually reviewed and independently reproduced.
