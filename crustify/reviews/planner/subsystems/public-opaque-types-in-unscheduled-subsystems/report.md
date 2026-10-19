# Public opaque types defined in headers of unscheduled subsystems

## What the earlier plan decided

The setup decomposition (`crustify/tmp/scripts/decompose.py`) homes a private header with
its same-stem translation unit, else in the library's internal subsystem. Five private
headers that define types the public headers forward-declare were homed in subsystems the
public-API scope leaves unscheduled:

| header | committed home | public type it defines | published by |
|---|---|---|---|
| `libavcodec/vorbis_parser_internal.h` | `libavcodec/codecs` | `AVVorbisParseContext` | `libavcodec/vorbis_parser.h` (`libavcodec/utils`) |
| `libavcodec/internal.h` | `libavcodec/codecs` | `AVCodecInternal` | `libavcodec/avcodec.h` (`libavcodec/core`), field `AVCodecContext.internal` |
| `libavformat/internal.h` | `libavformat/formats` | `AVCodecTag` | `libavformat/avformat.h` (`libavformat/core`), `av_codec_get_id` & co. |
| `libavfilter/filters.h` | `libavfilter/filters` | `AVFilterPad` | `libavfilter/avfilter.h` (`libavfilter/core`), `avfilter_pad_get_name/type` |
| `libavfilter/formats.h` | `libavfilter/filters` | `AVFilterFormats`, `AVFilterChannelLayouts` | `libavfilter/avfilter.h`, `AVFilterFormatsConfig` fields |

The committed schedules still wrapped all six (e.g. `libavcodec/core` wave 0:
`{"name": "AVCodecInternal", "defined_in": "libavcodec/internal.h", "kind": "type", ...}`),
because the wavefront that scheduled them kept imported items in a subsystem's schedule.

## Evidence against it

The current wavefront (`c7c38c6`) schedules, in API mode, only the items a configuration
owns: an imported item stays a dependency and is scheduled by the configuration that owns
it (`/opt/venv/share/wavefront/docs/scoping.md`, API mode). A type is owned by the
configuration whose files hold its definition, so the six types belong to
`libavcodec/codecs`, `libavformat/formats` and `libavfilter/filters`, which install no
header and are never scheduled. Re-planned from the committed decomposition, no schedule
holds them (`crustify/tmp/planner-run-1/plan-v1a`, built as below with these headers
removed from the configurations): six public handles of the API would never be wrapped.

The placement itself contradicts `docs/artifacts/subsystems.md` ("A public header belongs
to the subsystem that implements what it declares"; a header shared by subsystems is
listed by each). From the build's include graph (`t1/includes.csv`):

- `libavcodec/vorbis_parser_internal.h` is included only by `libavcodec/vorbis_parser.c`,
  a `libavcodec/utils` translation unit: it is that unit's private header.
- `libavcodec/internal.h` is included by 5 `libavcodec/core` units (`avcodec.c`,
  `decode.c`, `encode.c`, `get_buffer.c`, `utils.c`) besides 32 codecs;
  `libavformat/internal.h` by 16 `libavformat/core` units besides 365 formats;
  `libavfilter/filters.h` and `formats.h` by 7 and 4 `libavfilter/core` units besides the
  filters. They are the core's private headers as much as the internals'.

Rerun from the repository root:

```bash
mkdir -p crustify/tmp/planner-run-1
D=crustify/reviews/planner/subsystems/public-opaque-types-in-unscheduled-subsystems
python3 $D/decompose.py      # -> crustify/tmp/planner-run-1/subsystems.v2.json, subsystem-graph.v2.json
python3 $D/order.py          # the order rule on that graph
```

Without `MOVE` and `SHARE` (set both to `{}`), `decompose.py` reproduces every committed
subsystem entry (`implementation_files`, `api_headers`, `counters`, `imported_deps` and
`objective` of all 22); with them, it emits the corrected file lists, counters and
`imported_deps` committed here. (It writes no `index` and stamps `version` with `HEAD`.)

## Edit and downstream effect

- `libavcodec/vorbis_parser_internal.h` moves from `libavcodec/codecs` to
  `libavcodec/utils`.
- `libavcodec/internal.h`, `libavformat/internal.h`, `libavfilter/filters.h` and
  `libavfilter/formats.h` become shared headers: listed by `libavcodec/core`,
  `libavformat/core` and `libavfilter/core` as well. The sharer scheduled first, the core,
  owns and counts their entities.
- `subsystems.json`: `implementation_files`, `counters.implementation` and
  `imported_deps` of the seven subsystems that change (`libavcodec` core, utils, bsf,
  codecs; `libavformat` core, formats; `libavfilter` core, filters). `api_headers`, the
  objectives and the order do not change (`order.py` gives the committed order; within
  the scheduled `libavcodec` subsystems still core, utils, bsf).
- Configurations: `impl_files` of `libavcodec/core`, `libavcodec/utils`,
  `libavcodec/codecs`, `libavformat/core` and `libavfilter/core`; the campaign-wide union is
  unchanged.

Schedules (`plan-v1a` -> `plan-v1`): `AVCodecInternal` back in `libavcodec/core` wave 0,
`AVVorbisParseContext` in `libavcodec/utils` wave 0, `AVCodecTag` in `libavformat/core`
wave 0, `AVFilterPad`, `AVFilterFormats`, `AVFilterChannelLayouts` in `libavfilter/core`
wave 0, each homed as before (`avcodec.rs`, `vorbis_parser.rs`, `avformat.rs`,
`avfilter.rs`), with no anchors (opaque). Nothing else moves between waves. In the
campaign-wide API graph the `libavcodec/core|utils -> libavcodec/codecs`,
`libavformat/core -> libavformat/formats` and `libavfilter/core -> libavfilter/filters`
relations disappear. Re-emitted: `libavcodec/core`, `libavcodec/utils`, `libavformat/core`,
`libavfilter/core` (with this run's other corrections).

## Questions for the user

`AVCodecInternal` is only reachable through the private `AVCodecContext.internal` field;
it stays scheduled as the committed plan had it. Say if the binder should skip it.
