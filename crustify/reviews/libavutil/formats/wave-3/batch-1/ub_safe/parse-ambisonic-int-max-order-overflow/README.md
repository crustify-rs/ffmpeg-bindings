# `ChannelLayout::parse` reaches `av_channel_layout_from_string`'s `order + 1` overflow

- File: `crustify/rust/ffmpeg/libavutil/channel_layout.rs`
- Affected commit: `e0a75b5d7f` (formats wave-3 head at review time; the
  wrapper landed with the wave-2 type batch, `AVChannelLayout`, and is the
  wave-3 item `av_channel_layout_from_string`)
- Item: `ChannelLayout::parse` (`Wraps: av_channel_layout_from_string`)
- Sanitizer: UBSan, `signed integer overflow: 2147483647 + 1 cannot be
  represented in type 'int'` at `channel_layout.c:341:32`, then `:341:56`,
  `:346:46`, `:346:60` and `-2147483648 * -2147483648` at `:346:51` when
  it recovers
- Suite: `tests/ub_safe/channel_layout.rs`, `layout::parse_refuses_the_ambisonic_order_c_cannot_add_one_to`
  (and `unit_tests::parse_refuses_the_ambisonic_order_c_adds_one_to_at_int_max`)

## Finding

`av_channel_layout_from_string` reads an `ambisonic <order>` description
with `strtol` into an `int` and range-checks the order before squaring it:

```c
order = strtol(p, &endptr, 0);
if (order < 0 || order + 1  > INT_MAX / (order + 1) ||      /* channel_layout.c:341 */
    (*endptr && *endptr != '+'))
    return AVERROR(EINVAL);

channel_layout->order       = AV_CHANNEL_ORDER_AMBISONIC;
channel_layout->nb_channels = (order + 1) * (order + 1);   /* channel_layout.c:346 */
```

The check guards the square but not its own `order + 1`: for `order ==
INT_MAX` that sum overflows, which is undefined. In the compiled library
it wraps to `INT_MIN`, `INT_MAX / INT_MIN` is 0, `INT_MIN > 0` is false, so
C goes on, squares `INT_MIN` (overflowing again to 0) and returns success
with an ambisonic layout of **no channels**. With extra channels after a
`+` the same layout gets the extra's mask and count (`ambisonic
2147483647+stereo` is "ambisonic order -1 with FL and FR"); as the extra of
an outer valid order (`ambisonic 1+ambisonic 2147483647`) the empty
ambisonic extra is folded into a custom map that then canonicalises to
`ambisonic 1`.

`strtol` reads any spelling the C locale accepts, and C narrows the
`long` to an `int` modulo 2^32, so every one of these reaches it:
`2147483647`, `0x7fffffff`, `017777777777`, ` +2147483647`,
`-2147483649`, `6442450943`. C reads one order per `ambisonic ` segment of
the `+`-chain, recursing into the extra after a valid order, so the
overflowing segment can sit anywhere in the chain whose earlier orders are
in range. The wrapper passed every description to C unchanged:

```rust
let layout = ChannelLayout::parse(c"ambisonic 2147483647").unwrap();
assert_eq!(layout.as_ref().channel_count(), 0);   // on the affected commit
```

The landed suites' descriptions stop at `ambisonic 46339`/`46340` (the
largest order whose square fits, and the first C refuses) and the `int`
bound of the *total* count (`ambisonic 46339+88047C`); none spells the
order itself at `INT_MAX`.

## Fix

`parse` mirrors `strtol(s, &end, 0)` (`c_strtol`: white space, sign, the
`0x`/`0` base prefixes, saturation at `long`'s bounds, the end index;
checked against glibc's `strtol` on 20 000 random strings in
`unit_tests::c_strtol_matches_libc_strtol`) and walks the `+`-chain as C
does (`ambisonic_order_overflows`): a segment whose order narrows to
`INT_MAX` is refused with `AVERROR(EINVAL)` before C is called, the same
answer C gives for `2147483646`, the next order down. Orders C's range
check refuses, and spellings C narrows to a valid order (`4294967297` is
order 1), still go to C; the latter are now in both suites' description
lists so the narrowing stays compared with C's. Documented in `parse`'s
`# Errors`.

## Reproduce

