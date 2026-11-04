# `LogContext`'s stated requirements omit the bound on `get_category`'s result

- Objective: `contracts` (type invariant that the safety comments of its users rely on)
- Where: `src/libavutil/libavutil/log.rs`, the doc comment of `LogContext` (wave 800,
  commit `c0f53d14a81`, lines 1111-1122 at the wave tip `3df5deb2cbe`)
- Fixed: yes, in this review's Rust-tree commit

## What is wrong

`LogContext` documents the invariant every handle upholds, and says that it is "`av_log`'s
own requirement of a context, which C relies on whenever it logs one, and `ObjectRef`'s
contract includes it". The list asks that the class's `item_name` and `get_category`
callbacks accept the object and do not log. It says nothing about the value `get_category`
returns.

C relies on that value. `av_log_default_callback` (`c/libavutil/log.c:379`) calls
`format_line`, which sets `type[1] = get_category(avcl)` (`log.c:344`). For a class of
version 51.59.100 or later whose own `category` is in range, the static `get_category`
(`log.c:259-270`) returns `avc->get_category(ptr) + 16` without checking it. The default
callback then passes it to `colored_fputs(type[1], ...)` (`log.c:425`), which indexes
`static const uint32_t color[16 + AV_CLASS_CATEGORY_NB]` (`log.c:98`, read by `ansi_fputs` at `log.c:194-207`)
whenever colour is on (a terminal on stderr, or `AV_LOG_FORCE_COLOR`). A callback returning
`AV_CLASS_CATEGORY_NB` or more is therefore an out-of-bounds read of a static array.

`ObjectRef`'s contract (`src/libavutil/libavutil/opt.rs:1426-1437`) has the clause: "the
latter returns a category below `AV_CLASS_CATEGORY_NB`". `LogContext`'s copy of the
logging part of that contract dropped it. So the claim that the list is "`av_log`'s own
requirement", which `log_with_context`'s `// SAFETY:` comment rests on ("the handle's
contract makes `obj` a logging context, which `av_vlog` and the default callback only
read"), is incomplete.

## Why this is not a UB finding

No safe path builds a `LogContext` whose `get_category` returns out of range:

- `From<ObjectRef>` requires `ObjectRef`'s contract, which has the clause;
- `Record::context` hands out the context C passed to the callback, which C's own default
  callback would index by in the same way; and
- `Class::new` builds classes without callbacks, and `ClassMut` has no setters.

The gap is in the stated invariant, which the private `LogContext::from_ptr` makes its
callers vouch for, and which `Record::context`'s `// SAFETY:` claims of every C context.
The missing-clause kind of `contracts` finding needs no reproducer (reviewing spec,
Reproducers).

## Fix

The clause is added to `LogContext`'s requirements, in the words `ObjectRef` uses:

> whose class's `item_name` and `get_category` callbacks accept the object and do not log,
> the latter returning a category below `AV_CLASS_CATEGORY_NB` (the default callback
> indexes its colour table with it), ...

`ObjectRef`'s contract still includes all of `LogContext`'s, so `From<ObjectRef>` stays
sound. The one in-tree context that broke the clause was a test fixture, which
[`../equiv-record-context-test-claims-objectref-contract/`](../equiv-record-context-test-claims-objectref-contract/)
reports and fixes.

## Rerun

Read `log.rs`'s `LogContext` doc next to `opt.rs`'s `ObjectRef` contract at the wave tip:

```bash
git show 3df5deb2cbe:src/libavutil/libavutil/log.rs | sed -n 1111,1122p
git show 3df5deb2cbe:src/libavutil/libavutil/opt.rs | sed -n 1426,1437p
sed -n 259,270p c/libavutil/log.c; sed -n 340,345p c/libavutil/log.c; sed -n 420,426p c/libavutil/log.c
```
