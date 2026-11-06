# The typed stores' docs omit the rectangle hazards, and two of the offset ones, in C's encoders

- Objective: `misc` (incomplete documentation of a hazard that later wrappers must guard)
- Where: the doc comment of `RegionOfInterest`, `src/libavutil/libavutil/frame.rs:957-974` at the
  wave tip `1cc3c27b646` (wave 902 batch 1, commit `3ad70349610`), and the
  `AVRegionOfInterest` note in `crustify/wavefront/ownership-store.json`
- Fixed: yes, in this review's Rust-tree commit (documentation and the ownership-store note)

## What is wrong

Wave 902 made `FrameMut::add_regions_of_interest` and `SideDataArray{,Mut}::add_regions_of_interest`
the way regions reach side data. Their docs and the type's docs list what these stores guard
against on behalf of C's readers. A `self_size` other than `sizeof` and an empty payload are
handled. So is a quantisation offset with a zero denominator or outside `[-1, 1]`, which "some
encoders scale ... unchecked in `int` arithmetic". The docs also name the one residual offset
hazard they leave to a later wrapper: `d3d12va_encode`'s `num * qp_range` overflow for large
in-range terms.

The rectangle gets no such treatment. The stores accept any `top`, `bottom`, `left` and `right`,
and the docs say only that "a region outside the frame, or with any offset, is a valid value".
C's encoders trust these four `int`s as they do the offset:

| reader | code at `c/` `1019f8f036` | hazard |
|---|---|---|
| `libx264.c:450-467` | `starty = FFMIN(mby, roi->top / MB_SIZE); endy = FFMIN(mby, (roi->bottom + MB_SIZE - 1)/ MB_SIZE);` (same for x), then `qoffsets[x + y*mbx] = qoffset` | `bottom`/`right` within 15 of `INT_MAX`: signed overflow. Negative `top` or `left`: `FFMIN` bounds the start from above only, so the loop writes before `qoffsets` |
| `libx265.c:677-690` | the same pattern with `mb_size` | the same two |
| `d3d12va_encode.c:227-250` | the same pattern with `block_size`, writing `qp_map[x + y * block_width]` | the same two |
| `libvpxenc.c:1646-1658` | `av_clip((roi->bottom + block_size - 1) / block_size, 0, rows)` | the sum overflows before the clip (the start is clipped from below, so no out-of-bounds write) |
| `vaapi_encode.c:564-567` | `.width = roi->right - roi->left`, `.height = roi->bottom - roi->top` | signed overflow for opposite-signed extremes |
| `qsvenc.c:2181-2184` | `FFALIGN(roi->top, mb_size)`, and the same for the other three | signed overflow within `mb_size - 1` of `INT_MAX` |

The offset list is incomplete as well. The docs name `d3d12va_encode`'s `num * qp_range` as the
one overflow left for large terms within `[-1, 1]`. Two other encoders make the same `int`
product: `vaapi_encode.c:557` (`roi->qoffset.num * ctx->roi_quant_range / roi->qoffset.den`) and
`qsvenc.c:2185-2186` (`roi->qoffset.num * 51 / roi->qoffset.den`). An offset such as
`i32::MAX / i32::MAX`, which the stores accept, overflows all three.

So a safe caller can store, through the "checked" store, a region that makes a C encoder overflow
or write out of bounds. The stores' docs call out the analogous offset hazards and stay silent on
this one. A later wrapper of these encoders, in the `libavcodec` link unit this campaign also
wraps, would read the type's docs and the ownership-store note as the complete list of what it
must check.

## Why this is not a UB finding

None of these readers is in the campaign's build: `crustify/builds/plain/config_components.h` sets
`CONFIG_LIBX264_ENCODER`, `CONFIG_LIBX265_ENCODER`, `CONFIG_LIBVPX_VP8_ENCODER`,
`CONFIG_LIBVPX_VP9_ENCODER`, `CONFIG_H264_D3D12VA_ENCODER`, `CONFIG_HEVC_D3D12VA_ENCODER`,
`CONFIG_H264_VAAPI_ENCODER`, `CONFIG_HEVC_VAAPI_ENCODER` and `CONFIG_H264_QSV_ENCODER` to 0. The
only built readers are `vf_showinfo` (which prints the values) and `vf_addroi` (which copies
them). No sanitizer can be triggered through the campaign's target, so there is no reproducer.
The finding is that the hazard list is incomplete. The stores' behaviour stands: `frame.h`
documents no range for the rectangle, and refusing negative or large coordinates would diverge
from C's own stores. That choice belongs to an encoder's wrapper, as it already does for
`d3d12va_encode`'s offset.

## Fix

- `RegionOfInterest`'s doc comment now says, after the offset sentence, that the stores
  do not check the rectangle, and names the hazards above: the block-size sums near `i32::MAX` in
  `libx264`, `libx265`, `libvpxenc` and `d3d12va_encode`, the writes before the map for a negative
  `top` or `left` in `libx264`, `libx265` and `d3d12va_encode`, `vaapi_encode`'s differences and `qsvenc`'s alignment.
  It adds that a wrapper of those encoders must check the rectangle against the frame. It also
  names `vaapi_encode` and `qsvenc` beside `d3d12va_encode` as the encoders whose `int` product
  overflows for large in-range offset terms.
- The `AVRegionOfInterest` record's `_comment_user` in the ownership store gets the same facts,
  submitted with `wavefront ... query types --name AVRegionOfInterest --update` and spliced in
  alone (the CLI's rewrite of the other type records left out).

## Rerun

```bash
sed -n 444,467p c/libavcodec/libx264.c
sed -n 674,690p c/libavcodec/libx265.c
sed -n 222,250p c/libavcodec/d3d12va_encode.c
sed -n 1640,1658p c/libavcodec/libvpxenc.c
sed -n 556,568p c/libavcodec/vaapi_encode.c
sed -n 2179,2187p c/libavcodec/qsvenc.c
git show 1cc3c27b646:src/libavutil/libavutil/frame.rs | sed -n 940,978p
grep -E 'define CONFIG_(LIBX26[45]|LIBVPX_VP[89]|H264_QSV|(H264|HEVC)_(VAAPI|D3D12VA))_ENCODER ' \
    crustify/builds/plain/config_components.h
```
