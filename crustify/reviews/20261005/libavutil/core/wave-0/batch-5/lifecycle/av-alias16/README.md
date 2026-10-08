# Lifecycle: `av_alias16` had no borrowed form over caller bytes

- File: `crustify/rust/ffmpeg/libavutil/intreadwrite.rs`
- Affected commit: `8a4ef630c5` (wrap of `av_alias16` / `av_alias32`)
- Item: `av_alias16` (`Alias16`, `Alias16Ref`, `Alias16Mut`)

## Finding

The same gap as [`av_alias32`](../av-alias32/README.md): the wrapper had the
by-value form (`Alias16::from_u16` / `from_u8x2` / `zeroed()`), the C-allocated
box and array, but no safe way to view two bytes the caller already holds as
the union, which is the only thing C does with `av_alias16` through the
public macros (`AV_RN16A`, `AV_WN16A`, `AV_COPY16`, `AV_ZERO16`: a cast of an
aligned byte pointer). The handles' `from_ptr` is `pub(crate)` and had no
public caller. The `Unaligned16Ref::from_bytes` sibling exists; the aligned
union's view did not.

## Fix

`Alias16Ref::from_bytes(&'a [u8; 2]) -> Option<_>` and
`Alias16Mut::from_bytes(&'a mut [u8; 2]) -> Option<_>`, `None` when the
address is not 2-aligned (the precondition the C macros leave unchecked).
Regression and equivalence tests are the ones listed in the `av_alias32`
report, which sweep both unions; the compile-fail
`intreadwrite_alias_bytes_written_while_viewed.rs` checks that the bytes
cannot be viewed exclusively while a shared `Alias16Ref` over them lives.
