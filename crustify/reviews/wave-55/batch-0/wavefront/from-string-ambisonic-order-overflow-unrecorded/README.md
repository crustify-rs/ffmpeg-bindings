# `av_channel_layout_from_string`'s `str` finding recorded no undefined input

Affected base: `e0a75b5d7f` (finding submitted by formats wave-2 batch 0
with the `AVChannelLayout` type run, carried unchanged through wave 3).

The `str` note said only "The layout description, a NUL-terminated string
read during the call only (strcmp, av_strdup'd copy for parsing,
strtol/strtoull)". It is right about ownership (borrowed for the call, not
retained), but it did not say that the string's *content* can make the call
undefined: an `ambisonic <order>` segment whose order `strtol` narrows to
exactly `INT_MAX` makes C compute `order + 1` and `(order + 1) * (order +
1)` in `int` (`channel_layout.c:341`, `:346`), signed overflows UBSan
reports, after which C returns 0 with an ambisonic layout of `nb_channels`
0. The wave-3 batch-1 notes on the readers record their `int` hazards
(`nb_channels - popcount`, the ambisonic square); the parser's own was
missing, and the wrapper passed every description through
(`ub_safe/parse-ambisonic-int-max-order-overflow/`).

Resubmitted `submission.json` with the hazard, the spellings that reach
it (first segment or any `+`-chained one after a valid order), the
wrapper's refusal, and the other numeric paths that do not overflow
(`av_sscanf`'s `%d`, `strtoull`'s mask with `errno` checked, `strtol`'s
count with `channels > 0`):

```
wavefront <worktree> --config crustify/wavefront/configs/wavefront-config.json \
  query symbols --name av_channel_layout_from_string --file libavutil/channel_layout.c \
  --update submission.json
```

The other seventeen findings of this workset were checked against
`channel_layout.c` and need no correction: the readers' notes name the
NULL-map, negative-count and `int` hazards the wrappers guard; `copy`,
`custom_init`, `default`, `from_mask` and `uninit` record the
overwrite-without-uninit and failure states the owner relies on; `retype`
records the check-first, untouched-on-failure and map-replacement
behaviour; `standard` records the never-dereferenced index cookie.