The sanitized libraries are not on the binary's rpath, so put the
build's `lib*` directories on `LD_LIBRARY_PATH`. `Cargo.lock` is seeded
from `crustify/rust/Cargo.lock`, so `--offline` resolves. UBSan recovers by
default; `main` re-runs itself with `UBSAN_OPTIONS=halt_on_error=1` and
exits with the child's status.

```console
$ B=$(git rev-parse --show-toplevel)/crustify/builds/asan-ubsan
$ CRUSTIFY_FFMPEG_VARIANT=asan-ubsan RUSTFLAGS=-Zsanitizer=address \
  cargo +nightly build --offline -Zbuild-std --target x86_64-unknown-linux-gnu
$ LD_LIBRARY_PATH=$B/libavutil:$B/libavcodec:$B/libavformat:$B/libavfilter:$B/libavdevice:$B/libswscale:$B/libswresample \
  UBSAN_OPTIONS=halt_on_error=1:print_stacktrace=1 \
  target/x86_64-unknown-linux-gnu/debug/parse-ambisonic-int-max-order-overflow
src/libavutil/channel_layout.c:341:32: runtime error: signed integer overflow: 2147483647 + 1 cannot be represented in type 'int'
    #0 0x72dfa6da1c1b in av_channel_layout_from_string /target/crustify/builds/asan-ubsan/src/libavutil/channel_layout.c:341:32
    #1 0x6496dd03d1ca in ffmpeg::libavutil::channel_layout::ChannelLayout::parse::{closure#0} crustify/rust/ffmpeg/libavutil/channel_layout.rs:1455:38
    #2 0x6496dd03cee6 in ffmpeg::libavutil::channel_layout::ChannelLayout::init_with crustify/rust/ffmpeg/libavutil/channel_layout.rs:1381:19
    #3 0x6496dd03f279 in ffmpeg::libavutil::channel_layout::ChannelLayout::parse crustify/rust/ffmpeg/libavutil/channel_layout.rs:1455:9
    #4 0x6496dd03945e in parse_ambisonic_int_max_order_overflow::main src/main.rs:37:15
$ echo $?
1
```

(The frames were symbolized with `llvm-symbolizer-18 --obj=<binary>
<offset>`; the runtime prints offsets only.) Without `halt_on_error`, C
recovers and the program prints what each description became:

```
src/libavutil/channel_layout.c:341:32: runtime error: signed integer overflow: 2147483647 + 1 cannot be represented in type 'int'
src/libavutil/channel_layout.c:341:56: runtime error: signed integer overflow: 2147483647 + 1 cannot be represented in type 'int'
src/libavutil/channel_layout.c:346:46: runtime error: signed integer overflow: 2147483647 + 1 cannot be represented in type 'int'
src/libavutil/channel_layout.c:346:60: runtime error: signed integer overflow: 2147483647 + 1 cannot be represented in type 'int'
src/libavutil/channel_layout.c:346:51: runtime error: signed integer overflow: -2147483648 * -2147483648 cannot be represented in type 'int'
"ambisonic 2147483647": accepted as ChannelLayout { order: AMBISONIC, nb_channels: 0, details: Ambisonic(0x0), opaque: 0x0 }
"ambisonic 0x7fffffff": accepted as ChannelLayout { order: AMBISONIC, nb_channels: 0, details: Ambisonic(0x0), opaque: 0x0 }
"ambisonic  +2147483647": accepted as ChannelLayout { order: AMBISONIC, nb_channels: 0, details: Ambisonic(0x0), opaque: 0x0 }
"ambisonic -2147483649": accepted as ChannelLayout { order: AMBISONIC, nb_channels: 0, details: Ambisonic(0x0), opaque: 0x0 }
"ambisonic 2147483647+stereo": accepted as ChannelLayout { order: AMBISONIC, nb_channels: 2, details: Ambisonic(0x3), opaque: 0x0 }
"ambisonic 1+ambisonic 2147483647": accepted as ChannelLayout { order: CUSTOM, nb_channels: 4, details: Custom([ChannelCustom { id: AMBISONIC(0), ... }, ..., ChannelCustom { id: AMBISONIC(3), ... }]), opaque: 0x0 }
```

On the fixed tree the same binary prints `refused with -22` for all six
and exits 0.
