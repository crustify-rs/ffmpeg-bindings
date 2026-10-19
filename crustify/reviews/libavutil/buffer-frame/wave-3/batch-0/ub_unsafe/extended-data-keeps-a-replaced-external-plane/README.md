# UB (unsafe API): `extended_data` keeps a lent plane after it is replaced

- Items: `AVFrame` (`FrameMut::alloc_extended_data`, safe;
  `FrameMut::set_data_ptr`, unsafe; `FrameRef::try_clone`, safe), homed in
  `crustify/rust/ffmpeg/libavutil/frame.rs`.
- Affected commit: the wave tip `4f959b26a4` (translation batch 0).
- Instrument: ASan (heap-use-after-free). BSan reports the same access
  ("trying to access an allocation that has been freed"). Both traces are
  kept here as `asan.trace` and `bsan.trace`.
- Status: fixed in `frame.rs` (`alloc_extended_data` copies `data` only
  into the array of a reference-counted frame). Regression test:
  `tests/ub_unsafe/frame.rs::frame_type::extended_arrays_keep_no_lent_planes`.
  Unit test: `frame.rs::unit_tests::frame_extended_arrays_copy_only_counted_planes`.

## Finding

`set_data_ptr` lends caller memory to a frame that is not reference
counted. Its `# Safety` asks the memory to stay valid "until the plane is
cleared or replaced or the frame reset or dropped". `alloc_extended_data`
gave the frame a separate `extended_data` array and copied `data` into its
first entries, lent planes included. Those copies are not the plane.
Replacing or clearing plane `i` (`set_data_ptr`, `clear_plane`,
`set_plane`) ends the loan, but leaves `extended_data[i]` pointing at the
old memory. Buffer changes prune only planes of a counted frame, so nothing
cleared the copy.

The reproducer lends two U8P channels, allocates the array, replaces
channel 0's plane with a new block and frees the old one, as the contract
allows. It then clones. The frame is not counted, so `av_frame_clone`
copies the data through `frame_copy_audio`, which reads the channels from
`extended_data` (`frame.c:701-706`). `av_samples_copy` reads the freed
block.

The copy bit in the other direction too. An entry lent with
`set_extended_data_ptr` while `extended_data` is `data` is stored in
`data[i]`. Copied into a new array and cleared there, it stayed in
`data[i]`, and the video branch of a later copy reads `data`.

## Reproducer

Built for ASan in this directory, the binary exits 1:

```sh
CRUSTIFY_FFMPEG_VARIANT=asan-ubsan RUSTFLAGS=-Zsanitizer=address \
  cargo +nightly build -Zbuild-std --target x86_64-unknown-linux-gnu --offline
ASAN_SYMBOLIZER_PATH=/usr/lib/llvm-18/bin/llvm-symbolizer \
LD_LIBRARY_PATH=../../../../../../../../builds/asan-ubsan/libavutil \
  target/x86_64-unknown-linux-gnu/debug/extended-data-keeps-a-replaced-external-plane
```

The trace from the affected commit, abridged (full text in `asan.trace`):

```
cloning after replacing plane 0 and freeing its old block
==ERROR: AddressSanitizer: heap-use-after-free on address 0x7b7463fe0020
READ of size 64 at 0x7b7463fe0020 thread T0
    #0 in __asan_memcpy
    #1 in av_samples_copy   libavutil/samplefmt.c:237:13
    #2 in frame_copy_audio   libavutil/frame.c:705:5
    #3 in av_frame_copy   libavutil/frame.c:720:16
    #4 in av_frame_ref   libavutil/frame.c:305:15
    #5 in av_frame_clone   libavutil/frame.c:490:9
    #6 in <ffmpeg::libavutil::frame::FrameRef>::try_clone   frame.rs:3565:33
    #7 in extended_data_keeps_a_replaced_external_plane::scenario   main.rs:47:31
freed by thread T0 here:
    #0 in free
    ...
    #4 in <alloc::raw_vec::RawVec<u8> as core::ops::drop::Drop>::drop
    #5 in extended_data_keeps_a_replaced_external_plane::main   (drop(left), inlined)
SUMMARY: AddressSanitizer: heap-use-after-free ... in __asan_memcpy
```

BSan: `error: Undefined Behavior: trying to access an allocation that has
been freed.` at `libavutil/samplefmt.c:237:13` (`av_samples_copy`).

On the fixed tree it prints `no report: the clone was refused` (both array
entries are NULL, so C refuses to copy) and exits 0, under ASan and BSan.

## Fix

`alloc_extended_data` copies `data` into the new array only while `buf[0]`
holds a reference. Then every non-NULL plane points into the frame's
buffers, and buffer changes prune both arrays. A frame that is not counted
gets an all-NULL array, and the caller lends each entry where it is
stored, with `set_extended_data_ptr`. Each lent pointer then lives in
exactly one slot, so "until the plane is cleared or replaced" is the whole
story. The docs of `alloc_extended_data`, `set_data_ptr` and
`set_extended_data_ptr` say so. The last now states that an entry lent while
`extended_data` is `data` is plane `i`, whose loan ends with that plane, not
with an entry of a later array. The
random equivalence sequence's C mirror of the step follows the same rule.
Its frames never lend planes, so its outcomes are unchanged.
