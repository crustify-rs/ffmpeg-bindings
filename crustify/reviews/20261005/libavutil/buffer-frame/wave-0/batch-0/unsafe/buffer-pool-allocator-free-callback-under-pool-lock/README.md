# Unsafe: `with_allocator`'s contract let the allocator's buffers re-lock the pool

- File: `crustify/rust/ffmpeg/libavutil/buffer.rs`
- Affected commit: `c68b39206c` (buffer-frame wave 0, type batch 1), as landed
  on the wave branch at `2161c1d053`
- Item: `BufferPool::with_allocator` (`av_buffer_pool_init2`), the workset's
  one `pub unsafe fn` on a type (its raw `*mut AVBufferRef` return is a seam
  until wave 1 wraps `AVBufferRef`)
- Evidence: a self-deadlock (POSIX leaves a re-lock of a default mutex
  undefined). No sanitizer reports it: TSan's deadlock detector skips a mutex
  the thread already holds, and the thread then blocks in `futex`.

## Finding

`pool_alloc_buffer` keeps the allocator's original `free(opaque, data)` in
the pool entry, and `buffer_pool_flush` calls it when the entry is flushed.
`av_buffer_pool_uninit`, which `BufferPoolUninit` runs when the owner drops,
does that with the pool mutex held:

```c
// libavutil/buffer.c:337
    ff_mutex_lock(&pool->mutex);
    buffer_pool_flush(pool);          // buf->free(buf->opaque, buf->data)
    ff_mutex_unlock(&pool->mutex);
```

The `# Safety` section barred only `alloc` from calling into its own pool
("`alloc` must not call into this same pool: requesting one of its buffers,
releasing the last reference to one, or dropping its owner all lock the pool
mutex C holds during the call"). It asked of the returned buffers only that
their free callback be callable "on any thread, at any time until the pool is
freed". A free callback that returns a buffer of the same pool (or requests
one) meets every stated condition. When the owner is dropped,
`pool_release_buffer` then locks the mutex that `av_buffer_pool_uninit`
already holds on the same thread.

`src/main.rs` keeps the stated contract. Every allocator call wraps fresh
`av_malloc` bytes with `av_buffer_create`, so the reference is sole, writable
and of the pool's size, and `alloc` never touches the pool. The buffers' free
callback calls `av_free`, then releases one buffer reference the program owns,
which it got from the same pool. Two buffers are taken: `a` is returned to the
free list and `b` is kept for the callback. Dropping the owner then deadlocks:

```console
$ cargo run --offline
dropping the pool owner: av_buffer_pool_uninit flushes `a`
free callback: releasing the kept buffer of the same pool
deadlock: the free callback, run under the pool mutex by av_buffer_pool_uninit, locked it again to return the kept buffer
$ echo $?
1
```

The stuck thread, with `gdb -batch -ex 'break __lll_lock_wait' -ex run -ex 'bt 16'`
(addresses elided):

```text
#0  __GI___lll_lock_wait (futex=futex@entry=0x…, private=0) at ./nptl/lowlevellock.c:41
#1  0x… in lll_mutex_lock_optimized (mutex=0x…) at ./nptl/pthread_mutex_lock.c:48
#2  ___pthread_mutex_lock (mutex=mutex@entry=0x…) at ./nptl/pthread_mutex_lock.c:93
#3  0x… in pool_release_buffer (opaque=0x…, data=<optimized out>) at src/libavutil/buffer.c:350
#4  0x… in buffer_replace (dst=<optimized out>, src=0x0) at src/libavutil/buffer.c:133
#5  av_buffer_unref (buf=<optimized out>) at src/libavutil/buffer.c:144
#6  av_buffer_unref (buf=<optimized out>) at src/libavutil/buffer.c:139
#7  0x… in buffer_pool_allocator_free_callback_under_pool_lock::free_data (_opaque=0x0, data=0x…) at src/main.rs:49
#8  0x… in buffer_pool_flush (pool=0x…) at src/libavutil/buffer.c:308
#9  av_buffer_pool_uninit (ppool=<optimized out>) at src/libavutil/buffer.c:338
#10 0x… in ffmpeg::libavutil::buffer::{impl#11}::c_drop (self=0x…, ptr=...) at .../crustify/rust/ffmpeg/libavutil/buffer.rs:141
#11 0x… in ffibox::refs::{impl#7}::drop<ffmpeg::libavutil::buffer::BufferPool, ffmpeg::libavutil::buffer::BufferPoolUninit> (self=0x…) at /opt/venv/share/ffibox/src/refs.rs:511
...
#14 0x… in buffer_pool_allocator_free_callback_under_pool_lock::main::{closure#1} () at src/main.rs:92
```

The same program built against the `tsan` library (`CRUSTIFY_FFMPEG_VARIANT=tsan
RUSTFLAGS=-Zsanitizer=thread cargo +nightly run -Zbuild-std --target
x86_64-unknown-linux-gnu --offline`) deadlocks the same way without a report.

The pool's final release (`buffer_pool_free`, from the last returned buffer)
runs the same callbacks without the lock, while it frees the pool. A callback
that reached the pool there would be using a pool with no reference left, which
the C API already forbids, so the locked path above is the one the contract
missed.

The package depends on `libavutil-sys` directly only so its `build.rs` can
embed the selected variant's library directory as an rpath. `Cargo.lock` is
seeded from `crustify/rust/Cargo.lock`, so `--offline` resolves.

## Fix

`# Safety` now bars the free callbacks of the buffers `alloc` returns from
calling into the pool too, and says why: C holds the pool mutex during every
`alloc` call, and also while dropping the owner hands the pool's free buffers
to their free callbacks. The pool's final release runs those callbacks as it
frees the pool. This program breaks the new contract, so it keeps
deadlocking after the fix, which is expected for a contract fix.

C cannot avoid this, and the wrapper cannot either: the callback is the
caller's own C function. A safe `alloc` returning a safe `AVBufferRef` owner
(wave 1) still has to state the rule wherever user-supplied free callbacks
reach a pool.

Regression evidence within the new contract,
`tests/ub_unsafe/buffer.rs::free_callbacks_may_release_buffers_of_other_pools`:
the allocator's free callback returns a buffer of a *different* pool while
the owner's drop flushes it, on another thread, then the last outstanding
buffer frees the pool through the same callback. It passes under plain,
ASan/UBSan and TSan builds. The `ub_unsafe` module header now states the
narrowed contract its tests keep.

The wavefront finding for `AVBufferPool.alloc`/`alloc2` omitted the same
fact: see `../../wavefront/buffer-pool-entry-free-callback-lock-unrecorded`.
