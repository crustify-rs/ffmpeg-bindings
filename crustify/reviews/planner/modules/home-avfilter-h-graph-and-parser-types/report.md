# avfilter.h's graph and graph-parser types belong to avfiltergraph.c and graphparser.c

## What the earlier plan decided

Planner run 3 bundled `libavfilter/avfilter.h` with `libavfilter/avfilter.c` (libavfilter
module 1), which owned it but for 30 functions defined elsewhere, and so owned
`AVFilterGraph`, `AVFilterInOut`, `AVFilterPadParams`, `AVFilterParams`, `AVFilterChain` and
`AVFilterGraphSegment`, scheduled in its waves 570-580.

## Evidence against it

```console
$ git show 749e05641d:crustify/artifacts/modules.json > /tmp/m3.json
$ python3 crustify/reviews/planner/modules/home-avcodec-h-parser-and-hwaccel-types/type_homes.py \
    /tmp/m3.json AVFilterGraph AVFilterInOut AVFilterPadParams AVFilterParams AVFilterChain AVFilterGraphSegment
AVFilterGraph: owner libavfilter/avfilter.c; owner files name it 7x, access its fields 3x; field accesses by in-scope file: [('libavfilter/avfiltergraph.c', 65), ('libavfilter/graphparser.c', 14), ('libavfilter/formats.c', 6), ('libavfilter/avfilter.c', 3)]
AVFilterInOut: owner libavfilter/avfilter.c; owner files name it 0x, access its fields 0x; field accesses by in-scope file: [('libavfilter/graphparser.c', 48)]
AVFilterPadParams: owner libavfilter/avfilter.c; owner files name it 0x, access its fields 0x; field accesses by in-scope file: [('libavfilter/graphparser.c', 6)]
AVFilterParams: owner libavfilter/avfilter.c; owner files name it 0x, access its fields 0x; field accesses by in-scope file: [('libavfilter/graphparser.c', 84)]
AVFilterChain: owner libavfilter/avfilter.c; owner files name it 0x, access its fields 0x; field accesses by in-scope file: [('libavfilter/graphparser.c', 24)]
AVFilterGraphSegment: owner libavfilter/avfilter.c; owner files name it 0x, access its fields 0x; field accesses by in-scope file: [('libavfilter/graphparser.c', 42)]
```

- `AVFilterGraph`: `grep -c AVFilterGraph c/libavfilter/avfilter.c` prints 0 (the 7
  mentions are `avfilter_internal.h`'s derived `FFFilterGraph` and prototypes);
  `avfilter_graph_alloc`, `avfilter_graph_free` and `avfilter_graph_config` are
  `libavfilter/avfiltergraph.c`'s (`:85`, `:119`, `:1434`), with 65 of its field accesses.
- The other five are the graph parser's: `avfilter_inout_alloc` and `avfilter_inout_free`
  (`c/libavfilter/graphparser.c:71`, `:76`), `avfilter_graph_segment_parse` and
  `avfilter_graph_segment_free` (`:460`, `:276`), which build and release the segment, its
  chains, filter params and pad params; every field access is graphparser.c's.

## The edit and what it moves

- `modules.json`: `AVFilterGraph` is `included` in avfiltergraph.c's (3) `avfilter.h` entry,
  the five others in graphparser.c's (7); avfilter.c's entry `excludes` them. graphdump.c and
  graphparser.c now depend on avfiltergraph.c. `AVFilterContext.graph` and
  `AVFilterGraph.filters` make avfilter.c and avfiltergraph.c a cycle: avfilter.c (6 edges
  in, 1 out) runs first, and its dependency on avfiltergraph.c is cut. That edge is the
  graph's own cycle cut (`AVFilterContext`'s `fallback` on `AVFilterGraph`): wavefront
  orders `AVFilterContext` first and has it refer to `AVFilterGraph` raw until avfiltergraph.c
  back-fills it, so no pre-wave wraps `AVFilterGraph` before it, which would precede
  `AVFilterContext`, its own dependency. See `../wave-cuts-for-fallback-edges/report.md`.
- Configurations: `crustify/wavefront/configs/libavfilter/{1,3,7}/wavefront-config.json`.
- `schedule.json`: avfilter.c's waves 570-571 and 576-581 become 652-657 (`AVFilterPad`;
  `AVFilterLink` with the link and init functions; the remaining functions); avfiltergraph.c
  gains 658-659 (`AVFilterGraph`) before its kept wave 584; graphparser.c's 594-595 become
  660-669 (`AVFilterPadParams`; `AVFilterInOut`, `AVFilterParams`; `AVFilterChain` with the
  parse and in/out functions; `AVFilterGraphSegment`; the segment functions). The kept wave
  574 (`AVFilterContext`) records `cut: [654, 658]`.

Rerun: as in `../home-avcodec-h-parser-and-hwaccel-types/report.md`.
