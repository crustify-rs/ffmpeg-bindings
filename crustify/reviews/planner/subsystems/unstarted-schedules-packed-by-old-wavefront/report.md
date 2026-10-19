# Unstarted schedules packed by the previous wavefront

## What the earlier plan decided

The setup scheduled every subsystem with the wavefront of 2026-09-30 and projected the
result into the campaign schedule (`crustify/tmp/scripts/project_schedule.py`); planner run
0 split it into per-subsystem schema-v2 schedules without changing a batch (`4b0bde1a3a`).
For the subsystems that have not started:

| subsystem | committed waves / batches / items |
|---|---|
| `libavutil/metadata` | 6 / 54 / 177 |
| `libavutil/containers` | 3 / 10 / 63 |
| `libswscale/swscale` | 3 / 10 / 49 |
| `libavcodec/core` | 5 / 28 / 123 |
| `libavcodec/utils` | 3 / 13 / 52 |
| `libavformat/core` | 5 / 23 / 105 |
| `libavfilter/core` | 7 / 17 / 82 |

## Evidence against it

The installed wavefront (`c7c38c6`) and the tables re-extracted on 2026-10-18 (`t1/params.csv`,
`t2/arg_casts.csv`, `t2/field_casts.csv`, `is_erased`) schedule these subsystems
differently, with the committed configurations and no T3 table (`v0`):

1. **Underfilled batches.** The old wavefront packed each subsystem's imported items
   (`AVClass`, `AVColorPrimaries`, `AVFrame`, ...) into its batches; the projection then
   removed them as owned by an earlier subsystem, leaving batches with one type where two
   fit. The old raw schedules show it, e.g.
   `git show cf39ee56ad^:crustify/wavefront/schedules/libswscale/swscale.json`: wave 0
   batch 0 is `AVClass, AVColorPrimaries` (both imported), and the committed swscale
   schedule then opens with `SwsAlphaBlend` alone. The current API mode drops imported
   items before packing (`docs/batching.md`, API ownership), so the same items pack into
   fewer batches: metadata 54 -> 52, containers and swscale regrouped at the same count,
   `libavcodec/utils` 13 -> 12, `libavformat/core` 23 -> 20, `libavfilter/core` 17 -> 16,
   `libavcodec/core` 28 -> 26.
2. **Field variants.** `AVCPBProperties` is a genuine variant of `AVPacketSideData.data`
   (8 sites, e.g. `libavcodec/encode.c:1048` `ff_encode_add_cpb_side_data`), so it now
   follows its owner: wave 0 -> 2 in `libavcodec/core`, and `av_cpb_properties_alloc`
   1 -> 3.
3. **Callbacks.** `libavformat/avformat.h` publishes two function-pointer types the old
   schedule omitted, `av_format_control_message` (the type of
   `AVFormatContext.control_message_cb`) and `AVOpenCallback`; both are now scheduled.
   `av_mime_codec_str` moves wave 2 -> 1 on the current layers.

Five unstarted schedules are unchanged byte for byte: `libavutil/dsp`,
`libswresample/swresample`, `libavcodec/bsf`, `libavformat/avio`, `libavdevice/core`.

Rerun from the repository root (outputs in the gitignored `crustify/tmp/planner-run-1/`):

```bash
U=crustify/reviews/planner/subsystems/unstarted-schedules-packed-by-old-wavefront
mkdir -p crustify/tmp/planner-run-1
for j in $(python3 -c "import json;d=json.load(open('crustify/artifacts/subsystems.json'));print(' '.join(l['name']+'/'+s['name'] for l in d['link_units'] for s in l['subsystems'] if s.get('api_headers')))"); do
  $U/wf_schedule.sh crustify/wavefront/configs crustify/tmp/planner-run-1/wf-v1 $j
done
python3 $U/project.py crustify/tmp/planner-run-1/wf-v1 crustify/tmp/planner-run-1/plan-v1
python3 $U/sdiff.py crustify/artifacts/libavcodec/core/schedule.json crustify/tmp/planner-run-1/plan-v1/libavcodec/core/schedule.json
python3 $U/batches.py <schedule.json>      # batch composition
python3 $U/validate.py crustify/tmp/planner-run-1   # schema, routes, cross-subsystem order
```

`wf_schedule.sh` runs `wavefront schedule --file <api_headers + impl_files> --transitive
--api-headers-only --max-types 2 --min-fields 20 --max-syms 25 --max-loc 500 --skip
av_assert0_fpu av_init_packet` (the campaign caps; the two deprecated functions in
`crustify/wavefront/deprecated.json`); `project.py` applies the setup's homes and anchors
(deprecated fields dropped, member paths anchored on their top-level field).

## Edit and downstream effect

The seven schedules are re-emitted in place from the current configurations and T3 tables
(this run's other corrections included):

| subsystem | waves / batches / items | moves against the committed schedule |
|---|---|---|
| `libavutil/metadata` | 6 / 54 / 177 -> 6 / 52 / 177 | none; batches regrouped |
| `libavutil/containers` | 3 / 10 / 63 -> 3 / 10 / 63 | none; 3 type batches regrouped |
| `libswscale/swscale` | 3 / 10 / 49 -> 3 / 10 / 49 | none; 4 type batches regrouped |
| `libavcodec/core` | 5 / 28 / 123 -> 5 / 26 / 123 | `AVCPBProperties` 0 -> 2, `av_cpb_properties_alloc` 1 -> 3 |
| `libavcodec/utils` | 3 / 13 / 52 -> 3 / 12 / 52 | none; batches regrouped |
| `libavformat/core` | 5 / 23 / 105 -> 5 / 20 / 107 | + `av_format_control_message`, `AVOpenCallback` (wave 4); `av_hex_dump_log` 0 -> 1; `av_mime_codec_str` 2 -> 1 |
| `libavfilter/core` | 7 / 17 / 82 -> 7 / 16 / 82 | none; batches regrouped |

Every item keeps its home; `AVDOVIDmData`'s anchors are the same 13 fields in the tool's
order. The only cross-subsystem dependency on a later subsystem in the whole plan is
`AVClass` (`libavutil/core`) on `AVOption`/`AVOptionRanges` (`libavutil/options`), the
core/options cycle the order cuts. No order, wave of a completed subsystem, or batch
artifact dir of a landed batch moves.

## Questions for the user

None.
