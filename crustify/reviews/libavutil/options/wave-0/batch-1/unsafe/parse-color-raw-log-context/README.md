# `parse_color_with_context` kept a raw `*mut c_void` log context

- File: `crustify/rust/ffmpeg/libavutil/parseutils.rs`
- Affected commit: `cde88fc249`
- Item: `av_parse_color`
- Smell: `crustify scan-unsafe` `unsafe_fns_pub_smell`, `raw_ptr_pub_smell`,
  `void_ptr_pub_smell` at `parseutils.rs:160`

## Finding

The logging-context variant of `av_parse_color` was

```rust
pub unsafe fn parse_color_with_context(color: &CStr, len: Option<usize>,
    log_ctx: *mut c_void) -> Result<[u8; 4], i32>
```

whose only obligation was the log context ("NULL or ... a live struct
whose first member is a `const AVClass *`"), kept raw "while logging
contexts have no safe handle". The same wave's batch 3 added that handle:
`opt::ObjectRef`, which `eval::parse_and_eval` and `ObjectRef::show` already
take as `Option<ObjectRef<'_>>` for exactly this role, and `BareObject`
for a Rust-built context. Its contract also needed tightening: logging
calls the class's `item_name` on the context, so "a struct whose first
member is a `const AVClass *`" is not enough when the callback reads more
(see `../../ub_unsafe/bare-object-foreign-callbacks-read-past-object`);
`ObjectRef`'s contract requires the callbacks to accept the object.

## Fix

`parse_color_with_context(color, len, log_ctx: Option<ObjectRef<'_>>)` is
safe. `None` is no context (as NULL, or a context whose class pointer is
NULL, was: `ObjectRef::from_ptr` returns `None` for both). The former
`tests/ub_unsafe/parseutils.rs` suite moved: its sweeps over classed,
classless, stacked and boxed contexts are in `tests/ub_unsafe/opt.rs`
(`log_contexts`, adopting the contexts with `ObjectRef::from_ptr`), and a
safe sweep with `BareObject` contexts is in `tests/ub_safe/parseutils.rs`
(`colors_with_log_contexts`). The `equiv` comparisons pass `None` or an
adopted context.

Outside this workset, the wave's type review (batch 0, `382c43ab9b`) made
`Expr::parse_with_context` (`av_expr_parse`) safe over `ObjectRef` the same
way; `OptionRanges::query` / `query_default` (`av_opt_query_ranges*`, type
item `AVOptionRanges`) keep a raw object seam.
