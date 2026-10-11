# Stale and incomplete findings: `av_opt_set_array`, `av_opt_set_video_rate`, `av_dict_set_int`

Records submitted by options wave-1 translation batches 0 and 1; corrected
and resubmitted with `query symbols --name <n> --file <tu> --update <n>.json`
(the three JSON files here are the exact submissions; `ownership-store.json`
changes in these three records only).

## `av_opt_set_array` (`libavutil/opt.c`)

- `obj` (position 0): the note described the array field as "grown, shrunk
  or replaced ... releasing removed or replaced elements" but missed that
  the removal path releases and shifts *before* the shrinking
  `av_realloc_array`, and that the realloc's failure returns `ENOMEM`
  without storing the new count (`opt.c:2405-2427`), leaving slots that are
  owned twice or released: a double free at `av_opt_free`. Evidence: the
  reproducer `ub_unsafe/remove-array-shrink-failure-double-free` (ASan
  `attempting double-free`, `opt.c:132`). The note now records the hazard
  and the wrapper's repair (`ObjectMut::remove_array` writes the new count
  back after that `ENOMEM`).
- `val` (position 6): the note named the wrong Rust trait ("the sealed
  `opt::ArrayElement` trait (`T::C`, `T::TYPE`)"). `ArrayElement` is
  `av_opt_get_array`'s output trait (`TYPE`, `Raw`, `zero`, `adopt`); the
  inputs of `insert_array` / `replace_array` are `opt::ArrayInput`, whose
  sealed supertrait has `C` and `TYPE` (`opt.rs`, `mod array_input`). It also
  missed how C converts a `RATIONAL` value: `write_number(obj, o, dst, 1,
  den, num)` (`opt.c:2475-2485`), which stores `llrint(1 / den) * num` into
  an integer element, not the rounded quotient `av_opt_set_q` stores
  (`3/2` gives `0` in an `int` array and `2` in an `int` option; C probe and
  `tests/equiv/opt.rs::rational_array_conversion`). Both are corrected.

## `av_opt_set_video_rate` (`libavutil/opt.c`)

The `obj` note ended with "set_format converts the option's double bounds
to int (object contract keeps them in range)", copied from the pixel and
sample format setters. `av_opt_set_video_rate` does not call `set_format`:
it is `set_number(obj, name, val.num, val.den, 1, search_flags,
AV_OPT_TYPE_VIDEO_RATE)` (`opt.c:1008-1011`), whose `write_number` checks the
range in doubles (a zero `den` is `ERANGE`) and stores the `int` pair as is
(`(int) num == num` holds for an `int` numerator). The sentence is replaced
with that path.

## `av_dict_set_int` (`libavutil/dict.c`)

`key` (position 1) was recorded `owned: false`, although its own note says
that `AV_DICT_DONT_STRDUP_KEY` survives (`flags &= ~AV_DICT_DONT_STRDUP_VAL`
only, `dict.c:182`) and then transfers an `av_malloc`'d key to the
dictionary, exactly as for `av_dict_set`, whose `key` is recorded
`owned: true` with the same conditional note. Corrected to `owned: true`.
(The Rust `Dict::set_int` takes the key borrowed: `dict::Flags` cannot carry
the `DONT_STRDUP` bits, so the transfer never happens through it.)

The other 32 records of the workset were checked against the C bodies and
the wrappers and are accurate.
