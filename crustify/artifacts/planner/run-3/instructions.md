At the user's request, this run departs from your role in four places. Keep each
departure; do not reconcile it with the rest of your prompt:

- it **splits the waves that ran per module** when it converts them, although the
  schedule schema's Earlier versions section maps each v2 wave to one module;
- it **records those waves as they ran**: `done`, with `done` batches, although you
  otherwise leave the progress fields as you find them;
- it **re-plans the modules those waves ran**, into a separate file, so the user can
  compare the current scheduling with what ran; and
- it **writes a mapping** from every v2 batch to the v3 batches it became.

## The repository

The campaign has moved to its own repository, `ffmpeg-bindings`, laid out as the layout
spec's Tree section describes: the C is the `c/` submodule at the campaign revision
`1019f8f036`, unchanged, and the safe crate `ffmpeg-bindings` is at the repository's root,
with `src/<link-unit>/<link-unit>/<file>.rs` holding what `crustify/rust/ffmpeg/<link-unit>/`
held. The builds in `crustify/artifacts/build.json` (version 4) and the CodeQL database
and tables under `crustify/wavefront/codeql/` were rebuilt from `c/`.

The plan is still on the v2 schemas: `crustify/artifacts/subsystems.json`, one
`crustify/artifacts/<link-unit>/<subsystem>/schedule.json` per subsystem, and the
`schedule.replan.json` beside each completed one, which planner run 2 wrote as an
observation. Convert the first two as your prompt describes, and delete the
`schedule.replan.json` files with them: git keeps them. Leave the run dirs under
`crustify/artifacts/<link-unit>/<subsystem>/wave-*/` and `review-wave-*/` and the
`crustify/reviews/` tree where they are: the assistant converts them from your mapping.
The earlier plan corrections under `crustify/reviews/planner/subsystems/` stay where
they are; file this run's under `crustify/reviews/planner/modules/`.

## The waves that ran

These subsystems ran, in this order, each wave followed by its review wave:
`libavutil/raw-lifetime-void`, `libavutil/raw-lifetime-string`, `libavutil/core`,
`libavutil/formats`, `libavutil/options`, `libavutil/crypto`, `libavutil/buffer-frame` and
`libavutil/hwcontext`. Their v2 waves span several translation units, so a v3 wave, which
belongs to one module, cannot hold one of them whole:

- Convert each v2 wave into one v3 wave per module its units belong to, holding those
  units, and each v2 batch into one batch per module its items belong to, in the wave of
  that module. Keep every item as it ran, enum wrappers included: the user keeps the
  wrappers landed for enums, although no wave schedules an enum any more. Give each item
  the `home` the conventions give it in the current layout, the module's `.rs` file; the
  assistant moves the landed code there.
- Follow each converted translation wave with its review wave, holding the parts of the
  v2 review batches that reviewed its batches, one review batch per v2 review batch that
  reviewed any of them, its `reviews` naming those batches.
- Number the converted waves first, from 0, in the order they ran: the subsystems in the
  order above, each v2 wave and then its review wave, and within a v2 wave the modules in
  the order the current dependencies give them. Each depends, through `scheduled`, on the
  wave converted before it; none has a `cut`.
- Write every converted wave and batch as `done`, with empty `commits`: the assistant
  fills them.
- The raw-lifetime tiers became foundational modules: convert their waves into waves of
  those modules.

Write the mapping to `crustify/tmp/planner/v2-to-v3.json`, as an object from every v2
batch id, `<link-unit>/<subsystem>/wave-<i>/batch-<j>` for a translation batch and
`<link-unit>/<subsystem>/review-wave-<i>/batch-<j>` for a review batch, to the list of
`[wave, batch]` index pairs it became. A v2 review batch that landed nothing still maps.

A module whose every unit ran in these waves is complete: plan nothing more for it. One
whose units ran only in part keeps its remaining units in waves that are `todo`, after the
converted ones.

## The re-plan

Plan each module the converted waves hold again, as if none of it had run, under the
current settings, specs and evidence, and write those waves to
`crustify/artifacts/schedule.replan.json`, in the schedule's schema, numbered from 0 on
their own. They are not part of the plan. A difference between the two is an
observation, not a correction, so file no report for it. In the commit message, summarize
it per module: waves and batches before and after, and the items added, dropped or moved
between batches, with the reason.

## Scope

The scope is the whole public API, deprecated items excluded. `libavcodec/codecs`,
`libavformat/formats`, `libavfilter/filters` and `libavdevice/devices` installed no headers
in the v2 plan, which left them unscheduled, and the executables were outside it: keep
the units that install no headers and the executables out of the schedule. The modules
that have not run are planned from scratch, under the current specs.
