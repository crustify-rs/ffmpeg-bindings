# Two `PixelFormatDescriptor` unit tests assume two occurrences of one literal share an address

Affected base: `0a2fbf9bb7` (the wave tip; the tests landed in
`287eeecb28`, formats wave 1 batch 2). File:
`crustify/rust/ffmpeg/libavutil/pixdesc.rs`, `mod unit_tests`.

`pixel_format_descriptor_raw_round_trip_keeps_every_field` built a
descriptor with `set_name(Some(c"yuv420p"))` / `set_alias(Some(c"i420,yv12x"))`
and then asserted that `into_raw().name == c"yuv420p".as_ptr()` and
`.alias == c"i420,yv12x".as_ptr()` against *new* occurrences of the
literals; `pixel_format_descriptor_get_and_set_copy_whole_values` likewise
asserted `copy.name().as_ptr() == c"yuv420p".as_ptr()`. Rust does not
promise that two occurrences of one literal share an address: rustc and
the linker usually merge them, which is why the plain, sanitized and
coverage builds pass, but Miri does not, so under Miri the assertions
compare two distinct allocations.

Evidence (`miri-seeds-before.log`): on the reviewed tree,
`cargo +nightly miri test -p ffmpeg --lib -- pixel_format_descriptor_raw_round_trip
pixel_format_descriptor_get_and_set` fails both tests with
`MIRIFLAGS=-Zmiri-seed=<n>` for every seed 0-5 (`left: 0xc67f9a right:
0xc6f295`, two addresses of one literal). This review's Miri pass over
the three homes' 41 FFI-free unit tests (no explicit seed) passed once and
failed the round-trip test the next time, so the suite's Miri verdict was
down to the layout Miri picked: a flaky test, not a wrapper defect (the
wrapper stores and returns exactly the pointer it was given, as the fixed
tests still check).

Fix: the round-trip test compares `into_raw()`'s pointers with the
pointers the descriptor's own getters return before the conversion, and
the strings' contents with the literals; the copy test compares the
copy's name pointer with the source descriptor's. Both pass under Miri
for seeds 0-5 (`miri-seeds-after.log`) and on every other build. The
fixed tests are the regression.
