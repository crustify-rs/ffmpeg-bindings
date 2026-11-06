# `avfiltergraph.c`'s record for `avfilter.c` misses the `avfilter_execute_func` edge

Found by planner run 8, at `c/` = `1019f8f036`, wavefront `71b2c15`.

## What the plan decided

`modules.json`, `libavfilter/avfiltergraph.c` (libavfilter, index 3), its record for
`libavfilter/avfilter.c`:

```json
{"link_unit": "libavfilter", "module": "libavfilter/avfilter.c", "foundational": false,
 "outgoing_edges": 6, "incoming_edges": 1,
 "counters": {"structs": 2, "functions": 0, "global_variables": 0, "enums": 0, "unions": 0, "callbacks": 0}}
```

and the mirrored record, `libavfilter/avfilter.c` (index 1) for `libavfilter/avfiltergraph.c`:
`"outgoing_edges": 1, "incoming_edges": 6`.

Run 3 counted both records, and runs 4 to 7 kept them. They were counted at wavefront
`d39a8d6` and earlier.

## Evidence

`AVFilterGraph` (libavfilter/avfilter.h:608) declares its `execute` field with the callback
typedef `avfilter_execute_func` (avfilter.h:558), which `avfilter.c` owns:

```c
typedef int (avfilter_execute_func)(AVFilterContext *ctx, avfilter_action_func *func,
                                    void *arg, int *ret, int nb_jobs);
...
    avfilter_execute_func *execute;
```

The graph's field edges include "a callback typedef a field has" (wavefront `docs/graph.md`, Edges).
At wavefront `71b2c15`, the public-signature graph has the edge
`AVFilterGraph -> avfilter_execute_func`. Run 7's graph at `d39a8d6` did not have it
(`crustify/tmp/planner7/work/all-api-full.schedule.json`: `AVFilterGraph`'s `deps` were
`AVClass` and `AVFilterContext` only). The record therefore misses one edge and one callback.

The module records are counted from a schedule of the whole closure, without
`schedule_seeds`, with enums as units, as the earlier records were: with
`--with-enums`, every other in-tree record of `modules.json` matches the schedule's module
records edge for edge, except the three foundational body-level records and the frozen
`side_data.c -> frame.c` record (14, where the graph gives 16), which run 7 left for the user.
To rerun, from `c/`, with the committed configuration less its `schedule_seeds`
(`crustify/tmp/planner8/work/noseeds-config.json`):

```sh
wavefront . --config ../crustify/tmp/planner8/work/noseeds-config.json schedule \
  --api-headers-only --with-enums --output ../crustify/tmp/planner8/work/full-enums.schedule.json
python3 -c "
import json
d = json.load(open('../crustify/tmp/planner8/work/full-enums.schedule.json'))
m = {x['name']: x for x in d['modules']}
for a, b in (('libavfilter/libavfilter/avfiltergraph.c', 'libavfilter/libavfilter/avfilter.c'),
             ('libavfilter/libavfilter/avfilter.c', 'libavfilter/libavfilter/avfiltergraph.c')):
    print(a, '->', b, [(r['total_edges'], r['unique_edges']) for p in ('scheduled', 'cut')
                        for r in m[a]['depends_on'][p] if r['module'] == b])
for w in d['waves']:
    for bt in w['batches']:
        for i in bt['items']:
            if i['name'] == 'AVFilterGraph': print(i['depends_on']['scheduled'])
"
```

Output: `avfiltergraph.c -> avfilter.c [(7, 3)]`, `avfilter.c -> avfiltergraph.c [(1, 1)]`, and
`AVFilterGraph`'s scheduled dependencies `AVClass`, `AVFilterContext` and the symbol
`avfilter_execute_func`. `crustify/tmp/planner8/records.py` compares every record.

## Edit

- `libavfilter/avfiltergraph.c` -> `libavfilter/avfilter.c`: `outgoing_edges` 6 to 7,
  `counters.callbacks` 0 to 1.
- `libavfilter/avfilter.c` -> `libavfilter/avfiltergraph.c`: `incoming_edges` 6 to 7.

Both modules are `todo`. The edit moves no module, wave or batch. The cut stays as the order
gives it: `avfilter.c` cuts `avfiltergraph.c` (`AVFilterContext -> AVFilterGraph`), and the
wrap wave of `AVFilterGraph` that runs before `avfilter.c`'s `AVFilterContext` wave keeps
`avfilter_execute_func` as a cut dependency.

## Questions for the user

None.
