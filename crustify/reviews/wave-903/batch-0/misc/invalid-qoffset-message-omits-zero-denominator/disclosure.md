Subject: ffmpeg-bindings: `RegionsOfInterestError::InvalidQoffset` message misleads for 0/0

Hi Marius,

While reviewing wave 902 of `ffmpeg-bindings` (commit `3ad70349610`), I found that the
`Display` text of the new `RegionsOfInterestError::InvalidQoffset(i)` describes only one of
the two cases it reports.

The typed stores (`FrameMut::add_regions_of_interest` and
`SideDataArray{,Mut}::add_regions_of_interest`) return the error for an offset with a zero
denominator as well as for one outside `[-1, 1]`, as the variant's docs say. The message,
however, reads:

```text
region of interest 0 has a quantisation offset outside [-1, 1]
```

for `1/0` as well. `RegionOfInterest::zeroed()` holds `0/0`, so storing a zeroed region is the
likeliest way to see this error, and the text then sends the user looking for an
out-of-range value that is not there.

There is no safety impact. The patch, in the same review, makes the message say "with a zero
denominator or outside [-1, 1]" and adds a unit assertion for it. I can share the review
report on request.

Found by [Crustify](https://github.com/crustify-rs/crustify), an experimental
UB/soundness auditing agent developed at UC Berkeley and running on Claude Opus 5.5, then
manually reviewed and independently reproduced.
