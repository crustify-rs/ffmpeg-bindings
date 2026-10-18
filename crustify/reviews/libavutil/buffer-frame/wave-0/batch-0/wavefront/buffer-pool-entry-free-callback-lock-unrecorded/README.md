# Wavefront: `AVBufferPool.alloc`/`alloc2` omitted that the result's free callback runs under the pool lock

- Entity: type `AVBufferPool` (`libavutil/buffer_internal.h`), fields `alloc`
  and `alloc2`
- Earlier submission: buffer-frame wave 0, type batch 1 (`c68b39206c`)
- Resubmitted: `submission.json` here, through `query types --name
  AVBufferPool --update`. Only the two fields' `note`s change; every ownership
  property, the other fields and `_comment_agent` are kept verbatim.

## Finding

Both notes gave the allocator's result contract as "a sole, writable
reference whose AVBuffer data holds at least size bytes". That is everything
the pool requires of the *reference*. It leaves out the callback the pool keeps
from it. `pool_alloc_buffer` saves the result's own
`ret->buffer->free`/`opaque` in the `BufferPoolEntry`, and `buffer_pool_flush`
calls it when it frees the entry:

```c
// libavutil/buffer.c:308, buffer_pool_flush
        buf->free(buf->opaque, buf->data);
// libavutil/buffer.c:337-339, av_buffer_pool_uninit
    ff_mutex_lock(&pool->mutex);
    buffer_pool_flush(pool);
    ff_mutex_unlock(&pool->mutex);
// libavutil/buffer.c:317-325, buffer_pool_free: flush, then ff_mutex_destroy and av_freep(&pool)
```

So that free callback runs with the pool mutex held (when the caller's
reference is released), or while the pool is being freed. Either way it must not
lock the pool itself. The record said nothing of this, and the `# Safety` of the
wrapper built from it (`BufferPool::with_allocator`) missed the same
obligation. A free callback that keeps that contract and returns a buffer of
the same pool deadlocks in `pool_release_buffer`:
`../../unsafe/buffer-pool-allocator-free-callback-under-pool-lock` has the
reproducer and the stack.

## Correction

Each note now ends with: the result's AVBuffer keeps its own `free(opaque,
data)` in the `BufferPoolEntry`; `buffer_pool_flush` calls it under the pool
mutex from `av_buffer_pool_uninit` and unlocked from `buffer_pool_free`; that
callback must therefore not lock the pool (no `av_buffer_pool_get`, no
release of the last reference to one of its buffers, no
`av_buffer_pool_uninit`). The `pool` field's `locked_by` and the type comment
("flushes free entries under the mutex") were already right.
