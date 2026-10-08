# Misc: a stale BSan skip left the grown-default path untested under BSan

- File: `crustify/rust/ffmpeg/libavutil/opt.rs` (`unit_tests`),
  `crustify/rust/ffmpeg/tests/common/opt.rs`
- Affected commit: `ad7acaeaca`
- Item: `unit_tests::grown_defaults_agree_with_av_opt_set_defaults`
  (`OptionDefRef::default_value` for `DICT` and array options against
  `av_opt_set_defaults2`)

## Finding

The test returned early under `CRUSTIFY_FFMPEG_VARIANT=bsan`, citing a false
use-after-free the BSan runtime reported inside `av_realloc` when
`av_opt_set_defaults2` builds a `DICT` or array default. That report came
from the runtime `crustify/build.json` describes (bsan 0.1.0 `3f210d63ae`).
The runtime installed since (bsan 0.1.0 `de6ede0149`, clang 23.1.3) no longer
reports it: the bsan-built test binary, run with the variant variable unset so
the skip does not fire, passes:

```text
$ env -u CRUSTIFY_FFMPEG_VARIANT BSAN_OPTIONS=gc_hang_threshold_ms=600000 \
    .../ffmpeg-109a12b950670f42 grown_defaults
test libavutil::opt::unit_tests::grown_defaults_agree_with_av_opt_set_defaults ... ok
```

So the skip only removed the `Array` and `DICT` `Str` default comparisons
from the BSan pass, and the fixture's `GROWS` doc explained the split by a
false report that no longer exists.

## Fix

The early return is removed; the test now runs under every variant. The
`GROWS` flag stays as the fixture's way to select the grown defaults apart
from the scalar ones with `av_opt_set_defaults2`, and its doc now says so.

## Regression evidence

`cargo bsan test -p ffmpeg --lib opt` (`CRUSTIFY_FFMPEG_VARIANT=bsan`) runs
the test and passes.
