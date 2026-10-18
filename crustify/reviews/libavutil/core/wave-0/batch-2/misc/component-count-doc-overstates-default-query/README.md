# Misc: `component_count` documented components the default query never returns

- File: `crustify/rust/ffmpeg/libavutil/opt.rs`
- Affected commit: `ad7acaeaca`
- Item: `OptionRangesRef::component_count` (`AVOptionRanges.nb_components`)

## Finding

The getter's doc read "1, or more for a multi-component query of a type such
as `IMAGE_SIZE` (pixel count, width, height)". That paraphrases the C
header's description of the *field*, but libavutil itself never produces
more than one component: `av_opt_query_ranges_default` always builds one
range of one component, for `IMAGE_SIZE` too, and `av_opt_query_ranges`
forces `nb_components = 1` unless `AV_OPT_MULTI_COMPONENT_RANGE` is set, in
which case it stores the class callback's return value. Only a class's
`query_ranges` callback can give several components, and the existing
`tests/equiv/opt.rs` shows the default `size` query returning one component
even with the flag. A reader of the doc would expect three components from
`query_default(.., c"size", MULTI)`.

## Fix

The doc now states the three cases: 1 from the default ranges; the callback's
return value for a query with `AV_OPT_MULTI_COMPONENT_RANGE` (3 for an image
size, say); 1 without the flag whatever the callback built. Behaviour is
unchanged; `tests/equiv/opt.rs::class_callback_lists_agree` and
`tests/ub_unsafe/opt.rs::query_default::callback_is_bypassed` already pin it.
