# UB (unsafe API): a pool buffer resized in place under its pool entry

- Items: `av_buffer_pool_init2` (`BufferPool::with_allocator`, unsafe) and
  `av_buffer_realloc` (`UninitBuffer::resize`, `BufferReferenceMut::resize`,
  safe), both homed in `crustify/rust/ffmpeg/libavutil/buffer.rs`.
- Affected commit: the wave tip `83bd5f2e64`. The resize wrappers came from
  translation batch 1, `758666d156`.
- Instrument: ASan (heap-use-after-free). BSan reports the same access
  ("trying to access an allocation that has been freed"). Both traces are
  kept here as `asan.trace` and `bsan.trace`.
- Status: fixed in `buffer.rs` (`BufferReferenceMut::realloc_by_copy`).
  Regression test: `tests/ub_unsafe/buffer.rs::resizable_allocator_buffers_resize_by_copy`.

## Finding

`BufferPool::with_allocator`'s `# Safety` asks two things of the
allocator. Each buffer it returns must hold at least `size` bytes from the
start of its buffer's data. Neither the allocator nor those buffers'
releases may call into the pool. `BufferReference::alloc_resizable` meets
both: a fresh buffer of exactly `size` bytes whose window is its data, freed
with `av_free`. The reproducer passes `BufferReference::alloc_resizable`
itself as the allocator.

C's `pool_alloc_buffer` (`libavutil/buffer.c:361-388`) records the
allocator buffer's `data`, `opaque` and `free` in a new `BufferPoolEntry`.
It then points the buffer's `opaque` and `free` at the entry. It never
touches the buffer's `flags_internal`, so the
`BUFFER_FLAG_REALLOCATABLE` that `av_buffer_realloc` set on an empty slot
(`buffer.c:202`) survives. The first pool buffer is therefore sole,
writable and reallocatable, and its window starts at its data. That is
exactly the in-place branch of `av_buffer_realloc` (`buffer.c:209-230`):

```c
    tmp = av_realloc(buf->buffer->data, size);
    ...
    buf->buffer->data = buf->data = tmp;
```

`av_realloc` frees the 16-byte block (or shrinks it), but the entry still
records the old pointer. Dropping the buffer runs `pool_release_buffer`,
which lists the entry as free. The next `av_buffer_pool_get`
(`buffer.c:397-405`) builds a new buffer of `pool->size` bytes over the
freed block. `UninitBuffer::fill` then writes there, a heap use after free.
Had the scenario stopped earlier, `buffer_pool_flush` would have passed the
same freed block to the allocator buffer's `av_buffer_default_free` again,
a double free.

Only safe code follows the unsafe constructor: `get`, `resize`, `fill`,
`drop`. The unsafe call keeps its stated contract, so this is a defect of
the safe resize wrappers, not of the contract. C itself has the same hazard
for any custom allocator that returns a reallocatable buffer.
`av_buffer_pool_init`/`init2` do not document it.

## Reproducer

Built for ASan in this directory, the binary exits 1 (`main` re-runs the scenario in a child with `halt_on_error=1`):

```sh
CRUSTIFY_FFMPEG_VARIANT=asan-ubsan RUSTFLAGS=-Zsanitizer=address \
  cargo +nightly build -Zbuild-std --target x86_64-unknown-linux-gnu --offline
ASAN_SYMBOLIZER_PATH=/usr/lib/llvm-18/bin/llvm-symbolizer \
LD_LIBRARY_PATH=../../../../../../../../builds/asan-ubsan/libavutil \
  target/x86_64-unknown-linux-gnu/debug/pool-buffer-resized-in-place-under-its-entry
```

The trace from the affected commit, abridged (full text in `asan.trace`):

```
recycled buffer of 16 bytes; filling it
==ERROR: AddressSanitizer: heap-use-after-free on address 0x7091183e0030
WRITE of size 16 at 0x7091183e0030 thread T0
    #0 in __asan_memset
    #1 in core::ptr::write_bytes::<core::mem::maybe_uninit::MaybeUninit<u8>>
    #2 in <ffmpeg::libavutil::buffer::UninitBuffer>::fill   buffer.rs:1188:18
    #3 in pool_buffer_resized_in_place_under_its_entry::scenario   main.rs:41:16
freed by thread T0 here:
    #0 in realloc
    #1 in av_buffer_realloc   libavutil/buffer.c:224:11
    #2 in <ffmpeg::libavutil::buffer::BufferReferenceMut>::realloc   buffer.rs:1031:25
    #3 in <ffmpeg::libavutil::buffer::UninitBuffer>::resize   buffer.rs:1228:25
    #4 in pool_buffer_resized_in_place_under_its_entry::scenario   main.rs:36:11
previously allocated by thread T0 here:
    #0 in realloc
    #1 in av_buffer_realloc   libavutil/buffer.c:192:25
    #2 in <ffmpeg::libavutil::buffer::BufferReference>::alloc_resizable   buffer.rs:602:18
    #4 in pool_alloc_buffer   libavutil/buffer.c:368:26
    #5 in av_buffer_pool_get   libavutil/buffer.c:407:15
    #6 in <ffmpeg::libavutil::buffer::BufferPoolRef>::get   buffer.rs:323:55
SUMMARY: AddressSanitizer: heap-use-after-free ... in __asan_memset
```

On the fixed tree it prints `no report: the pool did not recycle a freed
block` and exits 0, both under ASan and under BSan.

## Fix

`BufferReferenceMut::realloc` (behind both `resize` wrappers) now runs C's
copy path itself whenever the buffer has an opaque, and also for the NULL
window it already handled. A pool always gives its buffers an opaque, its
entry. The only C code that sets the reallocatable flag, `av_buffer_realloc`
of an empty slot, creates a buffer without one. So a buffer with an opaque
is never safe to resize in place, and one without is never a pool's. The
copy is `alloc_resizable(len)`, then a copy of the first `min(len, old)`
bytes, then a swap into the same struct. That is C's own sequence for that
branch (`av_buffer_realloc(&new)`, `memcpy`, `buffer_replace`), so the
outcome C can observe is unchanged. `equiv/buffer.rs::mutation` still
compares owner-backed buffers (which carry an opaque) with C after every
resize. The pool buffer returns to its pool intact.

The hazard stays in C for routines that `av_buffer_realloc` a reference
handed to them. Within libavutil only `buffer.c` does that.
`av_grow_packet`/`av_packet_*` (libavcodec `packet.c:89,151`) and a few
codecs and demuxers do it too. Later wrappers that pass Rust references into
those routines must keep pool buffers out of them, or copy first.
