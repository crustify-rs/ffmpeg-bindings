# Wavefront: `av_refstruct_pool_get`'s return note ignored `ZERO_EVERY_TIME`

- Entity: symbol `av_refstruct_pool_get` (`libavutil/refstruct.c`), `ptr_ret`
- Earlier submission: buffer-frame wave 1, symbol batch 1 (`5804b1d443`)
- Resubmitted: `submission.json` here, through `query symbols --name
  av_refstruct_pool_get --update`. Only `ptr_ret.ptr.note` changes. Every
  ownership property, the `pool` argument and the lifetime role are kept
  verbatim.

## Finding

The note described the returned entry's bytes as:

> Reused entries hold what the last user/reset left; fresh ones zeroed
> unless NO_ZEROING.

That is wrong for a pool made with `AV_REFSTRUCT_POOL_FLAG_ZERO_EVERY_TIME`.
`refstruct_pool_get_ext` zeroes every entry it returns, reused or new, after
`init_cb` or the last `reset_cb` (`refstruct.c:289-290`). So a reused entry
does not hold what its last user left. A fresh entry is allocated with
`NO_ZEROING` (`refstruct.c:370-373`), so `init_cb` sees uninitialised bytes
even though the flag set does not contain `NO_ZEROING`. Both paths are
reproducible through the existing suites. `tests/equiv/refstruct.rs::reuse_order_and_contents_match_c`
gets zeroes back where the note predicts `[12, 10]`. `tests/ub_safe/refstruct.rs::init_sees_zeroed_entries_where_c_leaves_them_dirty`
exists because C's `init_cb` sees dirty memory under that flag.

The sibling records `av_refstruct_pool_alloc_ext(_c)` already state "ZERO_EVERY_TIME
forces entry NO_ZEROING". `av_refstruct_pool_get`'s record is the one a
consumer of the return value reads, and it contradicted them.

## Correction

The note now says that under `ZERO_EVERY_TIME` every entry is memset to
zero right before it is returned, after `init_cb` or the last `reset_cb`. New
entries are allocated with `NO_ZEROING` there, so `init_cb` sees
uninitialised bytes. Otherwise, a reused entry holds what its last user and
`reset_cb` left, and a new one is zeroed before `init_cb` unless
`NO_ZEROING` is set. The Rust pointer adds that `NO_ZEROING` is stripped and
that `init_with` zeroes a new entry first under `ZERO_EVERY_TIME`.
