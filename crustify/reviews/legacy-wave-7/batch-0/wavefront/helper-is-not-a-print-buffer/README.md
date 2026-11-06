# The sizeof helper does not own AVBPrint storage

Affected base: `b2cff27cd691af1482bd36e8c2edab9d5e25fcf8`.

The previous `ff_pad_helper_AVBPrint.str` block said `owned: true`,
`borrowed.lifetime: self`, `mutable: true`, and both `string` and `array`.
It inferred runtime ownership from a different type, `AVBPrint`.

Evidence: `FF_PAD_STRUCTURE` in `libavutil/bprint.h` uses the helper only
in `sizeof`; no bprint routine takes a helper pointer. `BPrintFields` is
copyable, never frees its string, and independently sets all scalar fields.
The previous record's own comment acknowledged these facts but its structured
field still directed a consumer toward an owning/self-referential wrapper.

Resubmit `submission.json` with `wavefront ... query types --name
ff_pad_helper_AVBPrint --update submission.json`. The nullable string is a
borrow of external immutable storage in the authored Rust surface; there
is no C runtime ownership to mirror. No Rust ABI or lifecycle change.
