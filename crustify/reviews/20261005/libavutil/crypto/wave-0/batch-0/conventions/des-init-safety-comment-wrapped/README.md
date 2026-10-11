# Conventions: `DesMut::init`'s unsafe block counted as undocumented

- File: `crustify/rust/ffmpeg/libavutil/des.rs`
- Affected commit: `f7cdef0c4d` (wave tip `7935c403e0`)
- Item: `av_des_init` through `DesMut::init` (`AVDES` lifecycle)

## Finding

`crustify scan-unsafe` counted one undocumented unsafe site in `des.rs`.
rustfmt had wrapped the call as `let result =\n    unsafe { ffi::av_des_init(.., i32::from(decrypt)) };`,
so the `// SAFETY:` block sat above the `let` line, not above the line
holding `unsafe {`. Clippy accepts that; the campaign's safety accounting
does not.

## Fix

The mode conversion is bound first (`let mode = i32::from(decrypt);`), so
`let result = unsafe { .. };` fits one line directly under its unchanged
`// SAFETY:` block. `scan-unsafe` reports no undocumented site in `des.rs`.
