# Log context findings omit that logging calls the class's callbacks on the context

- Entities: `av_parse_color` (arg 3 `log_ctx`), `av_opt_show2` (arg 1
  `av_log_obj`), `av_expr_parse_and_eval` (arg 10 `log_ctx`)
- Submissions: `av_parse_color.json`, `av_opt_show2.json`,
  `av_expr_parse_and_eval.json` (merged with `query symbols --update`)

## Previous finding

- `av_parse_color.log_ctx`: "NULL means no context; otherwise a struct whose
  first member is a const AVClass * (type-erased)."
- `av_opt_show2.av_log_obj`: "Log context (AVClass-bearing object) for the
  listing, read by av_log during the call; NULL for none."
- `av_expr_parse_and_eval.log_ctx`: "Parent log context (an AVClass-bearing
  object) of the parser's "Eval" context, read by av_log during the call
  only; NULL for none."

## Evidence

`av_log` → `av_log_default_callback` → `format_line` prints the prefix
with the context's class callbacks, called on the context itself, and does
the same for a parent context:

```c
av_bprintf(part+0, "[%s @ %p] ", item_name(parent, *parent), parent);   /* log.c:337 */
if(type) type[0] = get_category(parent);
...
av_bprintf(part+1, "[%s @ %p] ", item_name(avcl, avc), avcl);           /* log.c:342 */
if(type) type[1] = get_category(avcl);                                   /* -> avc->get_category(ptr) */
```

A class's `item_name` may read its object (`hwdevice_ctx_class`'s returns
`ctx->hw_type->name`), so a context that is only "a struct whose first
member is a const AVClass *" is not enough: it must be an object the
class's callbacks accept. The wrappers' `BareObject`, an 8-byte object of
any option-less class, relied on the weaker reading and let such a callback
read past it (`../../ub_unsafe/bare-object-foreign-callbacks-read-past-object`,
ASan heap-buffer-overflow in `item_name` from `log.c:343`).

## Corrected finding

Each note now states that the default callback calls the class's
`item_name` and `get_category` on the context (and on the parser's parent,
for `av_expr_parse_and_eval`), so the context must be a live object of its
class unless the class's `item_name` is NULL or `av_default_item_name` and
it has no `get_category`, and names the safe wrapper type
(`Option<opt::ObjectRef>`). Pointer facets (borrowed for the call,
nullable, not mutable) are unchanged.
