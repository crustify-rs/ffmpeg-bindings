# UB (safe API): copies of audio at `INT_MAX - 31` samples overflow C's sample rounding

- Items: `av_frame_make_writable` (`FrameMut::make_writable`), `av_frame_ref`
  (`FrameMut::try_clone_from`), `av_frame_replace` (`FrameMut::assign`, copy
  path), `av_frame_clone` (`FrameRef::try_clone`, from the wave-3 type run),
  all in `crustify/rust/ffmpeg/libavutil/frame.rs`. The root is
  `av_frame_get_buffer`, which C runs from each of them.
- Affected commit: the wave tip `2453c27798` (translation batch 0).
- Instrument: UBSan (asan-ubsan build), `signed-integer-overflow`. Trace in
  `ubsan.trace`.
- Status: fixed in `frame.rs`. Regression tests:
  `tests/ub_safe/frame.rs::frame_routines::copies_at_a_sample_count_c_cannot_round_stay_defined`
  (runs in a child with `UBSAN_OPTIONS=halt_on_error=1`, so the report fails
  the test) and the unit test
  `libavutil::frame::unit_tests::frame_copies_at_a_sample_count_c_cannot_round`.

## Finding

`av_samples_get_buffer_size` with `align == 0` rounds the sample count up to
a multiple of 32 (`libavutil/samplefmt.c:133-138`):

```c
    if (!align) {
        if (nb_samples > INT_MAX - 31)
            return AVERROR(EINVAL);
        align = 1;
        nb_samples = FFALIGN(nb_samples, 32);
    }
```

`FFALIGN(x, a)` is `((x)+(a)-1)&~((a)-1)`, so the sum is `nb_samples + 32`
before the `- 1`. The range check lets `INT_MAX - 31` through, and
`INT_MAX - 31 + 32` overflows `int`.

`av_frame_get_buffer(frame, 0)` reaches that call for an audio frame without
a line size (`get_audio_buffer`, `frame.c:156`). The translation guarded it in
`FrameMut::alloc_buffers`, which gives C the line size it would compute.
But C calls `av_frame_get_buffer(dst, 0)` itself in two other places:

- `av_frame_ref` for a source that is not reference counted (`frame.c:301`).
  `av_frame_clone` and the copy path of `av_frame_replace` go through it.
- The wrapper's own copy helper, `FrameRef::copy_into_zeroed_buffers`, which
  `make_writable` uses for every frame that is not writable.

None of those paths had the guard. All the safe API needs is a frame with
`set_sample_format`, `set_sample_count(i32::MAX - 31)` and a valid layout:

- `make_writable()` on that frame (it is not reference counted, so it is not
  writable), or on a reference-counted copy whose buffer is shared;
- `try_clone()` or `try_clone_from(src)` with it as the source.

The reproducer uses `make_writable` on a new U8 mono frame:

```text
src/libavutil/samplefmt.c:137:22: runtime error: signed integer overflow: 2147483616 + 32 cannot be represented in type 'int'
    #0 in av_samples_get_buffer_size samplefmt.c:137:22
    #1 in get_audio_buffer frame.c:156:15
    #2 in av_frame_get_buffer frame.c:215:16
    #3 in <ffmpeg::libavutil::frame::FrameRef>::copy_into_zeroed_buffers frame.rs:3709:23
    #4 in <ffmpeg::libavutil::frame::FrameMut>::make_writable frame.rs:5720:31
    #5 in audio_copy_sample_count_rounding_overflow::scenario main.rs:36:17
SUMMARY: UndefinedBehaviorSanitizer: undefined-behavior src/libavutil/samplefmt.c:137:22
```

On this build the overflow wraps, the rounded count comes back to
`INT_MAX - 31`, and C goes on to `ENOMEM` (2 GiB for U8 mono, beyond
`av_max_alloc`) or `EINVAL` (larger samples). The `ub_safe` suite tested
the boundary for `alloc_buffers` only. `make_writable` and the clone tests
used small sample counts.

## Reproducer

`cargo run` exits 1 on `2453c27798` and 0 on the fixed tree. `main` runs the
scenario again in a child with `UBSAN_OPTIONS=halt_on_error=1`, because
UBSan only prints its reports otherwise. Build it against the asan-ubsan C
build:

```sh
CRUSTIFY_FFMPEG_VARIANT=asan-ubsan RUSTFLAGS=-Zsanitizer=address \
  cargo +nightly run -Zbuild-std --target x86_64-unknown-linux-gnu
```

The binary needs the asan-ubsan libraries on `LD_LIBRARY_PATH`
(`crustify/builds/asan-ubsan/lib*`).

## Fix

- `FrameRef::sample_rounding_overflows` holds the shared predicate: audio as
  C tells it apart from video, a valid layout, and exactly `INT_MAX - 31`
  samples. `FrameRef::default_sample_linesize` computes the line size C would
  round to, through `samplefmt::samples_buffer_size`, which already handles
  this count.
- `alloc_buffers` and `copy_into_zeroed_buffers` (so `make_writable`) set
  that line size before calling `av_frame_get_buffer`. C uses a line size it
  is given as is and skips the rounding. Their results are C's: `ENOMEM` or
  `EINVAL`, as above.
- `av_frame_ref` allocates inside C, so the wrapper cannot pass it a line
  size. `try_clone`, `try_clone_from` and `assign` (copy path) now refuse a
  source that is not reference counted when it has this count, with
  `AVERROR(EINVAL)` (`FrameRef::copy_refusal`, which also covers the older
  `copy_reads_past_extended_data` refusal). C returns the same `EINVAL` after
  the overflow, unless it can allocate the whole copy. That needs lent planes
  of 2 GiB (unsafe `set_data_ptr`) and an `av_max_alloc` above `INT_MAX`.
  This divergence is documented on the three methods.

Frames that are reference counted are not affected: `av_frame_ref` only
adds references to their buffers.
