# Wavefront: `av_hash_alloc` is not recorded as `AVHashContext`'s constructor

- Store: `crustify/wavefront/ownership-store.json`
- Affected commit: `cc91f6866d` (hash/HMAC batch); wave tip `c45fd5b3d1`
- Items: `AVHashContext` (lifecycle), symbol `av_hash_alloc` (`libavutil/hash.c`)

## Finding

`wavefront query types --name AVHashContext --lifecycle-ops` lists only

```
av_hash_freep
av_hash_init
```

so the type's allocating constructor is missing. The batch submitted
`av_hash_alloc`'s two pointer arguments (the `AVHashContext **ctx` slot as
`scalar.by_ref.owned: true`, and `name`) but no `lifetime` block at all:

```json
{"name": "av_hash_alloc", "defined_in": "libavutil/hash.c", "ptr_args": [ ... ]}
```

The C body is an allocating constructor into that slot:

```c
int av_hash_alloc(AVHashContext **ctx, const char *name)
{
    ...
    *ctx = NULL;
    ...
    res = av_mallocz(sizeof(*res));
    if (!res) return AVERROR(ENOMEM);
    res->type = i;
    switch (i) { case MD5: res->ctx = av_md5_alloc(); break; ... }
    if (i != ADLER32 && i != CRC32 && !res->ctx) { av_free(res); return AVERROR(ENOMEM); }
    *ctx = res;
    return 0;
}
```

It is the only public way to obtain an `AVHashContext` (the body is
private to `hash.c`), it is exactly what `HashContext::new` wraps, and its
result is released by `av_hash_freep`, which *is* recorded as the dropper.
Every sibling constructor in the wave carries
`is_constructor: {alloc: true, init: false}` (`av_md5_alloc`,
`av_aes_alloc`, `av_hmac_alloc`, ...), and slot-writing ones such as
`av_expr_parse` record it with `for: <slot arg>`.

## Fix

Resubmitted `av_hash_alloc` (`submission.json`, applied with
`wavefront ... query symbols --name av_hash_alloc --update submission.json`):
`lifetime: {for: "ctx", is_constructor: {alloc: true, init: false}}`, both
argument records unchanged except a fuller `ctx` note (child ownership,
failure behaviour, the `av_hash_init` requirement). Afterwards
`--lifecycle-ops` for `AVHashContext` lists `av_hash_alloc`, `av_hash_freep`
and `av_hash_init` (`lifecycle-ops-after.txt`).
