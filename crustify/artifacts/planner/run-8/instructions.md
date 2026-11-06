# Planner run 8: convert the plan to the module-ordered schedule

The user decided the following departures from your prompt. Keep them; do not reconcile them
with the rest of it.

- **Review waves.** Drop every review wave that has not run (`status: todo`), and batch 1 of
  review wave 953, which never launched. The review waves that ran keep their records as they
  are, wave 953 with its batch 0 only and `status: done`.
- **Started waves.** Waves 902 and 952 ran only part of their batches. Keep only the batches
  that landed, 902's batch 1 and 952's batch 0, and set both waves' `status` to `done`. The
  items of the batches dropped from them (902's batch 0, 952's batches 1 and 2) are re-planned
  like any other unlanded item.
- **As-run records.** Waves 0-45 are the records of what ran, with `module: null`, as run 5
  left them: keep them as they are.
