# Wavefront: output capacities missing from three crypto wave-0 findings

- Symbols: `av_base64_encode` (`libavutil/base64.c`, args 0 `out` and 2 `in`),
  `av_md5_sum` (`libavutil/md5.c`, args 0 `dst` and 1 `src`),
  `av_uuid_unparse` (`libavutil/uuid.c`, arg 1 `out`)
- Store: `crustify/wavefront/ownership-store.json`
- Submitted by: crypto wave-0 batch 10 (`00728268f2`)
- Resubmitted with `query symbols --name <symbol> --update <symbol>.json`
  (the three submissions are kept beside this report)

## Previous findings

Every output argument above carried the generic note

> Synchronous borrowed buffer; no storage, retention, cloning or deallocation.

(plus "NUL-terminated string." for the two `char *` outputs), and
`av_base64_encode`'s `in` was recorded `nullable: false`.

## Why they were incomplete

The structural facets (by-value array, borrowed for the call, mutable, no
lifecycle role) were right. What the records did not state is the one
property a wrapper's soundness rests on for an output pointer: how many bytes
C writes through it.

- `av_uuid_unparse` has no size argument and writes exactly 37 bytes
  (`uuid.c`: 32 hex digits, 4 hyphens, NUL; `uuid.h`: "no less than 37
  characters"). The record gave no length at all.
- `av_md5_sum` writes exactly 16 bytes (`av_md5_final`, four `AV_WL32`);
  again no length.
- `av_base64_encode` writes exactly `AV_BASE64_SIZE(in_size)` bytes after
  checking `in_size < UINT_MAX / 4` and `out_size >= AV_BASE64_SIZE(in_size)`,
  and never writes past that. The record did not say so, nor that `out_size`
  is merely an upper bound the caller may set to any `int` up to its buffer.
  The wrapper written from it treated the buffer length itself as having to
  fit `int` and refused larger buffers (see
  `../../equiv/base64-rejects-outputs-beyond-int-max`).
- `av_base64_encode`'s `in` is neither dereferenced nor offset when
  `in_size` is 0 (both loops are guarded by `bytes_remaining`), so `NULL` is
  valid there, as `av_adler32_update`'s `buf` was already recorded. By
  contrast `av_md5_sum`'s `src` stays non-nullable: `av_md5_update` forms
  `src + (len & ~63)` unconditionally, and C does not define `NULL + 0`; the
  resubmitted note now says why.

The wrappers' own contracts (`[u8; 37]`, `[u8; 16]`, the encoder's
`needed` check) were right in the Rust code; the store is what later
batches and reviewers query, so it now carries them too.

## Corrected findings

Only `note` changed, plus `nullable: true` on `av_base64_encode`'s `in`:

| symbol | arg | corrected note (abridged) |
|---|---|---|
| `av_base64_encode` | 0 `out` | writes exactly `AV_BASE64_SIZE(in_size)` bytes after its checks, never more; `out_size` may be any `int` up to the buffer length |
| `av_base64_encode` | 2 `in` | read for `in_size` bytes; `NULL` valid when `in_size` is 0 |
| `av_md5_sum` | 0 `dst` | exactly 16 bytes, written once at the end |
| `av_md5_sum` | 1 `src` | `NULL` invalid even for `len` 0 (`src + (len & ~63)`) |
| `av_uuid_unparse` | 1 `out` | exactly 37 bytes; the `AVUUID` input is an array-typedef parameter extraction does not enumerate |

The last point is the known extraction gap for array parameters (formats
wave-1 batch 4, recorded in the campaign's test recipes). Related and
composer-owned, so not correctable through `--update`: the records' `type`
strings for UUID routines list every `uint8_t[16]` typedef in the database
as if they were parameters, e.g.
`int av_uuid_equal(const AVUUID, const UID, const ff_asf_guid, const uint8_t[16], const AVUUID, const UID, const ff_asf_guid, const uint8_t[16])`
for a two-parameter function (`UID` is `libavformat/mxf.h`'s,
`ff_asf_guid` `libavformat/asf.h`'s). Read the header, not the `type`
string, for such signatures.
