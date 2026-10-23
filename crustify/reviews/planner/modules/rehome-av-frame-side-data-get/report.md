# `av_frame_side_data_get` belongs to `side_data.c`'s module

Filed by the assistant, at the user's instruction (2026-10-23), in place of a planner run.

## What the earlier plan decided

Planner run 3 split `libavutil/frame.h` between `libavutil/frame.c` (module 64) and
`libavutil/side_data.c` (module 78), giving module 78 the nine `av_frame_side_data_*`
functions that `side_data.c` defines, and leaving the header's `static inline`
`av_frame_side_data_get` with module 64:

```json
{"path": "libavutil/frame.h", "items": {"included": ["av_frame_side_data_add", "av_frame_side_data_clone", "av_frame_side_data_desc", "av_frame_side_data_free", "av_frame_side_data_get_c", "av_frame_side_data_name", "av_frame_side_data_new", "av_frame_side_data_remove", "av_frame_side_data_remove_by_props"], "excluded": []}}
```

`schedule.json` homed it in `src/libavutil/libavutil/frame.rs`, and module 78's
wavefront configuration listed it under `out_of_scope`.

## Evidence against it

A `static inline` function of a shared header goes to the module whose logic it
implements, as the functions it calls show (decomposition § Bundling). Its whole body
calls `side_data.c`'s function:

```console
$ sed -n 1195,1201p c/libavutil/frame.h
static inline
const AVFrameSideData *av_frame_side_data_get(AVFrameSideData * const *sd,
                                              const int nb_sd,
                                              enum AVFrameSideDataType type)
{
    return av_frame_side_data_get_c((const AVFrameSideData * const *)sd,
                                    nb_sd, type);
```

Its wrapper, `SideDataArrayMut::find_mut`, is a method of the side data arrays that
`side_data.rs` holds.

## The edit and what it moves

- `modules.json`: `av_frame_side_data_get` joins module 78's `frame.h` `included` and
  module 64's `excluded`; the modules' `api.functions` counters move from 19 to 18 (64)
  and 9 to 10 (78). Their `loc` counters are left as they were.
- `crustify/wavefront/configs/libavutil/78/wavefront-config.json`: the item leaves
  `out_of_scope`.
- `schedule.json` and `schedule.replan.json`: the item's `home` becomes
  `src/libavutil/libavutil/side_data.rs`.

Both modules are frozen and the item's wave is done: the edit moves its landed wrapper,
not its schedule. No wave or batch is re-planned.
