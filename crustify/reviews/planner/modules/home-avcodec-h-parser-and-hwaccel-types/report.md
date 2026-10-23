# avcodec.h's parser and hwaccel types belong to parser.c and decode.c

## What the earlier plan decided

Planner run 3 bundled `libavcodec/avcodec.h` with `libavcodec/avcodec.c`, which carries its
name, and so homed every type the header defines there. Its entry for the header listed
only functions as `excluded`, so `libavcodec/avcodec.c` (libavcodec module 11) owned:

```json
{"path": "libavcodec/avcodec.h", "items": {"included": [], "excluded": ["av_fast_padded_malloc", "...", "av_parser_close", "av_parser_init", "av_parser_iterate", "av_parser_parse2", "..."]}}
```

and with it `AVCodecParserContext`, `AVCodecParser` and `AVHWAccel`, scheduled in its waves
452 (`AVCodecParser`, `AVHWAccel`) and 454 (`AVCodecParserContext`).

## Evidence against it

The decomposition spec's Bundling section homes an item of a TU's named header with
another TU when it belongs to that TU, and a type with the TU that implements its lifecycle
and functionality. `avcodec.c` neither names these three types nor touches their fields:

```console
$ git show 749e05641d:crustify/artifacts/modules.json > /tmp/m3.json
$ python3 crustify/reviews/planner/modules/home-avcodec-h-parser-and-hwaccel-types/type_homes.py \
    /tmp/m3.json AVCodecParserContext AVCodecParser AVHWAccel
AVCodecParserContext: owner libavcodec/avcodec.c; owner files name it 0x, access its fields 0x; field accesses by in-scope file: [('libavcodec/parser.c', 76), ('libavformat/demux.c', 26), ('libavcodec/vorbis_parser.c', 2), ('libavcodec/ac3_parser.c', 1)]
AVCodecParser: owner libavcodec/avcodec.c; owner files name it 0x, access its fields 0x; field accesses by in-scope file: [('libavcodec/parser.c', 7)]
AVHWAccel: owner libavcodec/avcodec.c; owner files name it 0x, access its fields 0x; field accesses by in-scope file: [('libavcodec/decode.c', 5)]
```

(The script reads the CodeQL T2 tables under `crustify/wavefront/codeql/`, which
`extract-ql` regenerates.)

- `AVCodecParserContext`: `libavcodec/parser.c` allocates it in `av_parser_init`
  (`c/libavcodec/parser.c:60`, `av_mallocz(sizeof(AVCodecParserContext))`), frees it in
  `av_parser_close` (`:208-209`) and drives it in `av_parser_parse2` (`:120`).
  `grep -c AVCodecParserContext c/libavcodec/avcodec.c` prints `0`.
- `AVCodecParser` (`codec_ids[7]`, `c/libavcodec/avcodec.h:2776`): no lifecycle (static
  instances); `av_parser_init` matches its `codec_ids` against the requested codec
  (parser.c's 7 field accesses), and its derived `FFCodecParser` sits in
  `libavcodec/parser_internal.h`, which parser.c's module already bundles.
  `libavcodec/parsers.c` only returns it from `av_parser_iterate`.
- `AVHWAccel`: `libavcodec/decode.c` sets and clears `avctx->hwaccel` in `hwaccel_init` and
  `ff_hwaccel_uninit` (`c/libavcodec/decode.c:1182-1224`) and selects it in `ff_get_format`;
  avcodec.c only calls `ff_hwaccel_uninit` (`c/libavcodec/avcodec.c:474`).

## The edit and what it moves

- `modules.json`: `AVCodecParserContext` and `AVCodecParser` are `included` in parser.c's
  (libavcodec 23) `libavcodec/avcodec.h` entry, `AVHWAccel` in decode.c's (19); avcodec.c's
  entry `excludes` all three. Their dependency records follow: parser.c and decode.c gain
  the edges, `parsers.c` and libavformat's `demux_utils.c` now depend on parser.c rather
  than avcodec.c, and `AVCodecContext.hwaccel` makes avcodec.c depend on decode.c, a cycle
  (avcodec.c -> decode.c 1 edge, decode.c -> avcodec.c 5): avcodec.c, depended on more, keeps
  its place, and its dependency on decode.c is cut.
- Configurations: `crustify/wavefront/configs/libavcodec/{11,19,23}/wavefront-config.json`.
- `schedule.json`: avcodec.c's waves 452-455 are replaced by 620-623 (`AVSubtitleRect`;
  `AVSubtitle` with the packet side-data converters); parser.c gains waves 636-639
  (`AVCodecParser`, then `AVCodecParserContext`) before its kept wave 492; the cut adds the
  pre-wave 618-619, decode.c [`AVHWAccel`], right before avcodec.c's first wave 450.
  `depends_on` of the kept waves 450, 456, 492, 494 and 546 follows.

Rerun the re-plan: `crustify/tmp/planner4/work/scripts/` (gitignored), `decompose.py`,
`gen_configs.py ../cfg`, `deps.py`, `order.py`, `skips.py`, `replan4.py`, `build_modules4.py`,
then `validate4.py`.
