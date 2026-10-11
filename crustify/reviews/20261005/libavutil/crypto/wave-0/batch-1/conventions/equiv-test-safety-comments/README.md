# Conventions: misplaced `// SAFETY:` comments in two equivalence tests

- Files: `crustify/rust/ffmpeg/tests/equiv/base64.rs`, `tests/equiv/adler32.rs`
- Affected commit: `c51ba553a2` (crypto wave-0 batch 10)

## Finding

- `equiv/base64.rs`: rustfmt wrapped
  `let valid =\n    unsafe { ffi::av_base64_decode(core::ptr::null_mut(), ...) };`, so the
  `// SAFETY:` line no longer sits right above the line holding `unsafe {`.
  `crustify scan-unsafe` counts the block as undocumented
  (`undocumented_unsafe.per_file["ffmpeg/tests/equiv/base64.rs"] = 1`).
- `equiv/adler32.rs`: a `// SAFETY:` comment sat above `let owned =
  input.to_vec();`, which has no unsafe operation, followed by a second one
  for the actual FFI call: a safety comment that justifies nothing.

## Fix

`base64.rs` binds the null pointer to a local first so the call stays on one
line under its comment (the new regression tests there follow the same
shape); `adler32.rs` keeps one comment, on the FFI call. `scan-unsafe` no
longer lists `tests/equiv/base64.rs` as undocumented.
