# Wavefront: `av_calloc` and `av_memdup` miss their single-object paths

- Store: `crustify/wavefront/ownership-store.json`
- Affected commit: `77473b8574`
- Records: `av_calloc` (`ptr_ret`), `av_memdup` (`ptr_args[0]` `p`, `ptr_ret`)

## Finding

The schema defines `scalar` as "is there any execution path where this pointer
references a SINGLE pointee?". The previous run submitted `scalar: null` (array
only) for these three slots, but FFmpeg's own callers use both routines on one
object:

| record | call site | single object |
|---|---|---|
| `av_calloc` return | `libavfilter/af_arnndn.c:201` `ret = av_calloc(1, sizeof(RNNModel));` | one `RNNModel` |
| `av_calloc` return | `libavcodec/vvc/thread.c:755` `ft = av_calloc(1, sizeof(*fc->ft));` | one `VVCFrameThread` |
| `av_memdup` `p` and return | `libavcodec/dnxhdenc.c:512` `ctx->thread[i] = av_memdup(ctx, sizeof(DNXHDEncContext));` | one encoder context, copied |
| `av_memdup` `p` and return | `libavcodec/mpegvideo.c:105` `s->thread_context[i] = av_memdup(s, slice_size);` | one slice context, copied |

`av_malloc` and `av_mallocz` already carried both `scalar` and `array`.
`av_malloc_array` keeps `scalar: null`: its `nmemb == 1` callers
(`libavformat/mov.c:5898-5909`) still treat the result as an array with a
stored count.

## Consequence

The array-only `av_memdup` record is what let the singleton clone strategy go
missing (see `lifecycle/void`): the findings described `av_memdup` as an array
cloner only.

## Correction

Resubmitted through `wavefront ... query symbols --name <sym> --file
libavutil/mem.c --update`, with `scalar: {by_val: true}` next to the existing
`array: {by_val: true}` and the call sites above added to each `note`. All
other fields are unchanged; `query symbols --lifetime-for void` still lists the
same droppers, disposers, cloners and constructors.
