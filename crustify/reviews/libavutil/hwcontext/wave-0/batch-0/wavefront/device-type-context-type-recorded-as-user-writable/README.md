# Wavefront: `AVHWDeviceType` record calls `AVHWDeviceContext.type` user-writable

- Type: `AVHWDeviceType` (`libavutil/hwcontext.h`). Its findings were
  submitted by translation batch 0 of hwcontext wave 0 (`b681e8e946`).
- Status: resubmitted. `submission.json` here is the corrected set,
  applied with
  `query types --name AVHWDeviceType --file libavutil/hwcontext.h --update`.

## What was wrong

The type has no fields, so its `_comment_agent` is its whole finding. The
note said the value is "Stored in public fields users may write
(AVHWDeviceContext.type, set by av_hwdevice_ctx_alloc from hw_table and
documented as never changed afterwards; libavcodec
AVCodecHWConfig.device_type in static codec tables)".

That contradicts itself, and the first half is wrong. Here is the
evidence:

- `hwcontext.h:72-73` says of `type`: "This field is set when this struct
  is allocated and never changed afterwards."
- C dispatches on it and casts `hwctx` by it. The `device_derive` switches
  are at `hwcontext_cuda.c:854`, `hwcontext_vulkan.c:2156` and
  `hwcontext_opencl.c:1222`, and `libavcodec/ohdec.c:151-152` does the
  same. `av_hwdevice_ctx_create_derived_opts` (`hwcontext.c:662-671`)
  returns the ancestor whose `p.type` matches. A user write would
  therefore make C read one backend's `hwctx` as another's.
- `AVCodecHWConfig.device_type` is reached only through
  `const AVCodecHWConfig *`, from static const tables.

Hwcontext wave 1 schedules `AVHWDeviceContext` with a `type` field anchor,
and its translator reads this record to choose accessors. A
"users may write" finding points straight at an unsound safe setter.

## What changed

The note now records:

- `type` as read-only, with the dispatch sites above;
- the real reason the value set is open: C accepts any input value, and a
  newer runtime `libavutil` returns enumerators this build does not name;
- `av_hwdevice_find_type_by_name` as case-sensitive (`strcmp`,
  `hwcontext.c:114`), although the header says case-insensitive;
- the NULL name passed to `%s` in libavcodec and libavfilter.

The rest is unchanged: the enumerators, the `c_uint` backing, and the
by-value consumers and how they accept unknown values.

The `AVHWFrameTransferDirection` record was checked against the source
and is right. It is an input only, to `av_hwframe_transfer_get_formats`
and the backends' `transfer_get_formats`. Its C callers pass FROM
(`hwcontext.c:420`, `vf_hwdownload.c:88`) or TO (`ffplay_renderer.c:621`).
No struct stores it and nothing returns it.
