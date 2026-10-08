# Misc: `append_path_component` misstated its `av_max_alloc` boundary

- File: `crustify/rust/ffmpeg/libavutil/avstring.rs`
- Affected commit: `35f30a7185`
- Item: `append_path_component` (`av_append_path_component`)

## Finding

The doc said the call fails for "a result whose length plus its NUL exceeds
the `av_max_alloc` limit". C sizes the block before it knows the join:

```c
fullpath = av_malloc(p_len + c_len + 2);
```

and only then merges two slashes or skips the separator for an empty side.
So `av_malloc` is asked for `strlen(path) + strlen(component) + 2` bytes
whatever the join, and a join that needs fewer fails one byte early. With
the limit at 8, `append_path_component(c"", c"abcdefg")` returns `None`
although its result `"abcdefg"` plus NUL is 8 bytes, as does
`append_path_component(c"abc/", c"def")` for the 8-byte `"abc/def"`.

The behaviour matched C; only the documented boundary was off. The isolated
`av_max_alloc` equivalence test compared both sides but never pinned which
joins fail, so the doc's claim went unchecked.

## Fix

- The doc now states the request as `path.count_bytes() +
  component.count_bytes() + 2` bytes whatever the join, and that the sum
  exceeding the limit fails even when the joined text and its NUL would fit.
- `tests/equiv/avstring.rs::paths_and_format::failures_at_the_av_max_alloc_limit_match_c`
  adds the merged-slash and empty-side joins at the boundary and asserts, for
  every join, that the result is `Some` exactly when that sum is within the
  limit, beside the existing C comparison.
