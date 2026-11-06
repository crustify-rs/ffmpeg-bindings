Subject: ffmpeg-bindings: ROI store docs omit the rectangle hazards in FFmpeg's encoders

Hi Marius,

While reviewing wave 902 of `ffmpeg-bindings` (commit `3ad70349610`), I found that the
documentation of the new checked region-of-interest stores
(`FrameMut::add_regions_of_interest`, `SideDataArray{,Mut}::add_regions_of_interest`) lists
what they guard C's readers against, and leaves out a class of inputs those readers mishandle
just as badly.

The stores fix `self_size`, refuse an empty payload, and refuse a qoffset with a zero denominator
or outside [-1, 1]. They pass `top`, `bottom`, `left` and `right` through unchecked, and the docs
say only that "a region outside the frame ... is a valid value". FFmpeg's encoders trust those
`int`s:

```c
/* libavcodec/libx264.c:450-466 (libx265 and d3d12va_encode alike) */
starty = FFMIN(mby, roi->top / MB_SIZE);
endy   = FFMIN(mby, (roi->bottom + MB_SIZE - 1)/ MB_SIZE);
...
for (int y = starty; y < endy; y++)
    for (int x = startx; x < endx; x++)
        qoffsets[x + y*mbx] = qoffset;
```

A negative `top` or `left` writes before `qoffsets`, and `bottom`/`right` near `INT_MAX` overflow.
`libvpxenc` has the same sum, `vaapi_encode` computes `right - left`, and `qsvenc` aligns each edge
with `FFALIGN`, all in `int`. The docs also name only `d3d12va_encode` for the in-range offset
product `num * range`. `vaapi_encode.c:557` and `qsvenc.c:2186` compute the same product.

None of these encoders is in the campaign's build, so nothing is reachable from the crate today
and I have no sanitizer trace. These are source readings at FFmpeg `1019f8f036`. The risk is that a
future encoder wrapper treats the store's docs as the complete list of what it must check.

The patch, in the same review, documents the rectangle hazards and the two extra offset products
on `RegionOfInterest`, says that an encoder wrapper must check the rectangle against the frame,
and adds the same facts to the ownership-store note. The stores' behaviour is unchanged. I can
share the review report on request.

Found by [Crustify](https://github.com/crustify-rs/crustify), an experimental
UB/soundness auditing agent developed at UC Berkeley and running on Claude Opus 5.5, then
manually reviewed and independently reproduced.
