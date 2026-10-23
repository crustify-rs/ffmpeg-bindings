Planner run 3 converted the plan to `modules.json` and the schema-3 `schedule.json` and
appended the waves still to run. This run checks it, as your workflow's step 1 describes.

At the user's request, run 3 departed from your role in three places, which this run
keeps. Do not reconcile them with the rest of your prompt:

- waves 0-231 are the v2 waves that ran, **split per module** and recorded as they ran:
  `done`, with `done` batches. They are the record of what ran: correct none of their
  batches or items, although their status lets you leave them alone anyway;
- `crustify/artifacts/schedule.replan.json` re-plans the modules those waves ran, as an
  observation the user compares with what ran. Correct it only where it contradicts the
  current specs, and file no report for a difference between the two; and
- `crustify/tmp/planner/v2-to-v3.json` maps every v2 batch to the v3 batches it became.
  Keep it as it is: the assistant converts the old run dirs from it.

Check everything else run 3 decided against the evidence, the todo waves 232-609 and the
decomposition above all, and correct what is wrong, filing each correction as your
prompt describes. Leave every module's `status` as `todo`: the assistant freezes the
complete ones after this run.
