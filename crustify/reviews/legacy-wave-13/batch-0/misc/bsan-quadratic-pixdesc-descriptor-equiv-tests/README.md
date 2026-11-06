# Two `PixelFormatDescriptor` equivalence tests cost 28 minutes under BSan

Affected base: `0a2fbf9bb7` (the wave tip; the tests landed in
`287eeecb28`, formats wave 1 batch 2). File:
`crustify/rust/ffmpeg/tests/equiv/pixdesc.rs`, module
`pixel_format_descriptor`.

The wave's BSan gate (`review-wave-1/bsan-gate.log`) failed on the 7200 s
timeout of the `equiv` and `ub_safe` binaries. The orchestrator's triage
names only symbol-route tests (`imgutils::images`, `samplefmt::buffers`),
but this workset's `equiv` module held the next two heaviest tests, timed
alone on the BSan build (`BSAN_OPTIONS=gc_hang_threshold_ms=600000`, each
`cargo bsan test --test equiv -- --exact <name>`):

| test | alone, before | alone, after |
|---|---:|---:|
| `getters_read_what_c_stores_and_reports_for_every_entry` | 215 s | 7 s |
| `descriptors_rebuilt_through_setters_read_like_c_s_table` | 1432 s | 215 s |
| `a_heap_array_of_descriptors_set_whole_reads_like_c_s_table` | 16 s | 16 s |
| the five workset modules together (16 tests) | 1416 s | 224 s |

Inside the whole binary they are slower still (BSan serialises on its
global state, so parallel heavy tests inflate each other), and together
they ate about a quarter of the gate's budget.

Both are a BSan cost profile, not a defect in the wrapper and not UB (the
tests pass under every instrument, and BSan prints no report). The cost is
not where it looks: the ~5500 C observations (`av_read_image_line2`,
`av_write_image_line2`, the bit counts) of the rebuild test cost ~15 ms
each, as the heap-array test's 900 show. It is Rust iterating bytes:

- `getters_…` built a fresh four-plane image for every one of C's ~230
  table entries byte by byte (`(0..1536).map(xorshift).collect()`); under
  BSan each element push is a retag, ~0.85 s per image, ~200 s of the
  215 s.
- `descriptors_rebuilt_…` tallied the planes C wrote with
  `writes.iter().flatten().flatten().any(|&b| b != 0)`. Every `&u8` the
  slice iterator yields is a retag whose cost grows with the allocation's
  history, so a plane walked to its end costs seconds: the hwaccel formats
  (`vaapi`, `dxva2_vld`, ..., whose planes stay all-zero, so all 24 KiB are
  walked) took 26-47 s each in that one line, the `gbrp*` family
  (components on planes 2, 0, 1, so two empty planes are walked first)
  3.5 s each, and every other format 0.36 s. Measured with per-format
  timers around the rebuild, the tally and the drop; a cap on the number
  of formats showed the test linear in formats at ~1.2 s each until the
  first hwaccel entry.

Fix, in the tests only (the wrapper is untouched and every assertion is
kept): the image's bytes are a `const` table computed at compile time, so
`image()` is four block copies (`to_vec`), and the tally compares each
plane whole against a zero plane (`p[..] != ZERO_PLANE[..]`, one
`memcmp`-backed op). Plain, ASan/UBSan and TSan behaviour is unchanged
(same bytes, same seed, same observations, same counts); BSan timings
after the change are in the table above.

The same mechanism, in `Image::alloc`'s zero fill and the image and
sample sweeps of the symbol route, is the symbol review's
`../../../batch-1/misc/bsan-gate-per-byte-retags-in-image-alloc-and-sweeps`
(landed first); this report covers the type route's two tests only. The
shared test recipes gained the rule: under BSan never walk a buffer
element by element through a slice iterator (`iter().any`, `collect` from
a per-byte map, `for b in &v`); compare or copy whole slices.
