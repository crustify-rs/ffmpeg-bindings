# Conventions: an `equiv/frame.rs` unsafe block whose `// SAFETY:` comment sat above another line

- File: `crustify/rust/ffmpeg/tests/equiv/frame.rs`, `side_data_type::Frame::get`
- Affected commit: `36bcce3d6a` (buffer-frame wave 0, type batch 2,
  `AVFrameSideDataType`)

## Finding

```rust
// SAFETY: a non-NULL result is one of the frame's live entries.
(!sd.is_null())
    .then(|| SideDataType::from_raw(unsafe { ptr::addr_of!((*sd).type_).read() }))
```

The comment documents the projection inside the closure, but rustfmt had
already put the `(!sd.is_null())` line between them. Clippy's
`undocumented_unsafe_blocks` accepts that. `crustify scan-unsafe` does not: it
wants the `// SAFETY:` line directly above the line holding `unsafe {`, and it
counted the site in `undocumented_unsafe.per_file["ffmpeg/tests/equiv/frame.rs"]`,
the only count against the wave's files at the wave tip `adfecd6c81`.

## Fix

Return early on NULL, and bind the read on its own line under its comment:

```rust
if sd.is_null() {
    return None;
}
// SAFETY: a non-NULL result is one of the frame's live entries.
let raw = unsafe { ptr::addr_of!((*sd).type_).read() };
Some(SideDataType::from_raw(raw))
```

The behaviour is unchanged (`equiv/frame.rs` passes). The file's
`undocumented_unsafe` count is 0 after the fix.
