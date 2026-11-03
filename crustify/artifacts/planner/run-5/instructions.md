# Planner run 5: re-plan the schedule, the landed items included

## 1. Re-plan every module's waves (departure)

The landed wrappers were bound in the subsystem waves that ran, before several of the
dependencies their public signatures name had been wrapped, so they reach those
dependencies raw. The user wants the schedule re-planned for the whole scope as the
scheduling spec plans it, so that each dependency is wrapped before its consumers: the
binders of the new waves then replace the raw references their wrappers make obsolete.
This departs from "landed work is never re-planned"; apply it as written:

- plan every module's waves, the items that have already landed included, as if no wave
  had run: every new wave and batch `todo`, with empty `commits`, each translation wave
  followed by its review wave;
- waves 0-45, the waves that ran, are records: keep them exactly as they are;
- drop the `todo` waves 232-669 and number the re-plan from 670, after every index the
  schedule has used, so that no index is reused. A new wave's `depends_on` names only new
  waves, never one of 0-45.

Keep the decomposition: `modules.json` already follows the current specs. Correct it
only where it contradicts them, filing each correction as your prompt describes, and
leave every module's `status` as it is.

## 2. The CodeQL tables

The orchestrator has just re-extracted `crustify/wavefront/codeql/t1` and `t2` with a
wavefront fix: a field whose type names a struct that one translation unit completes and
another leaves opaque had lost the callback typedef it is declared with (11 fields in
ffmpeg, all in internal headers), and with it the struct's edge to the callback. Plan
from the re-extracted tables.

## 3. Report

The re-plan replaces the waves still to run, so do not file a finding per changed wave.
File one report instead, `crustify/reviews/planner/modules/replan-landed/report.md`, for
the user to read: for every landed item, the new wave and batch that schedule it against
the wave it ran in; and the landed items whose public-signature dependencies the new
plan wraps before them but which were not wrapped when they landed, each with those
dependencies and the batches that wrap them. Give counts first, then the detail, and the
commands that reproduce them.
