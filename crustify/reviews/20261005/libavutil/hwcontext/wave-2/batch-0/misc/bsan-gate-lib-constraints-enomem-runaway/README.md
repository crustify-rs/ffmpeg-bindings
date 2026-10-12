# Misc: the BSan gate's lib binary stopped at a runaway hwcontext test and at a known `strto*` false positive

- Tests:
  - `libavutil::hwcontext::unit_tests::device_context::constraints_keep_what_they_have_on_enomem`
    (hwcontext wave 1, batch 0).
  - `libavutil::channel_layout::unit_tests::channel_layout_c_initialisers_fill_a_fresh_layout`
    (formats wave 2, batch 0).
- Kind: BSan gate triage. Neither is a code defect.
- Status: the first test was thinned for every instrument; the second is
  skipped in the gate (`extra-skips.txt`, next to the options wave-2 skip
  list).

## Runaway: `constraints_keep_what_they_have_on_enomem`

This test checks that a failed `set_valid_sw_formats` keeps the old list.
It ran its own child process for more than 32 CPU minutes, reaching 40-42
GB RSS, on both the wave tip and the landed tree. It was the only test left
running when I stopped the child processes. The parent then reported the
test failed, which is how both trees show 953/955 passed and 1 failed in
that run. BSan reported nothing. The test built a list of 64 Ki formats
(`vec![..; 1 << 16]`). The wrapper scans that list for its terminator
(`iter().position`) before it allocates, and that scan costs one BSan
retag per element: the per-element retag cost class the campaign's BSan
notes describe. Plain, ASan and TSan run it in milliseconds.

The test needs only a list larger than its 1 KiB `av_max_alloc` limit, so
it now uses 512 formats (2 KiB). An assert pins the size, and the test is
unchanged otherwise. Alone under BSan it now takes 1 s. The allocation
still fails with `ENOMEM` under the limit, as before (checked under plain,
ASan/UBSan, TSan and BSan).

## False positive: `channel_layout_c_initialisers_fill_a_fresh_layout`

The test stops the lib binary within 90 s. It does so on both trees, and
also when run alone:

```
test libavutil::channel_layout::unit_tests::channel_layout_c_initialisers_fill_a_fresh_layout ... error: Undefined Behavior: an access of size 1b at offset 0xffffe19b960d8835 is out of bounds for alloc8805 of size 18b.
   --> /target/crustify/builds/bsan/src/libavutil/channel_layout.c:417:20
417 | if (!errno && !*end && !strchr(str, '-') && mask) {
0: av_channel_layout_from_string
```

The trigger is the test's `ChannelLayout::parse(c"FL+NOPE")`, a
description `parse_channel_list` rejects. That is the documented
stale-stack-shadow false positive under glibc `strtol`'s end pointer
(`../../../../formats/wave-2/batch-0/misc/bsan-stale-stack-shadow-under-glibc-strtol-end-pointers`):
uninstrumented glibc writes `end`, and the slot keeps an earlier frame's
provenance. The options wave-2 skip list does not name this test. The
buffer-frame wave-4 gate passed it, by stack layout. This gate skips it.
In the first loop, a script error also skipped
`channel_layout_ambisonic_setter_bounds_the_count_as_cs_parser`, which had
passed. The landed tree's final lib run includes it, and it passes.

## Gate figures

In the batch README's BSan section.
