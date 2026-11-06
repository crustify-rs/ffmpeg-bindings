# Wavefront: hash and HMAC output sizes missing from the wave-1 findings

- Symbols: `av_hash_final`, `av_hash_final_bin`, `av_hash_final_hex`,
  `av_hash_final_b64` (`libavutil/hash.c`, arg 1 `dst`), `av_hmac_final`
  (`libavutil/hmac.c`, arg 1 `out`), `av_hmac_calc` (args 1 `data`, 3 `key`,
  5 `out`)
- Store: `crustify/wavefront/ownership-store.json`
- Submitted by: crypto wave-1 batch 1 (`0896e41211`)
- Resubmitted with `query symbols --name <symbol> --update <symbol>.json`;
  the six submissions are kept beside this report, and
  `notes-before-after.txt` lists every changed note (only `note` changed; all
  structural facets and `lifetime` are as submitted)

## Previous findings

Four of the output arguments, and `av_hmac_calc`'s inputs, carried a note
shared by every pointer of the batch:

> Call-scoped borrow; no caller pointer retained. Context storage and
> children remain C-owned; byte buffers have the scalar length/count or
> backend digest size.

`av_hash_final_hex`'s and `_b64`'s `dst` notes did record their small-size
hazards (capacities 0/1 write nothing; size 0 writes `dst[-1]`), but none of
the six said how many bytes C writes or that the size parameter is only an
upper bound.

## Why they were incomplete

This is the class wave 0 found for `av_base64_encode` / `av_md5_sum` /
`av_uuid_unparse` (`../../../../wave-0/batch-1/wavefront/output-capacities-missing-from-findings`),
and it had the same consequence: the wrappers written from these records
treated the slice length as having to fit the C size type and refused larger
buffers, where C, given the largest size it accepts, succeeds
(`../../equiv/hash-and-hmac-reject-outputs-beyond-c-ranges`). What C actually
writes, from `hash.c` and `hmac.c`:

| symbol | bytes written through the output |
|---|---|
| `av_hash_final` | exactly `av_hash_get_size(ctx)` (<= 64) |
| `av_hash_final_bin` | exactly `size`: `min(size, digest)` digest bytes, then zeros |
| `av_hash_final_hex` | at most `min(2 * digest + 1, size)`; nothing for `size` < 2 |
| `av_hash_final_b64` | at most `min(AV_BASE64_SIZE(digest), size)` (<= 89); `dst[-1]` for `size` 0 |
| `av_hmac_final`, `av_hmac_calc` | exactly `hashlen` (16..64) after `outlen >= hashlen`, read back by the outer hash; nothing on `EINVAL` |

So `size` / `outlen` may be any value up to the caller's buffer length, and a
caller with a longer buffer passes `INT_MAX` / `UINT_MAX`.

## Corrected findings

Each changed note now states the exact byte count written, that the size is
an upper bound (and what a caller with a longer buffer passes), and the
existing small-size hazards. `av_hmac_calc`'s `data` and `key` notes now say
what C does with them (read once by the update callback; copied, or hashed
down to `hashlen` when longer than the block). See `notes-before-after.txt`.
