At the user's request, this run departs from your role in two places. Keep each departure;
do not reconcile it with the rest of your prompt:

- it **keeps the completed subsystems** in the plan, although the planning instructions
  drop the subsystems a campaign completed; and
- it **re-plans the completed subsystems** as well, into a separate file, so the user can
  compare the new plan with what ran.

## The schema update has landed

Planner run 0 updated the plan to the current schema in `4b0bde1a3a` and was stopped
before planning. That commit changed no decision; the assistant checked it. Do not redo or
amend it: start from it, and commit this run's plan as one commit on top of it.

## The plan

Run your workflow from step 1 on the converted plan, with the current wavefront and
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

The v1 order is what `4b0bde1a3a` committed. The order of the link units and of the
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
