Subject: ffmpeg-bindings: UninitDevice::from_c's contract no longer covers what hwctx relies on

Hi Marius,

Wave 952 of `ffmpeg-bindings` made `DeviceContextRef::hwctx` and
`DeviceContextMut::into_hwctx` safe. They type the context's `hwctx` by its `type` alone, which
is sound for contexts `av_hwdevice_ctx_alloc` returns: `hwcontext.c` sizes `hwctx` from the
backend of the `type` it stores.

`UninitDevice::from_c` also accepts a context "built exactly as it does", and its `# Safety`
never says that `type` must name the backend whose struct `hwctx` holds:

```rust
/// `ptr` must be NULL or a reference the caller owns and hands over, to
/// a device context `av_hwdevice_ctx_alloc` allocated (or built exactly
/// as it does) that has no other reference and was not initialised, and
/// whose `free` and `user_opaque` are unset, a backend's, or this crate's.
pub unsafe fn from_c(ptr: *mut ffi::AVBufferRef) -> Option<Self>
```

A caller that builds a context over one backend under another's type, as the test fixtures do,
meets that wording. Safe code could then read its `hwctx` as the other backend's struct. No
reproducer is attached: this is ambiguous wording that soundness now depends on, not a
demonstrated break.

The review's patch adds the obligation: `hwctx` must be NULL or the struct the backend of `type`
allocates, since the accessors type it by `type` alone.

Found by [Crustify](https://github.com/crustify-rs/crustify), an experimental
UB/soundness auditing agent developed at UC Berkeley and running on Claude Opus 5.5, then
manually reviewed and independently reproduced.
