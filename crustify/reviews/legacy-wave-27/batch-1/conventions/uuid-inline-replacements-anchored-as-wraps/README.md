# Conventions: native UUID replacements anchored as `Wraps:`

- File: `crustify/rust/ffmpeg/libavutil/uuid.rs`
- Affected commit: `00728268f2` (crypto wave-0 batch 10)
- Items: `copy` (`av_uuid_copy`), `equal` (`av_uuid_equal`), `nil` (`av_uuid_nil`)

## Finding

The three header-inline routines are `memcpy`/`memcmp`/`memset` over 16
bytes. The batch rightly implemented them with the equivalent Rust
operations (`*destination = *source`, `==`, `= [0; 16]`), which never call C,
but anchored them `/// Wraps: av_uuid_*`. The anchor table reserves `Wraps:`
for a safe view over the FFI seam and `Replaces:` for a native Rust
translation; the crate's other std-equivalent inline replacements
(`common.rs`'s `count_ones` for `av_popcount_c`, the saturating adds) use
`Replaces:`.

## Fix

The three anchors now read `/// Replaces: av_uuid_copy`, `av_uuid_equal`
and `av_uuid_nil`. The orchestrator's anchor check accepts either form, so
campaign coverage is unchanged; the record now says these items do not cross
the boundary.
