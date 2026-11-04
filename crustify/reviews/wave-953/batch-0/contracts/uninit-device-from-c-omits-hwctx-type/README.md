# `UninitDevice::from_c` does not require `hwctx` to match the context's `type`

- Objective: `contracts`
- Where: `UninitDevice::from_c`'s `# Safety`, `src/libavutil/libavutil/hwcontext.rs:1171-1177`
  at the wave tip `3ee33a6864f` (the contract predates wave 952; wave 952's commit
  `1b7312582ee` made the safe accessors rely on it).
- Reproducer: none. This is an ambiguous contract, not a demonstrated break. See below.
- Fixed: yes, in this review's Rust-tree commit (documentation).

## What is wrong

Since wave 952, `DeviceContextRef::hwctx` and `DeviceContextMut::into_hwctx` are safe. They type
`hwctx` by the context's `type` alone: they compare it with `T::DEVICE_TYPE`, then call
`T::ref_from_void_ptr` / `T::mut_from_void_ptr` on the pointer. Their `// SAFETY:` comments
(`:2286-2292`, `:2382-2385`) rely on "C allocated it for the backend of the context's type".

For contexts that `av_hwdevice_ctx_alloc` returns, that holds. `hwcontext.c:181-213` looks the
backend up by `type`, sizes `hwctx` from `hw_type->device_hwctx_size`, and stores the same `type`.
But `UninitDevice::from_c` lets a caller hand in a context that was "built exactly as it does".
Its contract lists what the context must be: one reference, uninitialised, `free`/`user_opaque`
unset or known. It never says that `type` must name the backend whose struct `hwctx` holds. A
caller who builds a context over one backend under another backend's type meets the letter of the
contract. Safe code can then reach type confusion, through any `BackendDeviceContext` for that
type. The test fixtures do build contexts this way (`fake_device_of(&FAKE_TYPE, <any type>)`).
Whether "exactly as it does" already requires the backend to be the one of `type` is left to the
reader. Since wave 952, that reading is what soundness depends on.

No reproducer: a break needs a caller that reads the contract the loose way, plus a
`BackendDeviceContext` implementation whose struct differs from the backend's. The fixture finding
beside this one ([`../fixture-device-impls-claim-every-vaapi-context/`](../fixture-device-impls-claim-every-vaapi-context/))
shows the same mismatch through `DeviceReferenceBox::from_c`, whose contract is the
`DeviceReference` invariant ("allocated by `av_hwdevice_ctx_alloc`").

## Fix

`from_c`'s `# Safety` now adds the obligation:

> Its `hwctx` must be NULL or the struct the backend of its `type` allocates there, as
> `av_hwdevice_ctx_alloc` sizes it from that backend: [`DeviceContextRef::hwctx`] and
> [`DeviceContextMut::into_hwctx`] type it by `type` alone, as the struct every
> [`BackendDeviceContext`] of that type names.

The fixtures' adoptions meet it, now that their fake backend's types are their own (previous
finding).

## Rerun

```bash
git show 3ee33a6864f:src/libavutil/libavutil/hwcontext.rs | sed -n 1168,1183p
sed -n 181,213p c/libavutil/hwcontext.c
```
