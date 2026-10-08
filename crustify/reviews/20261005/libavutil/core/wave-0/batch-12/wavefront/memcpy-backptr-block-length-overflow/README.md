# Wavefront: `av_memcpy_backptr`'s finding missed the block-length overflow

- Symbol: `av_memcpy_backptr` (`libavutil/mem.c`), argument 0 (`dst`)
- Store: `crustify/wavefront/ownership-store.json`
- Resubmitted with `query symbols --name av_memcpy_backptr --update`

## Previous finding

> Writes dst[0..cnt) from dst[-back..]: the buffer must extend back bytes
> before dst (read even when cnt is 0 for back 1..4) and cnt bytes after;
> back/cnt non-negative. [symbol] LZ77 overlapping copy; back 0 is a no-op.

## Why it was incomplete

The bounds above are necessary but not sufficient. For `back > 4` and
`cnt >= 16`, the function copies doubling blocks with an `int` block length
(`mem.c:466`, `blocklen <<= 1`). Once a block of 2^30 bytes or more has been
copied and the run is not finished, the doubling overflows `int`; the
compiled library wraps it to `INT_MIN`, re-enters the loop and calls
`memcpy` with `(size_t)INT_MIN`. A call that satisfies the recorded
precondition in full, `dst - back` and `dst + cnt` inside the buffer with
`back` and `cnt` within `int`, is undefined:

```text
av_memcpy_backptr(buf + 2^30, 2^30, 2^30 + 1)   // buf of 2^31 + 1 bytes
src/libavutil/mem.c:466:26: runtime error: left shift of 1073741824 by 1 places cannot be represented in type 'int'
AddressSanitizer: negative-size-param: (size=-2147483648)
```

The translator relied on the finding when writing `copy_back_reference`, whose
checks are exactly the recorded ones, so the safe wrapper reached the
overflow (see `../../ub_safe/memcpy-backptr-block-length-overflow`).

## Corrected finding

The note now states the bound: with `B` the first `back << k` of at least
2^30, a run longer than `2 * B - back` bytes overflows, so a wrapper must
split longer runs into calls of at most that many bytes (`back` itself for
`back >= 2^30`); distances up to 4 copy without a block length. The
structural facets (by-value array, borrowed for the call, non-nullable,
mutable, no lifecycle role) were right and are unchanged.
