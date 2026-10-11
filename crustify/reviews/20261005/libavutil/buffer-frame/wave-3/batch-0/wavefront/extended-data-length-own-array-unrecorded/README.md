# Wavefront: `AVFrame.extended_data`'s note missed C's per-channel reads of `data`

- Item: `AVFrame` (type record, field `extended_data`), submitted by
  translation batch 0 (`2a232726c8`).
- Status: resubmitted (`submission.json` in this directory, applied with
  `query types --name AVFrame --update`). Only the field's `note` changed:
  the structural properties (`array.by_val`, `owned`,
  `borrowed: {lifetime: self}`, `nullable: false`, `mutable: true`) were
  right.

## Defect

The earlier note said: "Its length is never stored: C reads nb_channels
entries (av_frame_ref memdup, frame_copy_audio), so the block must hold at
least ch_layout.nb_channels plane pointers". It treated the length rule as
one that applies to the *separate block* only. It missed that C does not
check which form `extended_data` has before reading one entry per channel.
`frame_copy_audio` (`frame.c:701-706`, reached from `av_frame_copy` and from
`av_frame_ref`/`av_frame_clone` of a frame without `buf[0]`) and
`av_frame_get_plane_buffer` (`frame.c:609-619`) both index
`extended_data[i]` up to the channel count for planar audio. When
`extended_data` is the frame's own 8-entry `data`, that reads `linesize[]`
and the fields after it, and dereferences them as plane pointers.

Evidence: `ub_unsafe/clone-reads-planar-channels-past-data` in this review
(ASan SEGV at address `0x40`, which is `linesize[0]`, in `av_samples_copy`
under `av_frame_clone`).

The note also recorded `alloc_extended_data` as "copies data". It now
copies `data` only while `buf[0]` is set
(`ub_unsafe/extended-data-keeps-a-replaced-external-plane`), and the note
says so.

## Resubmitted note (summary)

The new note keeps the two forms and their owners. It adds that the
per-channel readers ignore the form, so planar audio with more than
`AV_NUM_DATA_POINTERS` channels needs the separate block before any of
them runs. It also lists the Rust guards: `try_clone`'s refusal, and
`alloc_extended_data` copying only counted planes. Wave 4's symbol records
for `av_frame_copy`, `av_frame_make_writable` and
`av_frame_get_plane_buffer` should carry the same precondition.
