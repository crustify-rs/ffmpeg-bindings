# `copy_back_reference` reaches `av_memcpy_backptr`'s block-length overflow

- File: `crustify/rust/ffmpeg/libavutil/mem.rs`
- Affected commit: `3770f8f483` (wave-0 head at review time)
- Item: `copy_back_reference` (`Wraps: av_memcpy_backptr`)
- Sanitizers: UBSan (`left shift of 1073741824 by 1 places cannot be
  represented in type 'int'`), then ASan (`negative-size-param` in
  `memcpy`); the plain build dies with SIGSEGV
- Suite: `tests/ub_safe/mem.rs`, `back_references_past_the_doubling_bound_stay_inside`

## Finding

`av_memcpy_backptr(dst, back, cnt)` fills a run of `cnt` bytes from the
bytes `back` positions earlier. For `back > 4` and `cnt >= 16` it copies
doubling blocks:

```c
int blocklen = back;
while (cnt > blocklen) {
    memcpy(dst, src, blocklen);
    dst       += blocklen;
    cnt       -= blocklen;
    blocklen <<= 1;          /* mem.c:466 */
}
memcpy(dst, src, cnt);
```

`blocklen` is an `int`. Once a block of 2^30 bytes or more has been copied
and some of the run remains, `blocklen <<= 1` overflows: signed overflow is
undefined, and in the compiled library the value wraps to `INT_MIN`, so the
loop condition `cnt > blocklen` holds again and `memcpy` runs with
`(size_t)INT_MIN`, 2^63 bytes. The wrapper's checks (`back <= dst`,
`dst + len <= buf.len()`, both within `c_int`) all pass for such a call, so
safe code reaches it:

```rust
let back = 1 << 30;
let len = back + 1;
let mut buf = vec![0u8; back + len];           // 2 GiB + 1
copy_back_reference(&mut buf, back, back, len) // one block of 2^30, one byte left
```

Exactly: for a distance `back > 4`, let `B` be the first `back << k` of at
least 2^30. The block `B` is doubled when the run exceeds `2 * B - back`
bytes, and that doubling overflows. Every run of at most `2 * B - back`
bytes stays within `int`; for `back >= 2^30` that bound is `back` itself,
and for `back = 5` it is above `INT_MAX`, so the smallest affected buffer is
2 GiB + 1 (`back = 2^30`, `len = 2^30 + 1`).

The existing suites exercised every fill path with buffers of at most a few
KiB, where the doubling stops long before 2^30.

## Reproduce

The sanitized libraries are not on the binary's rpath (`rustc-link-arg` of a
dependency's build script does not reach a downstream binary), so put the
build's `lib*` directories on `LD_LIBRARY_PATH`. `Cargo.lock` is seeded from
`crustify/rust/Cargo.lock`, so `--offline` resolves.

```console
$ B=$(git rev-parse --show-toplevel)/crustify/builds/asan-ubsan
$ CRUSTIFY_FFMPEG_VARIANT=asan-ubsan RUSTFLAGS=-Zsanitizer=address \
  cargo +nightly build --offline -Zbuild-std --target x86_64-unknown-linux-gnu
$ LD_LIBRARY_PATH=$B/libavutil:$B/libavcodec:$B/libavformat:$B/libavfilter:$B/libavdevice:$B/libswscale:$B/libswresample \
  target/x86_64-unknown-linux-gnu/debug/memcpy-backptr-block-length-overflow
src/libavutil/mem.c:466:26: runtime error: left shift of 1073741824 by 1 places cannot be represented in type 'int'
SUMMARY: UndefinedBehaviorSanitizer: undefined-behavior src/libavutil/mem.c:466:26
=================================================================
==1250250==ERROR: AddressSanitizer: negative-size-param: (size=-2147483648)
    #0 0x62a710b83084 in __asan_memcpy (.../memcpy-backptr-block-length-overflow+0x1f8084)
    #1 0x79d1c9a8c1af in av_memcpy_backptr /target/crustify/builds/asan-ubsan/src/libavutil/mem.c:463:17
    #2 0x62a710bb9129 in ffmpeg::libavutil::mem::copy_back_reference .../crustify/rust/ffmpeg/libavutil/mem.rs:1045:14
    #3 0x62a710bb87f7 in memcpy_backptr_block_length_overflow::main .../src/main.rs:20:5
...
SUMMARY: AddressSanitizer: negative-size-param (.../memcpy-backptr-block-length-overflow+0x1f8084)
==1250250==ABORTING
$ echo $?
1
```

Without sanitizers, against `crustify/builds/plain`, the 2^63-byte `memcpy`
runs off the mapping:

```console
$ cargo run --offline     # with the plain libraries on LD_LIBRARY_PATH
Segmentation fault (core dumped)
$ echo $?
139
```

Only the written gibibyte is ever touched; the 1 GiB source stays on zero
pages, so the reproducer needs about 1.2 GB of resident memory under ASan.

## Fix

`copy_back_reference` now fills a long run with several C calls, each of a
run within `max_back_reference_run(back)` (`2 * B - back` as above, or
`INT_MAX` for distances up to 4, which copy without a block length). A run
split at any point fills the same bytes, since every byte of a run is read
from before it: the sequence of calls is the byte-by-byte expansion the
function documents. The bound is exact, so no call C could handle is split.

Regression tests:

- `tests/ub_safe/mem.rs`: `back_references_past_the_doubling_bound_stay_inside`
  runs the reproducer's call (skipped under BSan and TSan, whose shadow of a
  2 GiB buffer is impractical; it takes about 1.4 s under the plain build);
- `mod unit_tests`: `max_back_reference_run_is_the_last_run_c_copies_without_overflow`
  checks the bound against a model of C's `int` loop at every edge, and
  `back_references_in_short_runs_expand_byte_by_byte` checks that splitting
  at every run length down to one byte matches the one-call expansion over
  every fill path.

Against the patched crate the reproducer prints `filled 1073741825 bytes at
distance 1073741824` and exits 0 under ASan/UBSan.
