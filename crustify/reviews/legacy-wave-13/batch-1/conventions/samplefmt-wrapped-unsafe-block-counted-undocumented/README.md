# `SampleBuffer::new`: a rustfmt-wrapped `let data =\n unsafe { .. }` counts as an undocumented unsafe block

- File: `crustify/rust/ffmpeg/libavutil/samplefmt.rs`, `SampleBuffer::new`
  (`Wraps: av_samples_alloc`)
- Affected commit: `0a2fbf9bb7` (landed by formats wave-1 batch 5)
- Severity: convention only; no behaviour change.

## Finding

`crustify scan-unsafe` reported two undocumented unsafe sites in
`samplefmt.rs` while clippy's `undocumented_unsafe_blocks` (denied
workspace-wide) accepts the file. One is the known walker quirk the shared
test recipes describe: rustfmt wrapped the adoption of the `av_malloc`
block as

```rust
        // SAFETY: on success `slots[0]` is an `av_malloc` block of `size`
        // bytes, released by `av_free` (`Heap`), which nothing else owns.
        let data =
            unsafe { HeapVec::from_raw_parts(slots[0].cast(), size) }.ok_or(AVERROR_ENOMEM)?;
```

so the `// SAFETY:` line sits above `let data =` rather than above the line
holding `unsafe {`, which the scan requires. The other reported site could
not be attributed to any block: every `unsafe` in the file has its
`// SAFETY:` (or the `unsafe impl` proofs) directly above it, and clippy
agrees.

## Fix

Bind the pointer first so the adoption fits one line under its comment:

```rust
        let block = slots[0].cast();
        // SAFETY: on success `slots[0]` is an `av_malloc` block of `size`
        // bytes, released by `av_free` (`Heap`), which nothing else owns.
        let data = unsafe { HeapVec::from_raw_parts(block, size) }.ok_or(AVERROR_ENOMEM)?;
```

The scan's `samplefmt.rs` count drops from 2 to 1 (24 crate-wide, from
25); the remaining one is the unattributable site above.
