# Unsafe: `OptionRanges::query_default`'s contract rejected the NULL object C accepts

- File: `crustify/rust/ffmpeg/libavutil/opt.rs`
- Affected commit: `ad7acaeaca`
- Item: `OptionRanges::query_default` (`av_opt_query_ranges_default`)
- Kind: `# Safety` obligation stricter than the C contract (sound, but it
  forbids a legal input)

## Finding

`query_default` must stay `unsafe` until `AVClass`-enabled objects are
wrapped (core wave 1), so its `# Safety` section is the whole contract a
caller can rely on. It said: "As `query`, except that the class pointer may be
NULL (the query then fails with `AVERROR(ENOMEM)`)", which inherits from
`query` the obligation that `obj` "must address a live `AVClass`-enabled
object [...] or a live non-NULL `const AVClass *`".

C needs less. `av_opt_query_ranges_default` stores NULL into the out slot,
then looks the option up through `av_opt_find` -> `av_opt_find2`, which
starts with

```c
    if(!obj)
        return NULL;
    c= *(AVClass**)obj;
    if (!c)
        return NULL;
```

and never touches `obj` otherwise, so a NULL `obj` fails with
`AVERROR(ENOMEM)` exactly as a NULL class pointer does, with or without
`AV_OPT_SEARCH_FAKE_OBJ` or `AV_OPT_SEARCH_CHILDREN`. (`av_opt_query_ranges`
is different: it reads `*(AVClass **)obj` and `c->query_ranges`
unconditionally, so `query`'s obligation is right.) A Rust caller that holds
an optional object therefore had to branch before the call to honour a
contract C does not have, and the wavefront finding recorded the same
over-strict `nullable: false` for the argument
(see `../../wavefront/query-ranges-default-obj-nullable`).

## Fix

The `# Safety` section now reads: "As `query`, except that `obj` itself, or
the class pointer it addresses, may be NULL (the query then fails with
`AVERROR(ENOMEM)` without reading further), and the class's `query_ranges`
callback is not called." The implementation is unchanged.

## Regression evidence

- `tests/ub_unsafe/opt.rs::query_default::null_object_fails_without_a_list`:
  a NULL `obj` with each flag combination, within the stated contract,
  returns `Err(AVERROR(ENOMEM))` under ASan/UBSan, TSan and BSan.
- `tests/equiv/opt.rs::null_object_agrees_with_the_default_query`: the raw C
  call and the wrapper agree on the error and on the NULL out slot for a NULL
  object, known and unknown keys, and every flag combination.
