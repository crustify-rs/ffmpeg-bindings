# Conventions: a `// SAFETY:` comment in `equiv/frame.rs` sat above a `let` line, not above its `unsafe`

- File: `crustify/rust/ffmpeg/tests/equiv/frame.rs`,
  `side_data::c_adders_on_the_rust_array_match_c_arrays`
- Affected commit: `83bd5f2e64` (buffer-frame wave 2, type batch 0)

## Finding

rustfmt broke the statement after `let x =`, because the call did not fit on
one line:

```rust
// SAFETY: the Rust array, lent to C for the call, and a live
// source entry, whose buffer C shares and metadata it copies.
let x =
    unsafe { ffi::av_frame_side_data_clone(&mut sd, &mut nb, src.entry(i), flags) };
```

Clippy's `undocumented_unsafe_blocks` accepts this. `crustify scan-unsafe`
looks back from the `unsafe {` line and stops at the first line that is not
a comment, so it stopped at `let x =` and counted the site as undocumented.
At the wave tip it reported 23 undocumented sites,
`per_file["ffmpeg/tests/equiv/frame.rs"] == 1`. This was the only such site
in the workset's files (`frame.rs`, `tests/{equiv,ub_safe,ub_unsafe}/frame.rs`
and `tests/common/side_data.rs`). The scan's own rule finds the same one
site.

## Fix

The source entry is bound first, so the call fits on the `let` line and the
comment sits directly above the `unsafe`:

```rust
let entry = src.entry(i);
// SAFETY: the Rust array, lent to C for the call, and a live
// source entry, whose buffer C shares and metadata it copies.
let x = unsafe { ffi::av_frame_side_data_clone(&mut sd, &mut nb, entry, flags) };
```

The test is otherwise unchanged. The rewritten lines are rustfmt-clean, and
the scan now reports no undocumented site in the file.
