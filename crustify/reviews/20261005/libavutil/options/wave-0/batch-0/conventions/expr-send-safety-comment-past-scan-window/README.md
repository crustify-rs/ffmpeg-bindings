# Conventions: `Send for Expr`'s safety comment counted as undocumented

- File: `crustify/rust/ffmpeg/libavutil/eval.rs`
- Affected commit: `f9ad1d8e7f`
- Item: `unsafe impl Send for Expr` (`AVExpr`)

## Finding

`crustify scan-unsafe` counted one undocumented unsafe site in `eval.rs`.
It is `unsafe impl Send for Expr {}`: its `// SAFETY:` block ran nine lines,
and the scan looks back at most six lines from the `unsafe` line, so the
`SAFETY:` marker fell outside its window. Clippy's
`undocumented_unsafe_blocks` accepted it, but the campaign's safety
accounting did not.

## Fix

The general background (allocation and release through `av_free`, the
handover by `av_expr_parse`, why the type is not `Sync`) moved into a plain
comment above, and the `// SAFETY:` block keeps the proof proper (what a
node holds, and why nothing in it is tied to a thread) in five lines. The
argument is unchanged. `scan-unsafe` now reports no undocumented site in
`eval.rs`.
