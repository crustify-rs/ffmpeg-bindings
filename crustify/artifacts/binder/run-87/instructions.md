## Items that have landed (departure)

`AVRegionOfInterest` already has a wrapper, from wave 32, which bound it before
`AVFrameSideData` (wave 34) was wrapped. Your batch schedules it again on purpose, from
planner run 5's re-plan: treat its existing annotations as work to bring to the current
specs, not as completed work, and replace each raw reference to `AVFrameSideData` that its
wrapper now makes obsolete. Keep the public API wherever the specs allow, and name in the
commit message what changed.
