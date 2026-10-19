# Reused, user-opaque and per-implementation slots are no variants

## What the earlier plan decided

As for [`avclass-first-member-priv-variants`](../avclass-first-member-priv-variants/report.md):
`d4652b74b2` left `chased_casts.csv` without `erased_field` rows, so every field variant
the current wavefront derives is in the graph.

## Evidence against it

Nine variants record a value that one implementation parks in a slot, not an instance of
the slot's owner (`witness.py` in
[`../avclass-first-member-priv-variants/`](../avclass-first-member-priv-variants/), with the
committed table):

| owner field | variant | sites | why no type relation |
|---|---|---|---|
| `AVStream.priv_data` | `AVFormatContext` | `libavformat/sapenc.c:50,164,268` | the SAP muxer keeps each stream's chained RTP muxer context in the stream's private slot |
| `AVStream.priv_data` | `AVPacket` | `libavformat/flacenc.c:181`, `libavformat/moflex.c:320` | two formats keep a pending packet there |
| `AVPacket.data` | `AVFrame` | `libavcodec/wrapped_avframe.c:92`, `libavformat/mux.c:586,742`, `libavformat/yuv4mpegenc.c:185` | the wrapped-avframe codec and uncoded-frame muxing carry an `AVFrame` (or `AVFrame *`) as packet payload |
| `AVBufferRef.data` | `AVRegionOfInterest` | `libavfilter/vf_addroi.c:127` | a fresh buffer for side data; the relation is `AVFrameSideData.data -> AVRegionOfInterest`, which stays |
| `AVBufferRef.data` | `AVSubtitle` | `fftools/ffmpeg_enc.c:886`, `fftools/ffmpeg_filter.c:3099`, `fftools/ffmpeg_dec.c:616,628` | the ffmpeg CLI's own convention, in code outside the campaign's scope |
| `AVRefStructOpaque.nc` | `AVCodecContext` | `libavcodec/decode.c:1998` | the user-opaque argument of a pool callback |
| `AVRefStructOpaque.nc` | `AVContainerFifo` | `libavutil/container_fifo.c:41,53,59` | same |
| `AVRefStructOpaque.nc` | `AVRefStructPool` | `libavutil/refstruct.c:226,245` | same |
| `AVCodecParserContext.priv_data` | `AVVorbisParseContext` | `libavcodec/vorbis_parser.c:309` | one parser's private state; `priv_data` holds whichever parser's struct |

`docs/graph.md` lists the reused slot as a false positive; the user-opaque union and the
per-implementation private slot are the same case: the slot is typed by its user, not by
its owner. The variants put `AVFrame` (libavutil) after `AVPacket` (libavcodec) and
`AVPacket` after `AVStream` (libavformat), against the link order: the committed graph
cuts `AVPacket -> AVStream` as a cycle fallback and has a `libavutil/buffer-frame ->
libavcodec/core` relation (`AVFrame -> AVPacket`).

The two variants of `AVBufferRef.data` kept as genuine, `AVHWDeviceContext` and
`AVHWFramesContext`, are the payload every hardware device and frames reference carries
(`libavcodec/decode.c:1016,1159`, `libavcodec/get_buffer.c:261`, ...); the side-data
variants of `AVFrameSideData.data` and `AVPacketSideData.data` are kept likewise.

## Edit and downstream effect

Nine `erased_field ... drop` rows.

DAG change, campaign-wide API graph: the `AVPacket -> AVStream` fallback and the
`libavutil/buffer-frame -> libavcodec/core`, `libavcodec/core -> libavformat/core` and
`libavutil/containers -> libavutil/buffer-frame` relations disappear. Measured alone
(`tD` against `tA`):

- `libavcodec/core`: 6 -> 5 waves, 25 -> 24 batches. `AVCodecContext` no longer waits for
  `AVRefStructOpaque`'s layer: it moves wave 4 -> 3 and the 28 `avcodec_*`/`av_*` functions
  over it 5 -> 4; `avcodec_fill_audio_frame` 4 -> 1, as `AVFrame` no longer depends on
  `AVPacket`.
- `libavformat/core`: `av_append_packet`, `av_get_packet` 3 -> 1 and `av_pkt_dump2`,
  `av_pkt_dump_log2` 3 -> 2, as `AVPacket` no longer sits behind `AVStream`.
- `libavutil/buffer-frame` (completed; observation): `AVRefStructPool` and its five pool
  functions return one wave earlier, to where they ran.

Re-emitted: `libavcodec/core/schedule.json`, `libavformat/core/schedule.json` (together
with the other corrections of this run).

## Questions for the user

None.
