# frame.c's configuration still targets `av_frame_side_data_get`

Filed by planner run 5. It follows from
[`rehome-av-frame-side-data-get`](../rehome-av-frame-side-data-get/report.md), the `modules.json`
finding that moved the item to module 78.

## What the earlier plan decided

The rehome (1021c71c0af) gave `av_frame_side_data_get`, the `static inline` of
`libavutil/frame.h`, to `libavutil/side_data.c` (module 78). `modules.json` lists it in module 78's
`frame.h` `included` and in module 64's `excluded`:

```json
{"path": "libavutil/frame.h", "items": {"included": [], "excluded": ["av_frame_side_data_add", "av_frame_side_data_clone", "av_frame_side_data_desc", "av_frame_side_data_free", "av_frame_side_data_get", "av_frame_side_data_get_c", "av_frame_side_data_name", "av_frame_side_data_new", "av_frame_side_data_remove", "av_frame_side_data_remove_by_props"]}}
```

The edit removed the item from module 78's configuration's `out_of_scope`. It did not add it to
module 64's configuration, `crustify/wavefront/configs/libavutil/64/wavefront-config.json`, which
names no items:

```json
{"state_dir": "../crustify/wavefront", "t3_dir": "t3",
 "impl_files": ["libavutil/frame.c"], "api_headers": ["libavutil/frame.h"]}
```

## Evidence against it

A config owns every entity defined in its files, less its `items.excluded`. The nine other
`av_frame_side_data_*` functions are defined in `side_data.c`, so module 64 never targets them.
`av_frame_side_data_get` is defined in `frame.h`, which module 64's configuration lists, so both
configurations target it, and both modules' schedules hold it. Before the edit below, after the
schema update of the same run:

```console
$ wavefront "$PWD/c" --config ../crustify/wavefront/configs/libavutil/64/wavefront-config.json \
    query symbols --targeted-only | grep -w av_frame_side_data_get
av_frame_side_data_get	function_inline_header	libavutil/frame.h	libavutil/frame.h
$ wavefront "$PWD/c" --config ../crustify/wavefront/configs/libavutil/78/wavefront-config.json \
    query symbols --targeted-only | grep -w av_frame_side_data_get
av_frame_side_data_get	function_inline_header	libavutil/frame.h	libavutil/frame.h
```

The decomposition spec requires every entity to be targeted by exactly one module's configuration,
so that no two schedules hold it.

## The edit and what it moves

- `crustify/wavefront/configs/libavutil/64/wavefront-config.json`: `items.excluded` names
  `{"name": "av_frame_side_data_get", "defined_in": "libavutil/frame.h", "kind": "symbol"}`.
- frame.c's schedule no longer holds the item. In planner run 5's re-plan, it is wrapped once, by
  module 78 (`libavutil/side_data.c`), homed in `src/libavutil/libavutil/side_data.rs`, as
  `modules.json` and the landed wrapper `SideDataArrayMut::find_mut` already have it.
- The campaign-wide configuration and graph are unaffected, and the DAG changes nowhere else.
