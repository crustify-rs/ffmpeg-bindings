# bsf.h's filter-graph types belong to bitstreamfilter.c and bsfgraph.c

## What the earlier plan decided

Planner run 3 bundled `libavcodec/bsf.h` with `libavcodec/bsf.c` (libavcodec module 13), which
owned the header but for the functions defined elsewhere (`"excluded"`: 23 names), so it
owned `AVBitStreamFilterContext` and `AVBitStreamFilterGraph`. They were scheduled in bsf.c's
waves 468 and 470, after the pre-wave 464-465 that wrapped bitstreamfilter.c's
`AVBitStreamFilterLink` and `AVBitStreamFilterPad` for bsf.c's cut dependency on it.

## Evidence against it

`bsf.c` never names either type (`grep -c` prints 0) and no file of its module touches
their fields; its bundled `bsf_internal.h` names them only in the prototypes of functions
that bitstreamfilter.c and bsfgraph.c define and in the internal derived types
`FFBitStreamFilterContext` and `FFBitStreamFilterGraph`:

```console
$ git show 749e05641d:crustify/artifacts/modules.json > /tmp/m3.json
$ python3 crustify/reviews/planner/modules/home-avcodec-h-parser-and-hwaccel-types/type_homes.py \
    /tmp/m3.json AVBitStreamFilterContext AVBitStreamFilterGraph
AVBitStreamFilterContext: owner libavcodec/bsf.c; owner files name it 12x, access its fields 0x; field accesses by in-scope file: [('libavcodec/bitstreamfilter.c', 126), ('libavcodec/bsfgraph.c', 35), ('libavcodec/bsf/source.c', 18), ('libavcodec/bsf/sink.c', 13)]
AVBitStreamFilterGraph: owner libavcodec/bsf.c; owner files name it 7x, access its fields 0x; field accesses by in-scope file: [('libavcodec/bsfgraph.c', 35)]
```

The lifecycles:

- `AVBitStreamFilterContext`: `ff_bsf_alloc` and `ff_bsf_free`
  (`c/libavcodec/bitstreamfilter.c:54`, `:226`), its `AVClass` "AVBitStreamFilterContext"
  (`:45`), `av_bsf_link`, `av_bsf_init_dict` and `av_bsf_init_str` (`:126`, `:326`, `:352`).
- `AVBitStreamFilterGraph`: `av_bsf_graph_alloc`, `av_bsf_graph_free` and
  `av_bsf_graph_config` (`c/libavcodec/bsfgraph.c:48`, `:93`, `:294`), its `AVClass`
  "AVBitStreamFilterGraph" (`:41`).

## The edit and what it moves

- `modules.json`: `AVBitStreamFilterContext` is `included` in bitstreamfilter.c's (15)
  `bsf.h` entry, `AVBitStreamFilterGraph` in bsfgraph.c's (18). bsf.c no longer depends on
  bitstreamfilter.c, so its cut and the pre-wave it called for are gone; sink.c and source.c
  now depend on bitstreamfilter.c. The two types point at each other
  (`AVBitStreamFilterContext.graph`, `AVBitStreamFilterGraph.filters`): bitstreamfilter.c
  (more depended on) runs first and its dependency on bsfgraph.c is cut.
- Configurations: `crustify/wavefront/configs/libavcodec/{13,15,18}/wavefront-config.json`.
- `schedule.json`: bsf.c's waves 468-471 become 624-627 (`AVBitStreamFilter` with the list
  functions, then `AVBSFContext`); the kept waves 464-465 now open bitstreamfilter.c's own
  run, and its waves 476-477 become 630-635 (pad accessors, `AVBitStreamFilterContext`,
  init and link functions); the cut adds the pre-wave 628-629, bsfgraph.c
  [`AVBitStreamFilterGraph`], before bitstreamfilter.c. The graph orders
  `AVBitStreamFilterGraph` first and refers to `AVBitStreamFilterContext` raw until its
  owner back-fills it (`fallback`), so wave 628 records `cut: [632]`.

Rerun: as in `../home-avcodec-h-parser-and-hwaccel-types/report.md`.
