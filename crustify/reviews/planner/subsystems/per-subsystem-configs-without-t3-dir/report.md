# Per-subsystem configurations without `t3_dir`

## What the earlier plan decided

`d4652b74b2` set `"t3_dir": "t3"` in the campaign-wide
`crustify/wavefront/configs/wavefront-config.json` only:

> Per-subsystem configs keep no t3_dir. In API mode the loader rejects every row whose
> function is outside that config's api_headers, so a shared t3/ breaks every per-subsystem
> schedule [...]. A review run should decide between per-subsystem T3 dirs and a loader
> that skips those rows.

All 22 `crustify/wavefront/configs/<link-unit>/<subsystem>/wavefront-config.json` were
committed without the key, so every subsystem was scheduled without the T3 tables.

## Evidence against it

The installed wavefront (`c7c38c6`) is the loader that skips them: in API mode it loads the
whole table and keeps a cast edge only between two nodes of the configuration's API
closure. With `"t3_dir": "t3"` added, all 18 scheduled configurations and the four
unscheduled ones load the campaign-wide `crustify/wavefront/t3/` without an error (the
committed table only warns about its stale `av_opt_show2` row, filed as
[`../../chased-casts/stale-erased-arg-av-opt-show2`](../../chased-casts/stale-erased-arg-av-opt-show2/report.md)).
The campaign's wavefront usage requires every configuration to set it, so that one pair
of T3 tables serves them all.

Rerun, per subsystem (outputs under the gitignored `crustify/tmp/planner-run-1/`):

```bash
U=crustify/reviews/planner/subsystems/unstarted-schedules-packed-by-old-wavefront
$U/wf_schedule.sh <config root> crustify/tmp/planner-run-1/wf-<variant> libavformat/core
python3 $U/wfdiff.py crustify/tmp/planner-run-1/wf-v0/libavformat/core.json crustify/tmp/planner-run-1/wf-vold/libavformat/core.json
```

with `<config root>` the committed configurations (`v0`) and the same configurations plus
`"t3_dir"` naming the committed table (`vold`).

## Edit and downstream effect

`"t3_dir": "t3"` after `"state_dir"` in all 22 per-subsystem configurations. The committed
`erased_arg` rows now reach the per-subsystem graphs; measured with the committed table
(`v0` -> `vold`):

- `libavutil/core` (completed; observation): `av_log`, `av_log_default_callback`,
  `av_log_format_line`, `av_log_format_line2`, `av_log_once` wave 0 -> 2, after `AVClass`.
- `libavutil/options` (completed; observation): 19 `av_opt_*`/`av_parse_color`/
  `av_set_options_string` functions wave 0 -> 1.
- `libavformat/core`: `av_hex_dump_log` wave 0 -> 1 (the move `d4652b74b2` measured on its
  development tree).

Re-emitted: `libavformat/core/schedule.json` (with this run's other corrections).

## Questions for the user

None.
