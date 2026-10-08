# Misc: string constructors misstated the `av_max_alloc` boundary

- File: `crustify/rust/ffmpeg/libavutil/mem.rs`
- Affected commit: `a95b9e3220`
- Items: `string_from_c_str` (`av_strdup`), `string_from_bytes_until_nul`
  (`av_strndup`)

## Finding

`string_from_c_str` documented its failure as "a string longer than the
`av_max_alloc` limit". Both C routines allocate through `av_realloc`, which
rejects `size > max_alloc_size`, and both ask for one byte more than the copy
(`strlen(s) + 1`, `n + 1`). So a string whose length *equals* the limit
already fails: with the limit at 8, `"01234567"` (8 bytes) cannot be copied.
`string_from_bytes_until_nul` pointed to that sentence ("as
[`string_from_c_str`]"). It also never said that its size is the bytes before
the first NUL, not `bytes.len()`, so a long slice with an early NUL still
fits.

The behaviour itself matched C. Only the documented contract was off by one,
and nothing tested the failure paths: the C branches `if (ptr)` (`av_strdup`)
and `if (!ret) return NULL` (`av_strndup`) were never taken by any suite.

## Fix

- The docs now state the boundary as `count_bytes() + 1` / `n + 1` bytes
  exceeding the limit.
- `libavutil-sys` binds `av_max_alloc`.
- `tests/equiv/mem.rs`: `string_failure_at_the_av_max_alloc_limit_matches_c`
  lowers the limit to 8 and compares `string_from_c_str` with `av_strdup`,
  `string_from_bytes_until_nul` with `av_strndup` at every length of a
  NUL-free and an early-NUL buffer, and `try_clone` with `av_strdup` of the
  same source, on both sides of the boundary. The limit is process-global, so
  the test re-runs itself alone in a child process
  (`CRUSTIFY_EQUIV_ISOLATED`), and the other tests in the binary never see it.
  It also covers the two C failure branches above.
