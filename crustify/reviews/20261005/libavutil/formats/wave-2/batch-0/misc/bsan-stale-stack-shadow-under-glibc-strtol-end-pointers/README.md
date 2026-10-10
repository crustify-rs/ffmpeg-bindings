# Misc: BSan gate failure is a stale stack shadow under glibc-written end pointers

- Files: `crustify/rust/ffmpeg/tests/common/channel_layout.rs` (new),
  `crustify/rust/ffmpeg/tests/{equiv,ub_safe}/channel_layout.rs`,
  `crustify/rust/ffmpeg/libavutil/channel_layout.rs` (unit tests only)
- Affected commit: `693ccef878` (wave 2 batch 0, `ChannelLayout::parse`);
  the wave-1 equivalence tests of `channel_order` reach the same C lines.
- Item: `AVChannelLayout` (`ChannelLayout::parse`, which wraps
  `av_channel_layout_from_string`)
- Gate log: `crustify/campaigns/20261005/libavutil/formats/review-wave-2/bsan-gate.log`

## Finding

The wave-2 BSan gate aborted the `lib` (one isolated child test), `equiv`
and `ub_safe` binaries with "trying to access an allocation that has been
freed" inside C's `av_channel_layout_from_string`, at `channel_layout.c:342`
(`*endptr`, in the `ambisonic` branch) and `:417` (`*end`, after
`strtoull`). The orchestrator's triage attributed it to BSan not seeing
glibc's writes, "on any input". That is the right class and the wrong
scope: the report is deterministic for a *sequence* of calls, not for an
input.

`endptr` and `end` are uninitialised locals of `av_channel_layout_from_string`
that uninstrumented glibc `strtol` / `strtoull` write through their address.
BSan tracks provenance in a shadow of every memory slot; a store from
uninstrumented code updates the slot's bytes but not its shadow, so the
local keeps whatever provenance an *earlier frame* left at that stack
address. A previous `av_channel_layout_from_string` call frees its
`av_strdup` copy (`chlist`, `av_freep(&chlist)`) from a slot of its own
frame; when a later call's frame lands at the same addresses, its `endptr`
slot carries the freed `chlist` provenance, and the instrumented load
`*endptr` is reported as a use after free. `probe.c` (this directory),
built with the bsan toolchain's plugin and runtime against the bsan C
build, shows it with no Rust involved:

```
$ ./probe "ambisonic 1"            # alone: fine
ambisonic 1 -> 0 (4 ch, order 3)
$ ./probe stereo "ambisonic 1"     # a name match frees nothing: fine
$ ./probe 3c "ambisonic 1"         # "3c" strdup'd and freed its list first
3c -> 0 (3 ch, order 1)
error: Undefined Behavior: trying to access an allocation that has been freed.
   --> /target/crustify/builds/bsan/src/libavutil/channel_layout.c:342:14
342 | (*endptr && *endptr != '+'))
```

Which inputs trip which line depends on the caller's frames (the same
`3c` then `0x3` sequence passes in the C probe and fails in the `equiv`
binary), so no input that reaches those lines is safe to give C under
BSan. The lines are reached by every `ambisonic ...` description and by
every description `parse_channel_list` rejects (masks, `<n>c`, `<n>C`,
`<n> channels`, misspellings). They are not reached by a standard layout
name (matched by `strcmp` before any parsing) or by an accepted channel
list, with or without the `<n> channels (...)` wrapper. `av_channel_from_string`'s
`USR<n>` path initialises its `endptr` to the string before `strtol`
overwrites it, so that slot's shadow stays live and channel lists with user
ids are fine (probed: `3c` then `ch:USR40`).

Not a wrapper defect: `ChannelLayout::parse` passes C a live string and a
zeroed layout, and C's reads are of its own locals. It is the same class as
formats review wave 0's `av_strtod` report
(`wave-0/batch-1/misc/bsan-stale-provenance-in-av-strtod-under-rust-log-callback`),
which was timing-dependent for the same reason (which frame last used the
slot).

## Fix (tests only)

- `tests/common/channel_layout.rs`: `parsed_before_strtol(description)`
  mirrors `av_channel_layout_from_string` as far as needed to tell whether
  C returns before `strtol` / `strtoull` (the name table, the
  `%d channels (%[^)]` cut, and `parse_channel_list`'s acceptance through
  the safe `Channel::from_name`), conservatively: any spelling it cannot
  classify (quotes, escapes, blanks, empty keys) counts as parsed through
  glibc. `skipped_under_bsan` and `allowed(&DESCRIPTIONS)` apply it under
  `CRUSTIFY_FFMPEG_VARIANT=bsan` only; every other instrument keeps every
  input.
- `tests/ub_safe/channel_layout.rs` and `tests/equiv/channel_layout.rs`:
  the three `DESCRIPTIONS` consumers, the wave-1 `channel_order`
  constructor test and the copy destinations filter through it; the
  `Op::Ambisonic` C twin, which was built by parsing
  `ambisonic <n>+<mask>`, is built by hand under BSan from the parser's own
  formula (`c_ambisonic`), and the one `Op::Parse` of an ambisonic
  description uses the channel-list spelling of the same map under BSan.
- `libavutil/channel_layout.rs` unit tests: the two numeric descriptions
  are kept out under BSan (`c_parser_takes_numbers`), with channel-list
  inputs in their place.
- `tests/ub_safe/channel_layout.rs::parser_input_filter_classifies_cs_paths`
  pins the classification.

## Evidence

- Before: `cargo bsan test -p ffmpeg --lib --tests -- channel_layout` at
  `693ccef878` fails the isolated child of
  `channel_layout_allocation_failures_leave_valid_layouts` at `:342`
  (`/tmp/fw2b0/logs/bsan-cl-base.log` in the review session; the gate log
  has the `equiv` and `ub_safe` aborts at `:417`).
- After: the same command passes (lib 58, equiv 34, ub_safe 43, 136 s),
  and the whole-crate BSan run below in the review README.
