# `InvalidQoffset`'s message omits the zero-denominator case it reports

- Objective: `misc`
- Where: `impl fmt::Display for RegionsOfInterestError`, `src/libavutil/libavutil/frame.rs:1433-1438`
  at the wave tip `1cc3c27b646` (wave 902 batch 1, commit `3ad70349610`)
- Fixed: yes, in this review's Rust-tree commit, with a unit assertion

## What is wrong

`RegionsOfInterestError::InvalidQoffset(i)` is returned for two conditions, as its doc comment
and `qoffset_is_storable` (`frame.rs:1359-1364`) say: a quantisation offset with a zero
denominator, or one outside `[-1, 1]`. Its `Display` text names only the second:

```text
region of interest {i} has a quantisation offset outside [-1, 1]
```

Every rejected `0/0` or `n/0` offset is reported with this text, but a zero denominator is not
"outside [-1, 1]": it is no number at all. `RegionOfInterest::default()` uses `0/1`, but
`RegionOfInterest::zeroed()`, the `CZeroable` value, uses `0/0`. Storing a zeroed region is
therefore the likeliest way to hit this error, and the message then points the user at the wrong
cause.

## Evidence

At the wave tip, a region with offset `1/0` gets the out-of-range message:

```rust
use ffmpeg_bindings::libavutil::libavutil::frame::{Frame, RegionOfInterest};
use ffmpeg_bindings::libavutil::libavutil::rational::Rational;
let mut frame = Frame::new().unwrap();
let roi = RegionOfInterest::new(0, 16, 0, 16, Rational::new(1, 0));
let e = frame.as_mut().add_regions_of_interest(&[roi]).unwrap_err();
assert_eq!(e.to_string(), "region of interest 0 has a quantisation offset outside [-1, 1]");
```

## Fix

The message now names both conditions, as the variant's docs do:

```text
region of interest {i} has a quantisation offset with a zero denominator or outside [-1, 1]
```

The unit test `regions_payload_checks_and_writes_every_byte` asserts that the message mentions
the zero denominator.

## Rerun

```bash
git show 1cc3c27b646:src/libavutil/libavutil/frame.rs | sed -n 1412,1441p
cargo test -p ffmpeg-bindings --lib regions_payload_checks_and_writes_every_byte
```

The second command fails on the wave tip once the new assertion is in place, and passes on the
landed tree.
