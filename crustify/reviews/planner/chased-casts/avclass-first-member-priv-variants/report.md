# `priv_data` / `priv` read as `AVClass`: first-member convention, no type relation

## What the earlier plan decided

`d4652b74b2` decided that the public type-erased fields need no T3 rows:

> No erased_field rows. None of the 17 public erased fields that have no cast row is read
> as one named type [...]

That review covered only the erased fields without a cast row. The fields that do have
one are turned into **field variants** by the current wavefront (`c7c38c6`, T1/T2
re-extracted 2026-10-18 with `t2/field_casts.csv`): each type stored into or read out of
an erased field depends on the field's owner, as a derived type on its base. The committed
table holds no `drop` row (the `op` column did not exist), so every variant wavefront
derives is in the graph, including five that are no type relation.

## Evidence against it

The campaign-wide API graph derives these variants (rerun:
`python3 crustify/reviews/planner/chased-casts/avclass-first-member-priv-variants/cast_edges.py /target <config> <out.json>`
with the committed table restored by `git show 4b0bde1a3a:crustify/wavefront/t3/chased_casts.csv`,
then `witness.py <out.json> 4` for the sites):

| owner field | variant | witness |
|---|---|---|
| `AVCodecContext.priv_data` | `AVClass` | `libavcodec/avcodec.c:218` `*(const AVClass **)avctx->priv_data = codec->priv_class;` (also `:379`, `pthread_frame.c:849`, `options.c:133`) |
| `AVFormatContext.priv_data` | `AVClass` | `libavformat/demux.c:311` `*(const AVClass **) s->priv_data = s->iformat->priv_class;` (also `mux.c:132`, `mux.c:206`, `libavdevice/utils.c:50`) |
| `AVFilterContext.priv` | `AVClass` | `libavfilter/avfilter.c:732` `*(const AVClass**)ret->priv = filter->priv_class;` |
| `AVBSFContext.priv_data` | `AVClass` | `libavcodec/bsf.c:130` `*(const AVClass **)ctx->priv_data = filter->priv_class;` |
| `AVBitStreamFilterContext.priv_data` | `AVClass` | `libavcodec/bitstreamfilter.c:85` `*(const AVClass**)ctx->priv_data = filter->priv_class;` |

Each site writes the component's `priv_class` into the **first member** of its private
context (codec, demuxer/muxer, filter or bitstream-filter private data, whose layout each
implementation defines), so that `av_opt_*` can treat the private data as an
AVClass-enabled object. `priv_data` holds one implementation's private struct, never an
`AVClass`, and an `AVClass` is no instance of `AVCodecContext`: this is the first-member
convention `docs/graph.md` (Polymorphism) names as a false positive.

The variant makes `AVClass` depend on each context, which already depends on `AVClass`
through its own `av_class` field. The cycle is cut against the class: in the committed
graph, `AVClass` carries five `fallback` edges, and `libavutil/core` depends on libraries
linked after it:

```console
$ python3 crustify/reviews/planner/chased-casts/avclass-first-member-priv-variants/fallbacks.py <config with the committed table>
  AVClass (libavutil/log.h) -> AVBSFContext (libavcodec/bsf.h)
  AVClass (libavutil/log.h) -> AVBitStreamFilterContext (libavcodec/bsf.h)
  AVClass (libavutil/log.h) -> AVCodecContext (libavcodec/avcodec.h)
  AVClass (libavutil/log.h) -> AVFilterContext (libavfilter/avfilter.h)
  AVClass (libavutil/log.h) -> AVFormatContext (libavformat/avformat.h)
```

## Edit and downstream effect

Five `erased_field ... drop` rows:

```csv
erased_field,AVBSFContext,libavcodec/bsf.h,priv_data,AVClass,libavutil/log.h,,,drop
erased_field,AVBitStreamFilterContext,libavcodec/bsf.h,priv_data,AVClass,libavutil/log.h,,,drop
erased_field,AVCodecContext,libavcodec/avcodec.h,priv_data,AVClass,libavutil/log.h,,,drop
erased_field,AVFilterContext,libavfilter/avfilter.h,priv,AVClass,libavutil/log.h,,,drop
erased_field,AVFormatContext,libavformat/avformat.h,priv_data,AVClass,libavutil/log.h,,,drop
```

DAG change, campaign-wide API graph: the five `AVClass -> <context>` fallbacks disappear,
and with them the `libavutil/core -> libavcodec/bsf|core`, `-> libavformat/core` and
`-> libavfilter/core` relations (`api_subgraph.py`, before/after in the commit message).
Measured alone (the 68 kept `erased_arg` rows plus these five, `crustify/tmp/planner-run-1/tB` against `tA`), the
per-subsystem schedules move:

- `libavformat/core`: `av_format_control_message` wave 0 -> 4: with the class no longer in
  the cycle, the `AVFormatContext <-> av_format_control_message` cycle is cut on the
  context's `control_message_cb` field instead of on the callback.
- `libavfilter/core`: the `AVFilterContext / AVFilterGraph / AVFilterLink` cycle is cut
  differently; 57 items move by one wave (`AVFilterContext` 1 -> 2, `AVFilterGraph` 4 -> 3,
  the buffersink/buffersrc accessors 2 -> 3, the graph functions 5 -> 4, ...). Waves and
  batches stay 7 and 15.

No completed subsystem moves. Re-emitted: `libavformat/core/schedule.json`,
`libavfilter/core/schedule.json` (together with the other corrections of this run).

## Questions for the user

None. The AVClass-enabled object relation stays as the three cast pairs wavefront derives
(`AVCodecContext`, `AVFilterContext`, `SwsContext` -> `AVClass`, all kept and directed
toward the class); it is a class-descriptor pointer in the first member, not an embedded
base, so no `base_derived` row records it.
