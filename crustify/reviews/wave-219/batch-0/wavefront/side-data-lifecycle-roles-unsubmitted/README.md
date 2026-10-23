# Wavefront: no lifecycle role was recorded for `AVFrameSideData`

- Entity: type `AVFrameSideData` (`libavutil/frame.h`), and the routines
  that build, copy and free its entries (`libavutil/side_data.c`)
- Earlier submission: buffer-frame wave 2, type batch 0 (`83bd5f2e64`)
- Resubmitted: `submission.json` here. It has one record per symbol, each
  sent through `query symbols --name <symbol> --update`.

## Finding

Wavefront records lifecycle roles on the symbol that acts on a type, and
reads them back with `query symbols --lifetime-for <type>`. The type run
submitted the field records and a `_comment_agent` for `AVFrameSideData`.
The comment names the routines: entries are built by
`av_frame_side_data_new`/`_add`/`_clone`, and destroyed by `_remove`,
`_remove_by_props` and `_free`. But the run recorded no role on any of
those symbols. At the wave tip the query returned nothing:

```text
$ wavefront ... query symbols --lifetime-for AVFrameSideData
{ "type": "AVFrameSideData", ..., "dropped_by": [], "fields_disposed_by": [],
  "cloned_by": [], "constructed_by": [] }
```

All six symbol records still had `lifetime: null` and `ptr: null` on every
argument (`_analysis.submitted: false`). That includes the two routines the
type run itself bound: `av_frame_side_data_free` (the `Drop` of
`SideDataArray`) and `av_frame_side_data_add` (`SideDataArrayMut::add`).
The earlier wave's types have their roles recorded on their symbols
(`--lifetime-for AVBufferRef` lists `av_buffer_unref`, `av_buffer_ref`,
`av_buffer_replace` and the allocators).

## Correction

Roles and pointer records were submitted from `side_data.c`:

| symbol | role | on |
|---|---|---|
| `av_frame_side_data_free` | dropper | `sd`: every entry (buffer reference, metadata, struct) and the block |
| `av_frame_side_data_remove`, `_remove_by_props` | dropper | `sd`: the matching entries; the block itself is kept |
| `av_frame_side_data_add` | constructor (alloc) | `sd`: a new or replaced entry built around `*pbuf` |
| `av_frame_side_data_new` | constructor (alloc) | `sd`: a new or replaced entry holding an uninitialised `av_buffer_alloc` payload |
| `av_frame_side_data_clone` | cloner (deep) | `src`: a new entry, sharing `src`'s buffer by count, with a deep-copied dictionary |

The `ptr` blocks record what the wrappers depend on. `sd` is the caller's
array variable (never NULL; `*sd` NULL only with a 0 count). `nb_sd` is the
count C keeps in step with it. `pbuf`'s ownership moves on success without
`NEW_REF`, and the slot keeps the reference on failure. A returned entry is
borrowed from the array, NULL when C added nothing. `new`'s payload is
uninitialised. `src` must have a buffer. After the update the query
returns:

```text
dropped_by:     av_frame_side_data_free, av_frame_side_data_remove,
                av_frame_side_data_remove_by_props   (arg sd)
cloned_by:      av_frame_side_data_clone             (arg src, deep)
constructed_by: av_frame_side_data_add, av_frame_side_data_new (arg sd, alloc)
```

One C defect was found by reading `av_frame_side_data_new` for its
record. It is noted on the record for wave 3, whose symbol batch wraps the
routine. When `av_buffer_alloc` fails (OOM only) and `REPLACE` meets an
existing non-`MULTI` entry, `replace_side_data_from_buf` dereferences the
NULL `buf` (`dst->data = buf->data`). No wrapper in this workset calls
`av_frame_side_data_new`, and `SideDataArrayMut::add` always passes a live
buffer. The defect comes from reading the code; no run reproduced it.
