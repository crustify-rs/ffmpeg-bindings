# Misc: the hook setters' docs read as refusing only when both `free` and `user_opaque` are foreign

- Item: `AVHWDeviceContext`, wrapped as `hwcontext::DeviceContext` by
  translation batch 0 of hwcontext wave 1 (`b9dbecd340`), in
  `crustify/rust/ffmpeg/libavutil/hwcontext.rs`.
- Status: fixed. Only rustdoc changed, plus one regression unit test.

## What was wrong

`DeviceContextMut::set_free_callback` said: "Refused, giving `f` back, when
the context's `free` and `user_opaque` are someone else's: a backend's device
creation sets them." `set_user_data` repeated "`free` and `user_opaque` are
someone else's". The code refuses when either field is set and is not Rust's,
and it has to: DRM, OpenCL, D3D12VA and Vulkan set `free` alone (see
`wavefront/device-context-free-recorded-as-set-with-user-opaque`). Read
literally, the docs promise that a context with only a backend `free` accepts
the hooks. A caller who adopts such a context through `UninitDevice::from_c`
would then expect hooks that are never installed.

`DeviceContextRef::has_free_callback` said it reports "whether something runs
when the context's last reference is released". It returns `false` when only
user data is set. In that case the user data's `Drop` does run at the last
release, through the trampoline in `free`.

## What changed

- `set_free_callback` and `set_user_data` now say "`free` or `user_opaque` is
  someone else's, either one alone", and name the backends that set `free`
  only. The private `hooks_mut` doc says the same.
- `has_free_callback` now says it reports whether a free callback runs (one
  the setter installed, or the backend's), and that user data alone is not
  one, although it is dropped at the last release.
- New unit test
  `libavutil::hwcontext::unit_tests::device_context::a_backend_free_alone_is_neither_read_nor_replaced`.
  It gives a fake context a backend-style `free` with `user_opaque` NULL, as
  `drm_device_create` leaves it. It then checks that both setters refuse, that
  both takers find nothing, that `has_free_callback` is true, and that C runs
  the backend's callback exactly once at release. The existing
  `foreign_hooks_are_neither_read_nor_replaced` covered both fields set, and a
  cookie with no callback, but not `free` alone.
