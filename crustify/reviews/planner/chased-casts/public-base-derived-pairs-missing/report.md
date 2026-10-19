# Public bases of internal derived types: `base_derived` rows left out

## What the earlier plan decided

`d4652b74b2` deliberately left the hierarchy pairs out of `chased_casts.csv`:

> Left out because they cannot load in API mode: 25 base_derived pairs, each a public base
> embedded as the first member of an internal derived type (AVCodec/FFCodec,
> AVFormatContext/FFFormatContext and /FormatContextInternal, AVStream/FFStream, [...]).
> The T3 loader stops on a non-public row in --api-headers-only mode, and these rows add no
> edge to the API graph anyway.

## Evidence against it

The premise no longer holds: the current wavefront (`c7c38c6`) loads off-API rows in API
mode and keeps a cast edge only when both ends are in the API closure
(`/opt/venv/share/wavefront/docs/graph.md`, Type erasure). With the rows below the table
loads in the campaign-wide configuration and in all 22 per-subsystem ones, without error
or warning.

`t2/casts.csv` holds 23 direct casts from a public type to an internal one
(`crustify/wavefront/codeql/t2/casts.csv`, joined with `t1/types.csv` on the API headers of
`crustify/wavefront/configs/wavefront-config.json`). Each internal type embeds the public
one as its first member, which makes every one a hierarchy and the cast a downcast:

| base (public) | derived | first member |
|---|---|---|
| `AVFormatContext` | `FFFormatContext` | `libavformat/internal.h:64` `AVFormatContext pub` |
| `AVFormatContext` | `FormatContextInternal` | `libavformat/avformat_internal.h:33` `FFFormatContext fc` (whose first member is the base) |
| `AVStreamGroup` | `FFStreamGroup` | `libavformat/avformat_internal.h:121` `AVStreamGroup pub` |
| `AVStream` | `FFStream` | `libavformat/internal.h:135` `AVStream pub` |
| `AVCodecParser` | `FFCodecParser` | `libavcodec/parser_internal.h:27` `AVCodecParser p` |
| `AVInputFormat` | `FFInputFormat` | `libavformat/demux.h:66` `AVInputFormat p` |
| `AVOutputFormat` | `FFOutputFormat` | `libavformat/mux.h:61` `AVOutputFormat p` |
| `AVHWAccel` | `FFHWAccel` | `libavcodec/hwaccel_internal.h:34` `AVHWAccel p` |
| `AVFilterGraph` | `FFFilterGraph` | `libavfilter/avfilter_internal.h:138` `AVFilterGraph p` |
| `AVFilterLink` | `FilterLink` | `libavfilter/filters.h:129` `AVFilterLink pub` |
| `AVFilterLink` | `FilterLinkInternal` | `libavfilter/avfilter_internal.h:35` `FilterLink l` (whose first member is the base) |
| `AVFilterContext` | `FFFilterContext` | `libavfilter/avfilter_internal.h:99` `AVFilterContext p` |
| `AVFilter` | `FFFilter` | `libavfilter/filters.h:267` `AVFilter p` |
| `SwsContext` | `SwsInternal` | `libswscale/swscale_internal.h:337` `SwsContext opts` |
| `AVBitStreamFilterGraph` | `FFBitStreamFilterGraph` | `libavcodec/bsf_internal.h:210` `AVBitStreamFilterGraph p` |
| `AVBitStreamFilterLink` | `BitStreamFilterLinkInternal` | `libavcodec/bsf_internal.h:110` `AVBitStreamFilterLink l` |
| `AVBitStreamFilterContext` | `FFBitStreamFilterContext` | `libavcodec/bsf_internal.h:180` `AVBitStreamFilterContext p` |
| `AVBitStreamFilter` | `FFBitStreamFilter` | `libavcodec/bsf_internal.h:29` `AVBitStreamFilter p` |
| `AVBSFContext` | `FFBSFContext` | `libavcodec/bsf.c:36` `AVBSFContext pub` |
| `AVCodec` | `FFCodec` | `libavcodec/codec_internal.h:129` `AVCodec p` |
| `AVExpr` | `AVExprRoot` | `libavutil/eval.c:185` `AVExpr avexpr` (cast in both directions) |
| `AVHWFramesContext` | `FFHWFramesContext` | `libavutil/hwcontext_internal.h:93` `AVHWFramesContext p` |
| `AVIOContext` | `FFIOContext` | `libavformat/avio_internal.h:30` `AVIOContext pub` |

The other two pairs the earlier run counted, `AVCodecInternal` -> `EncodeContext` /
`DecodeContext`, have no public type and stay out. `AVBitStreamFilterLink` is defined in
`libavcodec/bsf/filters.h` (the loader rejects `libavcodec/bsf.h`, its declaring header).

Without the rows, the written direction (base cast to derived) would make each public base
depend on its internal derived type in a body-deep graph of the configuration; the rows
state `derived -> base`.

## Edit and downstream effect

23 `base_derived ... add` rows. DAG change: none in the API graphs the campaign schedules
from: none of the 23 derived types is in the campaign-wide API closure (the closure
`_api_closure` returns while `cast_edges.py` in
[`../avclass-first-member-priv-variants/`](../avclass-first-member-priv-variants/) builds the
graph), so no row adds an edge there; measured alone (`tE` against `tA`), no schedule moves.
The rows record the hierarchy for the body-deep graphs that binders, translators and
reviewers query through the campaign-wide configuration.

## Questions for the user

None.
