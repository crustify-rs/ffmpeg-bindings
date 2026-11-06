# Wavefront: `AVHWDeviceContext.free` recorded as always set together with `user_opaque`

- Type: `AVHWDeviceContext` (`libavutil/hwcontext.h`). Its field findings
  were submitted by translation batch 0 of hwcontext wave 1 (`bfce8a115b`).
- Status: resubmitted. `submission.json` here is the corrected set, applied
  with `query types --name AVHWDeviceContext --update`. The submission keeps
  the translator's `_comment_agent`, corrects its backend sentence, and
  appends a review note. Only the `free` field changes.

## What was wrong

The `free` note said the field is "NULL unless the caller sets it before
av_hwdevice_ctx_init or a backend's device_create/device_derive sets it
together with user_opaque (vaapi, qsv, dxva2, vdpau, drm, opencl, d3d12va)".
The type comment repeated this as "vaapi/qsv/dxva2/vdpau backends store a priv
in user_opaque".

Four backends set `free` alone and leave `user_opaque` NULL:

| backend | site | function |
|---|---|---|
| DRM | `hwcontext_drm.c:76` | `drm_device_create` |
| OpenCL | `hwcontext_opencl.c:630` | `opencl_device_create_internal`, called from `opencl_device_create` and `opencl_device_derive` |
| D3D12VA | `hwcontext_d3d12va.c:721` | `d3d12va_device_create` |
| Vulkan | `hwcontext_vulkan.c:1937` | `vulkan_device_create_internal`, called from `vulkan_device_create` and `vulkan_device_derive` |

The note left Vulkan out entirely. Only VAAPI (`:1728` create, `:2064`
derive), QSV (`:2491` derive, `:2515` create), DXVA2 (`:544`) and VDPAU
(`:483`) set both fields.

This matters for anyone designing an accessor from the record. A wrapper
that takes "both or neither" at its word could decide whether the pair is
foreign by testing `user_opaque` alone. It would then overwrite a DRM, OpenCL,
D3D12VA or Vulkan context's `free` and leak or skip that backend's teardown.
The landed Rust hooks are not affected. `DeviceContextMut::hooks_mut` claims
the pair only when `free` is NULL and `user_opaque` is NULL. The review added
`unit_tests::device_context::a_backend_free_alone_is_neither_read_nor_replaced`
to pin that behaviour.

## What changed

The `free` note now names which backends set both fields and which set `free`
alone, with their functions. It also says that a wrapper claiming the pair must
find both fields unset. The other field records were checked against
`hwcontext.c` and are right: `type` (read-only), `av_class` (static
`hwdevice_ctx_class`), `hwctx` (owned, `av_mallocz` at the backend's size,
never replaced) and `user_opaque` (a cookie C never reads).
