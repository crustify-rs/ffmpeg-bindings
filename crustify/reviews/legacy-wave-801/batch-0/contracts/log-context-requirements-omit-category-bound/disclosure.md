Subject: ffmpeg-bindings: `LogContext` contract omits the `get_category` range bound

Hi Marius,

While reviewing wave 800 of `ffmpeg-bindings` (commit `c0f53d14a81`), I found that the
documented invariant of the new `log::LogContext` handle is missing one requirement that
C relies on.

`LogContext` lists what every handle guarantees and calls the list "`av_log`'s own
requirement of a context". It asks that the class's `item_name` and `get_category`
callbacks accept the object and do not log, but it does not bound the category
`get_category` returns. FFmpeg's default log callback uses that value as an index:

```c
/* libavutil/log.c */
if(avc->get_category)
    return avc->get_category(ptr) + 16;          /* get_category(), :266-267 */
...
colored_fputs(type[1], 0, part[1].str);           /* :425 -> color[level], :194 */
```

`color` has `16 + AV_CLASS_CATEGORY_NB` entries, so a callback returning
`AV_CLASS_CATEGORY_NB` or more reads past it when colour output is on.
`opt::ObjectRef`'s contract already says "the latter returns a category below
`AV_CLASS_CATEGORY_NB`". `LogContext`'s copy of the logging part dropped that clause, while
`log_with_context`'s `// SAFETY:` comment relies on the list being complete.

No safe path reaches undefined behaviour today. A `LogContext` comes only from an
`ObjectRef`, which has the clause, or from a log callback's `Record`, which carries C's own
context. So this is a documentation defect in a safety contract rather than a soundness
bug, and there is no sanitizer trace.

The patch, in the same review, adds the clause to `LogContext`'s requirements in
`ObjectRef`'s wording. I can share the review report, with the exact source lines, on
request.

Found by [Crustify](https://github.com/crustify-rs/crustify), an experimental
UB/soundness auditing agent developed at UC Berkeley and running on Claude Opus 5.5, then
manually reviewed and independently reproduced.
