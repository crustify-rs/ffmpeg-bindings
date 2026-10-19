# Stale `erased_arg` row: `av_opt_show2` parameter 1

## What the earlier plan decided

Planner emit commit `d4652b74b2` ("T3 tables for the public API") added 69 `erased_arg`
rows to `crustify/wavefront/t3/chased_casts.csv`, among them:

```csv
kind,slot_name,slot_def_file,slot_member,base_name,base_def_file,derived_name,derived_def_file
erased_arg,av_opt_show2,libavutil/opt.c,1,AVClass,libavutil/log.h,,
```

The row was written against a wavefront that ignored `t3_dir` (the commit says so), so no
graph build checked it.

## Evidence against it

`av_opt_show2(void *obj, void *av_log_obj, int req_flags, int rej_flags)` reads its own
parameter 1 as an `AVClass`: wavefront derives that body read from `t2/arg_casts.csv`, so
the row adds nothing, and the current wavefront reports it as stale on every graph build:

```console
$ wavefront /target --config crustify/wavefront/configs/wavefront-config.json \
    query dag --name AVClass --api-headers-only --depth 1      # with the committed table
warning: T3 add erased_arg av_opt_show2 -> AVClass: already read in its body
```

The same warning was printed by each of the 18 per-subsystem schedules built with the
committed table under `"t3_dir"` (`crustify/tmp/planner-run-1/wf-vold/*/*.log`). The
derived read:

```console
$ grep '"param","av_opt_show2","libavutil/opt.c","1"' crustify/wavefront/codeql/t2/arg_casts.csv | grep '"type","AVClass"'
```

The T3 documentation (`/opt/venv/share/wavefront/docs/artifacts/t3.md`, Row meaning)
classifies an `add` of a body read wavefront already derives as stale.

## Edit and downstream effect

The row is deleted. The table now loads without warnings in the campaign-wide and every
per-subsystem configuration. No DAG change: the edge it stated is derived from T2 either
way, so no subsystem, wave or batch moves.

## Questions for the user

None.
