# Conventions: `RefStructUnref`'s `CDrop` `// SAFETY:` block was too long for `scan-unsafe` to find

- File: `crustify/rust/ffmpeg/libavutil/refstruct.rs`, `unsafe impl<T> CDrop<T> for RefStructUnref`
- Affected commit: `0ea2c20f1f` (buffer-frame wave 1, symbol batch 1). It
  extended the comment to cover the new `alloc_with_release`, `CAlloc` and
  typed-pool sources.

## Finding

The comment above the `unsafe impl` was 10 lines long, starting with
`// SAFETY:`. `crustify scan-unsafe` looks back at most 6 lines from the
`unsafe impl` line for the word `SAFETY`, so it reported the site as
undocumented. Clippy's `undocumented_unsafe_blocks` accepts the block, but
the scan does not. The count went from 22 at the wave-0 review to 23 at the
wave-1 tip `1a9271f931`
(`undocumented_unsafe.per_file["ffmpeg/libavutil/refstruct.rs"] == 1`). This
was the only new site in the wave's symbol homes.

## Fix

The general part of the argument (which safe sources give an object which
release callback) moved into a plain comment above. The `// SAFETY:` block
now sits directly on the `unsafe impl` and is 3 lines long:

```rust
// The safe sources of a `RefStructUnref` reference, and of its clones, give
// every object either no callback (...) or one that accepts every payload
// safe code can leave: (...).
// SAFETY: `av_refstruct_unref` drops one count of the object and frees its
// block once, when the last reference goes, after running the object's
// release callback, if any, which accepts the payload, as above.
unsafe impl<T> CDrop<T> for RefStructUnref {
```

The wording of the argument is unchanged. After the fix the scan reports
22 undocumented sites, and none in `refstruct.rs`. Clippy is still clean.
