# Unsafe: `Expr::parse_with_context` kept a raw `void *` log context after a safe handle landed

- File: `crustify/rust/ffmpeg/libavutil/eval.rs`
- Affected commit: `66fa3604d1` (options wave 0, batch 1); still at the
  reviewed wave tip `cde88fc249`
- Item: `AVExpr`'s constructor `Expr::parse_with_context` (`av_expr_parse`)
- Kind: public `unsafe fn` and raw pointer that a safe handle now replaces
  (`crustify scan-unsafe`: `unsafe_fns_pub_smell`, `raw_ptr_pub_smell` and
  `void_ptr_pub_smell` at `eval.rs:214`)

## Finding

Batch 1 wrapped `AVExpr` with two constructors: the safe `Expr::parse`, and
`pub unsafe fn parse_with_context(..., log_ctx: *mut c_void)`, documented as a
raw seam "while logging contexts have no safe handle". Batch 3 of the same
wave landed that handle: `opt::ObjectRef`, the borrowed handle over an
`AVClass`-enabled object, whose contract (a live object led by a non-NULL,
well-formed class, unwritten for the borrow) covers everything `av_log`
reads from a parent context: the class pointer, `class_name` /
`item_name`, and the `parent_log_context_offset` / `log_level_offset_offset`
fields. `eval::parse_and_eval`, in the same module, already takes its parent
as `Option<ObjectRef<'_>>` for exactly the same C use (`av_expr_parse`'s
`log_ctx`, passed through `av_expr_parse_and_eval`).

So the type's constructor still made every caller with a context write
`unsafe`, under a hand-stated obligation, although the safe crate could
express it. The batches ran in parallel, so batch 1 could not have known.

The old contract also admitted a context whose class pointer is NULL. C
treats such a parent exactly as no parent (`format_line` checks
`parent && *parent`), so `None` covers it, and `ObjectRef::from_ptr` returns
`None` for it.

## Fix

`parse_with_context` is now a safe `pub fn` taking
`log_ctx: Option<ObjectRef<'_>>`; `None` is no parent. It passes
`ObjectRef::as_ptr()` (or NULL) to C under `ObjectRef`'s contract, and
`Expr::parse` calls it with `None` instead of a NULL pointer in an `unsafe`
block. The module docs no longer list the log context as a raw seam; the
opaque pointer user functions receive remains the only one. A caller
holding a raw C context adopts it once with the existing
`ObjectRef::from_ptr` seam. The expression still keeps no pointer to the
context, so the result borrows nothing from it.

## Regression evidence

- The workset no longer adds public unsafe API in `eval.rs`, so
  `tests/ub_unsafe/eval.rs` is gone and its suite moved to
  `tests/ub_safe/eval.rs::parse_with_context` (no parent; a `BareObject`
  parent of a `Class::new` class; a parent dropped right after the parse,
  outlived by the tree; parents on four threads over one class), at offsets
  from `INT_MIN` to the overflow bound and past it, on valid and invalid
  expressions.
- `tests/equiv/eval.rs::parse_with_context::parses_log_as_c_under_each_parent`:
  for no parent, a parent with a NULL class (C) against `None` (Rust), and a
  parent with a class adopted as an `ObjectRef`, seven expressions at seven
  offsets, the result and every captured message (level, class, formatted
  line with addresses masked) agree with `av_expr_parse` on the same parent;
  it also asserts that the failing parses at logging offsets did log, prefixed
  by the parent exactly when there is one.
