Subject: ffmpeg-bindings: typed region-of-interest store and view lack their anchors

Hi Marius,

While reviewing wave 902 of `ffmpeg-bindings` (commit `3ad70349610`), I found that its
three new public functions carry no `Wraps` anchor or rustdoc alias, although each stands in
for a C item that its siblings already annotate:

- `FrameMut::add_regions_of_interest` stores regions with `av_frame_new_side_data`, like the
  annotated `alloc_side_data_zeroed` and `alloc_side_data_from_slice`;
- `SideDataArrayMut::add_regions_of_interest` does the same with `av_frame_side_data_new`,
  like `alloc_zeroed` and `alloc_from_slice`;
- `SideDataRef::regions_of_interest` reads `AVFrameSideData.data` as regions, like `data()`.

The campaign's conventions make these annotations the record of what was translated, and
the alias is how a user searching the docs for `av_frame_new_side_data` finds the Rust
equivalent. As it stands, that search turns up the byte-level stores and not the checked
one the type's docs recommend.

There is no behavioural impact. The fix, in the same review, adds the three anchors and
aliases in the form the siblings use, for example:

```rust
/// Wraps `fn::av_frame_new_side_data`
///
/// Appends `REGIONS_OF_INTEREST` side data holding `regions`, ...
#[doc(alias = "av_frame_new_side_data")]
pub fn add_regions_of_interest(&mut self, regions: &[RegionOfInterest]) -> ...
```

I can share the review report on request.

Found by [Crustify](https://github.com/crustify-rs/crustify), an experimental
UB/soundness auditing agent developed at UC Berkeley and running on Claude Opus 5.5, then
manually reviewed and independently reproduced.
