# The record-context equivalence test adopts its fixtures under a contract they break

- Objective: `contracts` (a `// SAFETY:` justification whose invariant does not hold)
- Where: `tests/equiv/log.rs`, `record_context::contexts_read_as_c_reads_them` (wave 800,
  commit `c0f53d14a81`; lines 2074-2122 at the wave tip `3df5deb2cbe`)
- Fixed: yes, in this review's Rust-tree commit

## What is wrong

The test logs in four fixture objects, so that a Rust callback can read each one's
`Record::context` and compare it with what C reads. It adopts each object as an
`ObjectRef` (line 2112):

```rust
// SAFETY: a live object whose class's callbacks accept it, with
// no options or offsets; nothing writes it while it logs.
let o = unsafe { ObjectRef::from_ptr((&raw mut *obj).cast()) }.unwrap();
log::log_with_context(o, Level::INFO, c"context\n");
```

`ObjectRef::from_ptr` asks for `ObjectRef`'s contract (`opt.rs:1413-1440`). Two of the
fixtures break it, and the comment does not mention either clause:

- `unnamed` is a class whose `class_name` is NULL (line 2081). The contract asks for "a
  class valid for `ClassRef` that outlives the borrow **and has a name**".
- The fourth object's `get_category` callback, `own_category`, returns `46` (line 2100).
  The contract asks that `get_category` return "a category below `AV_CLASS_CATEGORY_NB`",
  and 46 is `AV_CLASS_CATEGORY_NB` itself. C's default callback indexes its colour table with that value
  ([`../log-context-requirements-omit-category-bound/`](../log-context-requirements-omit-category-bound/)).

So the justification for the `unsafe` block is false: the block reaches `log_with_context`,
whose own `// SAFETY:` relies on the handle's contract, with objects that do not meet it.

Nothing fails at run time. The test installs a Rust callback, so C's default callback never
sees these objects. The `own` class has version 0, so C's own `get_category` would not
call the callback anyway (`log.c:261-264`). Still, a test that breaks the contract it
claims sets the wrong example for the suite. Copied with the default callback installed
and a class of the current version, the same fixture reads past `color[]` in
`ansi_fputs`.

The test only needs what `av_log` asks of a context. `LogContext` was introduced to
express exactly that. But no public safe or unsafe constructor builds a `LogContext` from a
raw pointer, so the test reached for `ObjectRef`'s stronger seam.

## Fix

- C logs in each fixture directly, through `ffi::av_log` with a `"%s"` format, as any C
  caller would. The callback still receives each object as its `Record::context`, which is
  what the test compares. The `// SAFETY:` comment now states what `av_log` and
  `Record::context` require, and why `ObjectRef` is not used.
- The fourth object's category is `AV_CLASS_CATEGORY_DEVICE_OUTPUT` instead of `46`, so
  every `get_category` result is in range, as `LogContext`'s corrected requirements ask.
  Out-of-range values still pass through unchanged: the `unnamed` class's `category` field
  is 4242, which `default_category` and `LogContext::category` return as read
  (`av_default_get_category` does not check it).
- The now-unused `ObjectRef` import is removed.

The comparison is unchanged: the Rust side's address, name, category, default name and
default category of every context, against `c_read` on the raw object, plus the general
log's `None`.

## Rerun

```bash
git show 3df5deb2cbe:tests/equiv/log.rs | sed -n 2074,2122p
git show 3df5deb2cbe:src/libavutil/libavutil/opt.rs | sed -n 1413,1440p
cargo test --test equiv -- log::record_context
```
