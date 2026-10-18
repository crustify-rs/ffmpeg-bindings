# Conventions: `strncasecmp` kept its C name beside `cmp_ignore_ascii_case`

- File: `crustify/rust/ffmpeg/libavutil/avstring.rs`
- Affected commit: `ccdbbc0a77` (core wave 0, batch 14); the sibling it
  diverges from landed in `35f30a7185` (batch 10, `av_strcasecmp`)
- Item: `av_strncasecmp` (symbol route, wrap)

## Finding

The naming convention asks the safe surface to "prefer the idiomatic Rust
term over a transliterated C one". Batch 10 wrapped `av_strcasecmp` as
`cmp_ignore_ascii_case` (after `u8::eq_ignore_ascii_case`) and listed the
module's case-insensitive helpers in the module docs by that vocabulary
(`strip_prefix_ignore_ascii_case`, `find_ignore_ascii_case`,
`replace_ignore_ascii_case`). Batch 14 wrapped the bounded variant of the
same comparison as `strncasecmp`, so the module offered
`cmp_ignore_ascii_case(a, b)` next to `strncasecmp(a, b, n)` for one
operation, and the module docs did not mention the bounded form at all. The
`strl*` family (`strlcpy`, `strlcat`, `strlcatf`, `strnlen`) is left as is:
those are the established names of those exact truncation semantics, with no
Rust term to prefer.

The same batch also omitted `#[must_use]` on its pure queries
(`strip_prefix`, `find`, `strnlen`, `to_ascii_uppercase`,
`to_ascii_lowercase`, `utf8_decode`), which every batch-10 query in the
module carries; a discarded comparison or search result compiled silently.

## Fix

`strncasecmp` is renamed `cmp_n_ignore_ascii_case`, its doc refers to
`cmp_ignore_ascii_case` for the shared folding rule and states the `n == 0`
case, the module docs list it among the case-insensitive helpers, and the
pure queries above get `#[must_use]`. The equivalence test
`helpers::cmp_n_ignore_ascii_case_orders_as_av_strncasecmp` and the
`ub_safe` suite follow the rename; no other consumer existed (the anchor
`/// Wraps: av_strncasecmp` keeps the C name traceable).
