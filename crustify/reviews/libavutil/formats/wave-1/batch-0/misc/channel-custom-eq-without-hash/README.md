# `ChannelCustom` is `Eq` but not `Hash`

Affected base: `0a2fbf9bb7` (the wave tip; `ChannelCustom` landed in
`fac2331574`, formats wave 1 batch 1). Home:
`crustify/rust/ffmpeg/libavutil/channel_layout.rs`.

`ChannelCustom` (`AVChannelCustom`) implements `PartialEq` and `Eq` over
the fields C observes: the id, the name up to its NUL, and the user
pointer's address. It implemented no `Hash`, although every other `Eq`
value type of the `formats` subsystem does: the enum newtypes (`Channel`,
`ChannelOrder`, `MatrixEncoding`, `PixelFormat`, ...), `ChannelName` (the
copy its own `name()` getter returns), and the two other wrapped structs
with a hand-written equality, `ComponentDescriptor` and
`PixelFormatDescriptor`, whose `Hash` was written beside their `Eq`.

Without it, a `HashSet<ChannelCustom>` or a `HashMap<ChannelCustom, _>` (a
de-duplicated channel map, a per-channel table keyed by the channel) does
not compile, while the same code over any sibling type does. The three
fields hash naturally and consistently with the equality: `Channel` and
`ChannelName` derive `Hash`, and a raw pointer hashes by address, which is
exactly what `PartialEq` compares for `opaque`.

An ergonomic defect, not a soundness one.

Fix: `impl Hash for ChannelCustom` over the same three fields the equality
compares (id, `name()` copy, `opaque()` address), so `a == b` implies equal
hashes, the bytes after the name's NUL included. Regression:
`channel_layout::unit_tests::channel_custom_hashes_as_it_compares` (equal
values with different post-NUL bytes hash alike and de-duplicate in a
`HashSet`; the id, name and pointer each distinguish; `default()` and
`zeroed()` are distinct members), and the trait assertion in
`channel_custom_is_a_plain_shareable_value` now requires `Hash`.
