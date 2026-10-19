# Byte buffers read through `intreadwrite.h` alignment unions are no variants

## What the earlier plan decided

As for [`avclass-first-member-priv-variants`](../avclass-first-member-priv-variants/report.md):
`d4652b74b2` left `chased_casts.csv` without `erased_field` rows, so every field variant
the current wavefront derives from `t2/field_casts.csv` is in the graph.

## Evidence against it

21 variants of public byte-buffer fields are the unaligned-load unions of
`libavutil/intreadwrite.h` (`union unaligned_16/32/64 { ... } __attribute__((packed))`,
`av_alias32`), through which the `AV_RN*`/`AV_WN*` macros read and write integers at any
alignment. Sites, as `witness.py` lists them (first of each):

| owner field | variants | sites | e.g. |
|---|---|---|---|
| `AVBufferRef.data` | `unaligned_32`, `unaligned_64` | 5 | `libavcodec/exif.c:792` (`av_exif_write`) |
| `AVCodecContext.extradata` | `av_alias32`, `unaligned_16`, `unaligned_32` | 28 | `libavcodec/4xm.c:1022` |
| `AVCodecParameters.extradata` | `av_alias32`, `unaligned_16`, `unaligned_32` | 31 | `libavcodec/bsf/h264_mp4toannexb.c:271` |
| `AVDictionaryEntry.key` | `unaligned_32` | 2 | `libavformat/id3v2enc.c:301` |
| `AVFrameSideData.data` | `av_alias32`, `unaligned_32` | 3 | `libavcodec/decode.c:332` |
| `AVPacket.data` | `unaligned_16`, `unaligned_32`, `unaligned_64` | 58 | `libavcodec/hcadec.c:538` |
| `AVPacketSideData.data` | `av_alias32`, `unaligned_32`, `unaligned_64` | 5 | `libavformat/dump.c:506` |
| `AVProbeData.buf` | `unaligned_16`, `unaligned_32`, `unaligned_64` | 89 | `libavformat/bink.c:67` |
| `AVProfile.name` | `unaligned_32` | 1 | `libavcodec/proresenc_anatoliy.c:916` |

Rerun: `cast_edges.py` and `witness.py` in
[`../avclass-first-member-priv-variants/`](../avclass-first-member-priv-variants/), with the
committed table (`git show 4b0bde1a3a:crustify/wavefront/t3/chased_casts.csv`).

These are the alignment puns `docs/graph.md` lists as false positives: the buffer holds
bytes, read four or eight at a time, not an instance of the union. As variants they make
the `libavutil/core` unions depend on `AVPacket`, `AVPacketSideData`, `AVCodecContext`,
`AVCodecParameters` (libavcodec) and `AVProbeData` (libavformat), libraries linked after
libavutil.

## Edit and downstream effect

21 `erased_field ... drop` rows, one per owner field and union (`type_def_file`
`libavutil/intreadwrite.h`).

DAG change: the campaign-wide API graph loses the `libavutil/core -> libavcodec/core`
(13 edges), `-> libavformat/core` (3), `-> libavutil/buffer-frame` (4) and
`-> libavutil/options` (1, `AVDictionaryEntry`) relations made of these unions. No
per-subsystem schedule moves (measured alone, `tC` against `tA`): no union is a scheduled
unit of a subsystem whose closure also holds its owner. No schedule is re-emitted for it.

## Questions for the user

None.
