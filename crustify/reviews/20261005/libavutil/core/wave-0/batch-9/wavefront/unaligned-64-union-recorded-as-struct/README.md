# Wavefront: `unaligned_64` is a `union`, its record says `kind: struct`

- Record: `wavefront query types --name unaligned_64`
- Field: `kind` (composer-emitted, not agent-submittable)

## Finding

`libavutil/intreadwrite.h:216` defines

```c
union unaligned_64 { uint64_t l; } __attribute__((packed)) av_alias;
```

and bindgen emits `#[repr(C, packed)] pub union unaligned_64 { pub l: u64 }`.
The type record reports `"kind": "struct"`. The same happens for
`unaligned_16` and `av_alias64`, so the composer's `kind` does not
distinguish unions from structs at all, although the schema says it emits
`struct`, `union`, or `enum`.

The agent-submitted part of the record (the `fields` entry for `l`, the
`_comment_agent` describing a packed, may-alias, lifecycle-free union only
ever reached through a cast byte pointer) was checked against the header and
every libavutil toucher (`mem.c` `fill32`, `aes.c` `addkey_*`, `adler32.c`
`av_adler32_update`) and is correct and complete; `--lifetime-for
unaligned_64` is empty, as it should be. Nothing was resubmitted:
`--update-help` lists only `fields` and `_comment_agent` as valid keys, so
`kind` cannot be corrected from here.

## Impact

For this item none: the translation treats the type as a union (the
bindgen layout decides). A translator reading a `kind: struct` record for a
multi-member union (an `av_alias*`) without opening the header could wrap
it as a struct with independent fields and miss that every member aliases
the same bytes. The composer should emit `union` for `union` tags.
