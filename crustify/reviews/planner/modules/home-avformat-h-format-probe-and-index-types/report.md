# avformat.h's format, probe and index types belong to format.c and seek.c

## What the earlier plan decided

Planner run 3 bundled `libavformat/avformat.h` with `libavformat/avformat.c` (libavformat
module 5), which owned every type the header defines, `AVInputFormat`, `AVOutputFormat`,
`AVProbeData` and `AVIndexEntry` included (waves 532 and 534), and ran after
`libavformat/format.c` (10) depended on it for them (`format.c -> avformat.c`, 11 edges).

## Evidence against it

```console
$ git show 749e05641d:crustify/artifacts/modules.json > /tmp/m3.json
$ python3 crustify/reviews/planner/modules/home-avcodec-h-parser-and-hwaccel-types/type_homes.py \
    /tmp/m3.json AVInputFormat AVOutputFormat AVProbeData AVIndexEntry
AVInputFormat: owner libavformat/avformat.c; owner files name it 0x, access its fields 1x; field accesses by in-scope file: [('libavformat/demux.c', 24), ('libavformat/format.c', 12), ('libavformat/options.c', 4), ('libavformat/dump.c', 3)]
AVOutputFormat: owner libavformat/avformat.c; owner files name it 0x, access its fields 1x; field accesses by in-scope file: [('libavformat/mux.c', 29), ('libavformat/format.c', 10), ('libavformat/mux_utils.c', 9), ('libavformat/options.c', 4)]
AVProbeData: owner libavformat/avformat.c; owner files name it 0x, access its fields 1x; field accesses by in-scope file: [('libavformat/format.c', 20), ('libavformat/demux.c', 13), ('libavformat/avformat.c', 1)]
AVIndexEntry: owner libavformat/avformat.c; owner files name it 0x, access its fields 0x; field accesses by in-scope file: [('libavformat/seek.c', 34)]
```

`avformat.c` names none of the four and accesses their fields once at most (`AVProbeData`,
`AVInputFormat`, `AVOutputFormat` once each; `AVIndexEntry` never). Their C lives elsewhere:

- `AVIndexEntry`: all of it is `libavformat/seek.c` (34 field accesses):
  `av_add_index_entry`, `av_index_search_timestamp`, `avformat_index_get_entry` and
  `avformat_index_get_entry_from_timestamp` (`c/libavformat/seek.c:122`, `:245`, `:257`,
  `:266`).
- `AVProbeData`: the probing it is the input of is `libavformat/format.c`'s
  (`av_probe_input_format3`, `av_probe_input_buffer2`, `c/libavformat/format.c:156`, `:256`;
  20 field accesses against demux.c's 13).
- `AVInputFormat`, `AVOutputFormat`: static descriptors with no lifecycle; format.c
  implements what is done with them as descriptors, finding and guessing them by name,
  extension and MIME type (`av_find_input_format`, `av_guess_format`, `av_guess_codec`,
  `c/libavformat/format.c:146`, `:79`), as `libavcodec/allcodecs.c` does for `AVCodec`,
  which run 3 homed there. Their callbacks belong to the derived `FFInputFormat` and
  `FFOutputFormat` of `demux.h` and `mux.h`.

## The edit and what it moves

- `modules.json`: format.c's (10) `avformat.h` entry `includes` `AVInputFormat`,
  `AVOutputFormat` and `AVProbeData`, seek.c's (17) `AVIndexEntry`; avformat.c's `excludes`
  them. format.c no longer depends on avformat.c, avformat.c now depends on format.c
  (`AVFormatContext.iformat`, `.oformat`), and so format.c runs before avformat.c; the
  dependency on `libavformat/utils.c` (`AVCodecTag`) moves from avformat.c to format.c.
  allformats.c, demux.c, mux.c, mux_utils.c and libavdevice's alldevices.c and avdevice.c
  depend on format.c.
- Configurations: `crustify/wavefront/configs/libavformat/{5,10,17}/wavefront-config.json`.
- `schedule.json`: format.c's waves 550-551 become 640-645 (`AVProbeData` and `av_match_ext`,
  the two format types, the find and probe functions), run right after libavformat/utils.c;
  avformat.c's waves 532-535 become 646-649; seek.c gains 650-651 (`AVIndexEntry`) before its
  kept wave 564. `depends_on` of the kept waves 536, 538, 542, 544, 552, 554, 556, 564, 598
  and 606 follows.

Rerun: as in `../home-avcodec-h-parser-and-hwaccel-types/report.md`.
