# Wavefront: `AVHWDeviceContext`'s releaser was never recorded

- Type: `AVHWDeviceContext` (`libavutil/hwcontext.h`), wrapped by
  translation batch 0 of hwcontext wave 1 (`bfce8a115b`).
- Symbol: `hwdevice_ctx_free` (`libavutil/hwcontext.c:157`, static).
- Status: recorded. `submission.json` here is the finding, applied with
  `query symbols --name hwdevice_ctx_free --update`.

## What was missing

`query symbols --lifetime-for AVHWDeviceContext` returns no dropper, disposer,
cloner or constructor. The translator noted this in the type's
`_comment_agent` and explained it: "No symbol takes or returns
AVHWDeviceContext* directly (only AVBufferRef* / uint8_t*) ... the lifecycle
lives on AVBufferRef". That reasoning holds for the constructor.
`av_hwdevice_ctx_alloc` returns an `AVBufferRef`, and it is recorded as that
type's constructor.

The releaser is a different case. The context is freed by exactly one routine,
`hwdevice_ctx_free(void *opaque, uint8_t *data)`. `av_hwdevice_ctx_alloc`
installs it as the `AVBuffer`'s free callback (`hwcontext.c:204-206`), and
`av_buffer_unref` calls it with the last reference. Before this review its
record was still unsubmitted:

```json
"lifetime": null,
"_analysis": { "submitted": false,
               "pending": ["ptr_args.opaque.ptr", "ptr_args.data.ptr"] }
```

So the store had no record that anything releases the context. It also had
nothing on what that release does in order: the backend's `device_uninit`,
then `ctx->free` (Rust's `release_hooks` trampoline when the hooks are set),
then the `source_device` unref, then `av_freep` of `hwctx` and of the context.

## What changed

`hwdevice_ctx_free` now records:

- `data` as an owned, non-NULL pointer to the `FFHWDeviceContext` (public
  struct first), with the release order above;
- `opaque` as the unused NULL cookie;
- `lifetime: {for: "data", is_dropper: true}`.

## Remaining tool limitation

`--lifetime-for AVHWDeviceContext` is still empty after the update. The query
matches roles by an argument's declared type, and `data` is declared
`uint8_t *`. The same holds for every `AVBuffer` free callback that releases a
typed payload, such as `hwframe_ctx_free` for `AVHWFramesContext` in wave 2.
Such a type's releaser is found by reading its allocator, not by this query.
No wavefront patch is filed: the declared type is what the C says, and the
role is now on record for anyone who queries the symbol.
