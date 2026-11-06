# Wavefront: `av_alias64` and `av_intfloat32` are unions recorded as `kind: struct`

- Items: `av_alias64` (`libavutil/intreadwrite.h`), `av_intfloat32`
  (`libavutil/intfloat.h`)
- Record field: `kind` (composer-emitted; not part of an agent submission)
- Same defect as batch 7's
  `../../batch-7/wavefront/union-kind-recorded-as-struct/`, which carries
  the root cause (`src/entities/types.ql` tests `Struct` before `Union`,
  and CodeQL's `Union` subclasses `Struct`) and a patch. This note only
  records that both items of this batch are affected.

## Finding

Both items are C unions (`typedef union { … } av_alias av_alias64;`,
`union av_intfloat32 { uint32_t i; float f; };`), yet their records say
`"kind": "struct"`, as do those of every other union the libavutil wave
reaches:

```console
$ wavefront . --config crustify/wavefront/configs/wavefront-config.json \
    query types --name av_alias64 av_intfloat32 av_intfloat64 av_alias32 \
    av_alias16 unaligned_64 unaligned_32 unaligned_16 | grep -E '"(name|kind)": "(av_|unaligned|struct)'
    "name": "av_alias64",
    "kind": "struct",
    "name": "av_intfloat32",
    "kind": "struct",
    "name": "av_intfloat64",
    "kind": "struct",
    "name": "av_alias32",
    "kind": "struct",
    "name": "av_alias16",
    "kind": "struct",
    "name": "unaligned_64",
    "kind": "struct",
    "name": "unaligned_32",
    "kind": "struct",
    "name": "unaligned_16",
    "kind": "struct",
```

`query types --update-help` lists no `kind` key, so a submission cannot
correct it. The previous run's submitted findings for both items are
correct and complete: the `_comment_agent` texts describe them as punning
unions whose every member spans the whole word, and this review's
`--lifecycle-ops`, `--taking` and `--returning` queries confirm there is no
dropper, disposer, cloner, constructor, taker or returner for either.
Nothing to resubmit; the Rust wrappers were written from the headers.
