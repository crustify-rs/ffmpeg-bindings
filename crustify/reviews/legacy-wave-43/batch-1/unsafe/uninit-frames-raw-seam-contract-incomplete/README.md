# Unsafe contract: `UninitFrames::as_mut_ptr` leaves out what the safe `init` relies on

- Items: `av_hwframe_ctx_alloc` and `av_hwframe_ctx_init`
  (`DeviceReferenceRef::alloc_frames` -> `UninitFrames`, the raw seam
  `UninitFrames::as_mut_ptr`, the safe `UninitFrames::init`), homed in
  `crustify/rust/ffmpeg/libavutil/hwcontext.rs` by translation batch 1
  (`dc1ebb6900`).
- Affected commit: the wave tip `bfce8a115b`.
- Status: contract fixed (documentation only). No reproducer: these builds
  compile no backend, and the test backends' `frames_init` ignores their
  `hwctx`. The hazard lives in real backends' code.

## Finding

`UninitFrames::init` is a safe call to `av_hwframe_ctx_init`. Safe code can
only reach it with the context as `av_hwframe_ctx_alloc` left it. Its format
is then `AV_PIX_FMT_NONE`, which C refuses with `AVERROR(ENOSYS)` before any
backend code runs (`hwcontext.c` validates `format` against `pix_fmts`
first). So `init` stays sound only if the raw writes through `as_mut_ptr`
respect a contract, and that contract was incomplete in two places and
wrong in a third:

1. **`hwctx` contents.** The seam asked writers to keep the `hwctx`
   *pointer*, but said nothing about what goes into the backend's struct.
   Backends dereference caller-provided fields there in `frames_init`. For
   example, `hwcontext_d3d11va.c:311-313` calls `ID3D11Texture2D_GetDesc`
   on a non-NULL `texture`. Their `frames_uninit` releases what they find
   whether or not the context was initialised (`hwcontext_d3d11va.c:134-135`
   `ID3D11Texture2D_Release`), and `hwframe_ctx_free` runs it for every
   context (`hwcontext.c:249-250`). A garbage `texture` is therefore UB in
   the safe `init` or in the drop.
2. **`free` and `user_opaque`.** `hwframe_ctx_free` calls `ctx->free(ctx)`
   at the last release (`hwcontext.c:252-253`). The seam did not mention
   either field.
3. **`pool` ownership was misstated.** The seam said a caller's pool is
   one "which the context then holds a reference to until it is released".
   C releases only the pool the backend made: `hwframe_ctx_free`
   uninits `pool_internal` (`hwcontext.c:246-247`), and init adopts it as
   `pool` only when the caller set none (`:373-374`). A pool the caller sets
   is never uninit by C. Against the header's "freed strictly before this
   struct's free() callback", it leaks unless the caller's own `free`
   uninits it.

## Fix

`UninitFrames::as_mut_ptr`'s docs now require the following:

- the backend's `hwctx` struct holds only what its `hwcontext_<api>.h`
  allows, with the reason (init uses it, uninit releases it either way);
- `free` is NULL or a function C may call with the context, and
  `user_opaque` as written, at its last release;
- `pool` is NULL or a usable pool, as before.

The docs also state that C never releases a pool the caller sets, so the
caller's `free` must, and that the safe `init` relies on all of this.
`init` stays safe: every new obligation falls on raw writes, which need
`unsafe`.
