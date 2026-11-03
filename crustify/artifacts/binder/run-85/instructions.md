## Items that have landed (departure)

`AVOption` and `AVOptionRanges` already have wrappers, from wave 4, which bound them before
`AVRational` (wave 4, another batch), `AVOptionArrayDef` and `AVOptionRange` (wave 20) were
wrapped. Your batch schedules them again on purpose, from planner run 5's re-plan: treat
their existing annotations as work to bring to the current specs, not as completed work,
and replace each raw reference to those three types that their wrappers now make
obsolete. Keep the public API wherever the specs allow, and name in the commit message
what changed for each item.
