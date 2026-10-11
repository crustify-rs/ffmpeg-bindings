# Misc: `TypedRefStructPool`'s validity argument misstated C's zeroing under `ZERO_EVERY_TIME`

- File: `crustify/rust/ffmpeg/libavutil/refstruct.rs`, the `TypedRefStructPool`
  type documentation
- Affected commit: `0ea2c20f1f` (buffer-frame wave 1, symbol batch 1)

## Finding

`TypedRefStructPool::get` adopts C's entry as a `RefStructBox<T>`. Its
`// SAFETY:` comment says the entry holds "a valid `T`, as the type's
documentation shows". The type's documentation argued this as follows:

> Every entry it hands out is a valid `T`: C zeroes each entry it allocates,
> before the init callback runs, and an entry it reuses holds what its last
> user and the reset callback left.

That premise is false for `PoolFlags::ZERO_EVERY_TIME`, which both
constructors accept. With that flag, C allocates entries *unzeroed*
and zeroes every entry only as it hands it out:

```c
// libavutil/refstruct.c:370-373, av_refstruct_pool_alloc_ext_c
    if (flags & AV_REFSTRUCT_POOL_FLAG_ZERO_EVERY_TIME) {
        ...
        pool->entry_flags |= AV_REFSTRUCT_FLAG_NO_ZEROING;
// libavutil/refstruct.c:269 and 276, refstruct_pool_get_ext: the init callback sees those bytes
        ret = av_refstruct_alloc_ext(pool->size, pool->entry_flags, pool, ...);
            int err = pool->init_cb(pool->opaque, ret);
// libavutil/refstruct.c:289-290: zeroed only before the entry is returned
    if (pool->pool_flags & AV_REFSTRUCT_POOL_FLAG_ZERO_EVERY_TIME)
        memset(ret, 0, pool->size);
```

The code was correct. The `init_with` trampoline zeroes a new entry first
under that flag (`PoolState::zero_before_init`). The unit, equivalence and
`ub_safe` tests also cover it, including allocators that return dirty memory.
But the argument the `get` adoption cites did not describe that code path.
It also never mentioned the entries a failed init hands to the reset and
entry-freeing callbacks. Those callbacks are only sound because of the same
trampoline zeroing.

## Fix

The type documentation now splits the two cases. Without `ZERO_EVERY_TIME`,
C zeroes at allocation and reuse keeps what the last user and the reset left.
With it, C allocates unzeroed and zeroes at hand-out, and the init trampoline
zeroes a new entry before the init callback or a failed init's callbacks see
it. The claim now covers every entry the callbacks see, not only the
returned ones. Documentation only; no behaviour changes.
