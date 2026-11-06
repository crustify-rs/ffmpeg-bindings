# Misc: `DeviceType`'s docs call `AVHWDeviceContext.type` a field users may write

- Item: `AVHWDeviceType`, wrapped as `hwcontext::DeviceType` in
  `crustify/rust/ffmpeg/libavutil/hwcontext.rs` by translation batch 0 of
  hwcontext wave 0 (`b681e8e946`).
- Status: fixed (documentation only; no code or test changes).

## What was wrong

`DeviceType`'s doc comment justified the open newtype like this: "C stores
the value in public struct fields its users may write directly
(`AVHWDeviceContext.type`, libavcodec's `AVCodecHWConfig.device_type`)".

C says otherwise. `hwcontext.h:69-75`:

```c
    /**
     * This field identifies the underlying API used for hardware access.
     *
     * This field is set when this struct is allocated and never changed
     * afterwards.
     */
    enum AVHWDeviceType type;
```

`AVCodecHWConfig.device_type` is not user-writable either. It lives in
libavcodec's static codec tables, which `avcodec_get_hw_config` returns as
`const AVCodecHWConfig *`.

This matters for the next wave. Hwcontext wave 1 wraps `AVHWDeviceContext`
with a `type` field anchor. A translator who takes the wave-0 rationale at
its word could add a safe `set_type` setter, and that setter would be
unsound. C picks the layout of the context's `hwctx` by `type`:

- `device_derive` callbacks switch on the source's `type` and cast its
  `hwctx` (`hwcontext_cuda.c:854`, `hwcontext_vulkan.c:2156`,
  `hwcontext_opencl.c:1222`). `libavcodec/ohdec.c:151-152` does the same.
- `av_hwdevice_ctx_create_derived_opts` (`hwcontext.c:662-671`) hands back
  any ancestor whose `p.type` equals the requested type. The caller then
  reads that context's `hwctx` as the requested backend's struct.

If a user rewrote `type`, C would read one backend's `hwctx` as another
backend's struct. The campaign builds compile no backend
(`--disable-autodetect`), so no reproducer can run here. That is also why
no device context exists in this wave to write through.

The open newtype is still the right choice, and the other reasons in the
old text hold. C accepts every value as input: `av_hwdevice_get_type_name`
bounds-checks, and the `hw_table` lookups compare by equality. The value
set C hands out is also open. FFmpeg appends enumerators in minor releases
(`OHCODEC` came last), so a newer `libavutil.so.61` loaded at run time can
return a type this crate has no constant for. The return paths are
`av_hwdevice_find_type_by_name`, `av_hwdevice_iterate_types` and a
context's `type`. The old docs did not give that reason.

## What changed

- The rationale paragraph now gives the real reasons for an open set:
  newer runtime enumerators on C's return paths, and C accepting any
  input.
- A new paragraph states C's contract. `AVHWDeviceContext.type` is set at
  allocation and never changed afterwards, because backends cast `hwctx`
  by it, so it is a value to read and not to write.
  `AVCodecHWConfig.device_type` lives in read-only tables.
- The wavefront record had the same claim, and it was resubmitted:
  `../../wavefront/device-type-context-type-recorded-as-user-writable/`.

## Notes for hwcontext wave 1

- Expose `AVHWDeviceContext.type` through a getter only.
- `av_hwdevice_find_type_by_name` is documented as case-insensitive, but it
  uses `strcmp` (`hwcontext.c:114`), so `"CUDA"` finds nothing.
  `tests/equiv/hwcontext.rs` already pins C's actual behaviour. The wave-1
  wrapper's docs should describe what C does.
- `av_hwdevice_get_type_name` returns NULL for `NONE` and for unnamed
  values. libavcodec (`decode.c:1090-1091`, `ohdec.c:157`) and libavfilter
  (`vf_amf_common.c:221`) pass its result to `%s` unchecked. glibc prints
  `(null)`, but the C standard leaves this undefined.
