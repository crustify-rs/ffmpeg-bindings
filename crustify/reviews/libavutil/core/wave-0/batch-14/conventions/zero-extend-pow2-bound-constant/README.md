# Conventions: `zero_extend` spelled its bit-position bound differently from its siblings

- File: `crustify/rust/ffmpeg/libavutil/common.rs`
- Affected commit: `ccdbbc0a77` (core wave 0, batch 14); the siblings
  landed in `35f30a7185` (batch 10, `av_clip_intp2_c` / `av_clip_uintp2_c`)
- Item: `av_zero_extend_c` (symbol route, wrap)

## Finding

`common.rs` guards the three header inlines that shift by a caller-chosen
position (`av_clip_intp2_c`, `av_clip_uintp2_c`, `av_zero_extend_c`) the
same way: `None` when `p > 31`, since `1U << p` is undefined past that. The
two clamps compare against the module constant `MAX_POW2` (which the module
docs and the inline tests also use) and are `#[inline]` like every other
value-only helper in the file. `zero_extend` compared against a literal `31`
and had no `#[inline]`, so the one bound was written in two places and a
reader could not tell whether the three helpers meant the same limit.

Behaviour was identical; this is a consistency deviation only.

## Fix

`zero_extend` uses `MAX_POW2`, is `#[inline]`, and its safety comment is
worded as the clamps' ("`p <= 31` keeps its `1U << p` defined"). The
existing tests `zero_extend_is_defined_at_every_position` (ub_safe) and
`zero_extend_matches_c_at_every_defined_position` (equiv) cover the bound
on both sides of 31 and keep passing.
