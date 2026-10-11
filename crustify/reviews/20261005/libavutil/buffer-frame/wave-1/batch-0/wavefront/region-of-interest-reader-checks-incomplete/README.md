# Wavefront: `AVRegionOfInterest`'s record misstated what C readers check

- Entity: type `AVRegionOfInterest` (`libavutil/frame.h`)
- Earlier submission: buffer-frame wave 1, type batch 0 (`1a9271f931`)
- Resubmitted: `submission.json` here, through `query types --name
  AVRegionOfInterest --update`. The fields have no pointer and their
  properties are unchanged. Only `_comment_agent` is corrected.

## Finding

The earlier comment listed the region readers (`vf_showinfo`, `libx264`,
`libx265`, `libvpxenc`, `vaapi_encode`, `qsvenc`, `d3d12va_encode`) as
"rejecting self_size == 0 or not dividing the payload size, and qoffset.den
== 0", and concluded "all-zero is a valid value (consumers reject it with an
error, not UB)". It is incomplete in three ways that matter to every later
wrapper that hands a region array to C:

1. Those checks accept any nonzero `self_size` that divides the payload size.
   The readers then read a full 28-byte struct at each `self_size` stride, so
   an undersized `self_size` reads past the payload. ASan confirms it in
   `vf_showinfo.c:151` and `vf_addroi.c:139` (traces and probe in
   `../../misc/region-of-interest-self-size-hazards`).
2. `vf_addroi` (a reader the record listed only as a writer) asserts
   instead of rejecting: `av_assert0(old_roi_size && ...)` at
   `vf_addroi.c:119` aborts on a zero `self_size`.
3. Only `qoffset.den == 0` is refused, and some encoders do unchecked
   arithmetic on the rest. `d3d12va_encode.c:239` computes `delta_qp =
   roi->qoffset.num * qp_range / roi->qoffset.den` in `int`, which overflows
   for large `num`. `libvpxenc.c:1610` computes `delta_q = (int)(num * 1.0f /
   den * MAX_DELTA_Q)`, a float-to-int conversion that is UB outside `int`.
   This campaign's build leaves out both encoders (`--disable-autodetect`), so
   this item is from reading the code, not a run.

## Correction

The comment now records that `frame.h` requires `self_size ==
sizeof(AVRegionOfInterest)`. It records the readers' actual checks, the
over-read for an undersized `self_size`, `vf_addroi`'s abort and the
encoders' unchecked `qoffset` arithmetic. It adds the rule for later
wrappers: check every element's `self_size` (and, for those encoders, the
`qoffset` range) before handing a `REGIONS_OF_INTEREST` payload to C. The
wrapper description at the end is kept, and notes that `Default` also sets
`self_size`.
