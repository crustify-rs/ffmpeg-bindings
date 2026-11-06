# `index_by_name`'s doc described a narrower refusal than the guard makes

- File: `crustify/rust/ffmpeg/libavutil/channel_layout.rs`
- Affected commit: `e0a75b5d7f`
- Item: `ChannelLayoutRef::index_by_name` (`Wraps:
  av_channel_layout_index_from_string`), shared by `channel_by_name`

The doc said the lookup is `None` "when the search by name would meet a
map name with no NUL, which C would read past". The guard
(`searchable_for` → `names_terminated`) refuses the whole map whenever the
lookup compares names (an `@` with text after it) and *any* element's
name has no NUL, whether or not C's search would reach that element: C's
loop stops at the first match, and its `strcmp` stops at the first
differing byte, so with `FL@left` on a map whose element 0 is `FL@left`
and whose element 1 has no NUL, C answers 0 without reading element 1,
while the wrapper answers `None`. The refusal is sound and the cheapest
exact check, and only `ChannelCustom::from_raw` can put such a name in a
map, so the code stays; the doc now says what it does ("any name in the
map has no NUL ... the whole map is refused, whether or not the search
would reach that element"). The wavefront note for
`av_channel_layout_index_from_string` already says "names must be
NUL-terminated" for the whole map.
