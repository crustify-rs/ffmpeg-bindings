# Wavefront: `av_frame_side_data_clone` record misses the in-place path's leak and its flags-0 metadata copy

- Symbol: `av_frame_side_data_clone` (`libavutil/side_data.c`). Its findings
  were submitted by translation batch 1 of buffer-frame wave 3
  (`63d684f623`).
- Status: resubmitted. `submission.json` here is the corrected set,
  applied with `query symbols --name av_frame_side_data_clone --update`.

## What was wrong

The `sd` note describes the in-place replacement path as "an existing
entry gets src's buffer (av_buffer_replace), data/size and a copy of its
metadata only with REPLACE". That leaves out two properties a wrapper
needs:

1. The C defect. On that path, a failing `av_dict_copy` into the local
   `dict` returns without freeing it (`side_data.c:275-277`). `av_dict_copy`
   keeps the entries it already copied, so a source whose copy fails after
   its first entry leaks a non-empty dictionary. The proof, with LSan's
   trace, is in
   `../../ub_safe/side-data-clone-replace-leaks-partial-metadata/`.
   Wrapping the call as recorded reproduced the leak, and that is what
   batch 1 did.
2. The copy semantics. Both metadata copies pass flags 0, without
   `AV_DICT_MULTIKEY`. Keys equal up to ASCII case therefore collapse into
   one entry that holds the last value, and the merged entry moves to the
   end. The old `src` note said the entry holds "its own deep copy of the
   metadata", which reads as an entry-for-entry copy, as the crate's
   `Dict::clone` (MULTIKEY) makes.

## What changed

The `sd` note now gives the in-place path's order: metadata copy, then
`av_buffer_replace`, then data/size, with the old metadata freed. It also
records the flags-0 merge, the leak, and the Rust mapping
(`SideDataArrayMut::add_clone` runs that path itself). The `src` note
records the merge. The ownership facets and the lifetime role (`for: src`,
`is_cloner: {deep: true, upref: false}`) are unchanged. They were checked
against the source and are right: the entry is a fresh allocation, and the
buffer it shares by count is a sub-object. The other seven records of the
workset were checked against `side_data.c` and `frame.h` as well, and they
are correct.
