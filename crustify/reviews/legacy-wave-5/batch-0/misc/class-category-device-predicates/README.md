# Misc: `ClassCategory` left out the header's two device predicates

- File: `crustify/rust/ffmpeg/libavutil/log.rs`
- Affected commit: `d511f2ca48` ("wrap libavutil/log.h's AVClassCategory and
  AVClassStateFlags")
- Item: `AVClassCategory` (`ClassCategory`)

## Finding

`libavutil/log.h` publishes exactly two operations over `AVClassCategory`,
the function-like macros

```c
#define AV_IS_INPUT_DEVICE(category) \
    (((category) == AV_CLASS_CATEGORY_DEVICE_VIDEO_INPUT) || \
     ((category) == AV_CLASS_CATEGORY_DEVICE_AUDIO_INPUT) || \
     ((category) == AV_CLASS_CATEGORY_DEVICE_INPUT))

#define AV_IS_OUTPUT_DEVICE(category) \
    (((category) == AV_CLASS_CATEGORY_DEVICE_VIDEO_OUTPUT) || \
     ((category) == AV_CLASS_CATEGORY_DEVICE_AUDIO_OUTPUT) || \
     ((category) == AV_CLASS_CATEGORY_DEVICE_OUTPUT))
```

which `libavdevice/utils.c` and `fftools/opt_common.c` use to classify a
format's `priv_class->category`. The wrapper exposed the eighteen constants,
`name()`, the raw conversions and `Debug`, but neither predicate, so a Rust
consumer classifying a category had to spell the three-way comparison out
itself, against constants whose grouping only the C header documents.

Wavefront schedules no macro nodes (`query dag --name AV_IS_INPUT_DEVICE`
is empty), so no later batch would have added them; the type's wrapper is
their only home. Not a soundness or equivalence defect: nothing the wrapper
did was wrong, it was incomplete.

## Fix

Per the translator's macro rule (a function-like macro is reached through
the smallest shim, then wrapped, never re-implemented as an independent Rust
API):

- `libavutil-sys/shim.{h,c}`: `crustify_AV_IS_INPUT_DEVICE(AVClassCategory)`
  and `crustify_AV_IS_OUTPUT_DEVICE(AVClassCategory)` return the macro's
  value. `shim.h` now includes `libavutil/log.h` for the parameter type.
  bindgen already allowlists `crustify_.*`, so `allowlist.txt` is unchanged.
- `log.rs`: `ClassCategory::is_input_device()` (`/// Wraps:
  AV_IS_INPUT_DEVICE`) and `ClassCategory::is_output_device()` (`/// Wraps:
  AV_IS_OUTPUT_DEVICE`), safe methods on the `Copy` value.

## Tests

- `mod unit_tests::device_predicates_name_the_device_categories`: the
  truth table over every named constant and over unnamed values below,
  inside and above the device range.
- `tests/equiv/log.rs::device_predicates_match_the_c_macros`: the safe
  methods against the raw shims over every named value and the boundaries
  `0, 11, 12, 39..=46, UINT_MAX`; also that the two sets are disjoint and
  together cover exactly `40..=45`.
- `tests/ub_safe/log.rs::device_predicates_accept_any_value`: the methods at
  the integer extremes; kept apart from the FFI-free category tests so those
  still run under Miri.
