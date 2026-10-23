At the user's request, this run departs from your role in three places. Keep each departure;
do not reconcile it with the rest of your prompt:

- it lands **two commits** instead of one: the schema update alone, then the plan;
- it **keeps the completed subsystems** in the plan, although the planning instructions
  drop the subsystems a campaign completed; and
- it **re-plans the completed subsystems** as well, into a separate file, so the user can
  compare the new plan with what ran.

## Commit 1: schema update only

The campaign branch holds a schema-v1 `crustify/artifacts/schedule.json`. Before anything
else, update it to the current schema, changing no decision:

- move its order into `subsystems.json`: an `index` on every link unit and subsystem,
  each list in that execution order, `raw-lifetime-void` and `raw-lifetime-string` at
  indices 0 and 1 of `libavutil` with only `index` and `name`;
- split its waves into one schema-v2 `crustify/artifacts/<link-unit>/<subsystem>/schedule.json`
  per subsystem it schedules, the raw-lifetime ones included, keeping every wave, batch
  and item, their order and their indices: a landed batch's wave and batch index must
  keep naming its existing `wave-<w>/batch-<b>/` artifact dir;
- delete `crustify/artifacts/schedule.json`.

Verify the update with a script under `crustify/tmp/`: the per-subsystem schedules,
read in `subsystems.json`'s order, reproduce the v1 file's waves and batches exactly, and
every landed batch's `task.json` has the same `objective` and `items` as its schedule
entry (84 of them do today). Commit only these changes, with a message saying that the
commit is a schema update with no planning change, and giving the verification command
and its result.

## Commit 2: the plan

Then run your workflow from step 1 on the converted plan, with the current wavefront and
its planning artifacts, the T3 tables in `crustify/wavefront/t3/` included. Re-emit, in
place, the schedules of the subsystems that have not started, and file a report for each
correction, as your prompt describes.

The completed subsystems are `libavutil/raw-lifetime-void`, `libavutil/raw-lifetime-string`,
`libavutil/core`, `libavutil/formats`, `libavutil/options`, `libavutil/crypto`,
`libavutil/buffer-frame` and `libavutil/hwcontext`. Keep them in `subsystems.json`, and
keep their converted `schedule.json` unchanged: it is the record of
what ran. Plan each of them again, as if it had not started, under the current settings
and evidence. Write the result beside its schedule, as `schedule.replan.json`, in the
same schema. A difference between the two is an observation, not a correction, so file
no report for it. In the commit message, summarize it for each completed subsystem: waves
and batches before and after, and the items added, dropped or moved between batches,
with the reason.

## Order

Commit 1 keeps the v1 order. In commit 2, the order of the link units and of the
subsystems in `subsystems.json` may change, as the Order section of the planning
instructions decides from the current evidence. The completed subsystems take part in
it like the rest: they keep their names, and their artifact dirs and branches do not
move. Re-emit the indices the order moves. Each move of a subsystem that has not
started is a correction, with its report. A completed subsystem that moves is an
observation, like its re-plan. List the order before and after in the commit message.

## Scope

`libavcodec/codecs`, `libavformat/formats`, `libavfilter/filters` and `libavdevice/devices`
install no headers, so the public-API scope leaves them unscheduled, and the executables
are outside it. Keep it that way.
