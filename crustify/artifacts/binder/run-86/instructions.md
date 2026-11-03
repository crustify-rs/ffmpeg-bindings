## Items that have landed (departure)

`av_default_get_category`, `av_default_item_name`, `av_log`, `av_log_default_callback`,
`av_log_format_line`, `av_log_format_line2`, `av_log_once` and `av_vlog` already have
wrappers, from waves 4 and 6, which bound them before `AVClass` (wave 6, batch 0) was
wrapped. Your batch schedules them again on purpose, from planner run 5's re-plan: treat
their existing annotations as work to bring to the current specs, not as completed work,
and replace each raw reference to `AVClass` that its wrapper now makes obsolete. Keep the
public API wherever the specs allow, and name in the commit message what changed for each
item.
