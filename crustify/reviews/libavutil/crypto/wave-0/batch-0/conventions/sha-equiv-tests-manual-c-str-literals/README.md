# Conventions: the SHA equivalence tests fail `clippy --all-targets -D warnings`

- Files: `crustify/rust/ffmpeg/tests/equiv/{sha,sha512}.rs`
- Affected commit: `eccfa5bb34` (the AVSHA / AVSHA512 batch); wave tip `7935c403e0`
- Items: `AVSHA`, `AVSHA512` (their equivalence suites)

## Finding

At the wave tip `cargo clippy --workspace --all-targets -- -D warnings`
fails. Besides the six `items_after_test_module` errors in the crypto homes,
which the symbols review fixed in parallel
(`../../batch-1/conventions/symbol-wrappers-after-unit-tests-module`), it
reports `clippy::manual_c_str_literals` twice:

```text
error: manually constructing a nul-terminated string
  --> ffmpeg/tests/equiv/sha.rs:42:13
   |
42 |             b"\0".as_ptr()
   |             ^^^^^ help: use a `c""` literal: `c""`
(same at tests/equiv/sha512.rs:47:13)
```

`b"\0"` there is the live static byte the probe hands to C for an empty
update (instead of an empty slice's dangling pointer).

## Fix

Both tests use `c"".as_ptr().cast()`, still a live static byte with the
same meaning. Clippy `--all-targets -D warnings` is clean afterwards.
