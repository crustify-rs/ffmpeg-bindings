# Waves that depend on later waves through the graph's cycle cuts record no cut

## What the earlier plan decided

Planner run 3 wrote every todo wave with `"cut": []`, and built the module dependency
records from each unit's `deps` alone (`crustify/tmp/planner/scripts/deps.py`), leaving out
its `fallback`: the dependencies wavefront cut to flatten a cycle, which the unit refers to
raw until the later unit lands and back-fills it. Two todo waves have such dependencies:

- wave 434, `libavcodec/exif.c`: `AVExifMetadata` refers to `AVExifEntry` (`entries`), which
  wave 436 wraps;
- wave 574, `libavfilter/avfilter.c`: `AVFilterContext` refers to `AVFilterLink` (`inputs`,
  `outputs`), which run 3 placed in wave 576.

## Evidence against it

`specs/artifacts/schedule.md`: `depends_on.cut` lists "the waves it depends on that run
after it, its dependency on them cut to break a cycle", and `modules.json`'s `depends_on`
records every dependency a module has. The campaign-wide public-signature graph holds four
fallback edges:

```console
$ cfg=crustify/wavefront/configs/wavefront-config.json
$ files=$(python3 -c "import json;d=json.load(open('$cfg'));print(' '.join(d['impl_files']+d['api_headers']))")
$ wavefront $PWD/c --config ../$cfg schedule --output /tmp/campaign.json --file $files \
    --transitive --api-headers-only --max-types 2 --min-fields 20 --max-syms 25 --max-loc 500
$ python3 -c "import json; d=json.load(open('/tmp/campaign.json'));
[print(i['name'], '->', [f['name'] for f in i['fallback']]) for w in d['waves'] for b in w['batches'] for i in b['items'] if i['fallback']]"
AVExifMetadata -> ['AVExifEntry']
AVBitStreamFilterGraph -> ['AVBitStreamFilterContext']
AVFilterContext -> ['AVFilterGraph', 'AVFilterLink']
```

Under run 3's decomposition every one of them stayed within a module, so leaving them out of
the module records changed no order; the waves' cuts were still missing. Under this run's
decomposition two cross modules (`AVFilterContext` -> `AVFilterGraph`, avfilter.c ->
avfiltergraph.c; `AVBitStreamFilterGraph` -> `AVBitStreamFilterContext`, bsfgraph.c ->
bitstreamfilter.c).

## The edit and what it moves

- The module records count fallback edges as dependencies (`deps.py` in
  `crustify/tmp/planner4/work/scripts/`): `libavfilter/avfilter.c` cuts its dependency on
  `libavfilter/avfiltergraph.c` (1 edge, 6 back), and bsfgraph.c's record of
  bitstreamfilter.c counts 4 edges (1 of them the fallback).
- A cut that only a fallback edge carries gets no pre-wave (`skips.py`): wrapping its target
  first would precede what the target itself depends on, which the graph orders first.
- Every todo wave records in `cut` the later waves that hold its units' fallback
  dependencies: 434 `cut: [436]`, 574 `cut: [654, 658]`, 628 `cut: [632]`. Done waves are
  unchanged.

Rerun: `python3 crustify/tmp/planner4/work/scripts/validate4.py` checks, among the rest,
that every cut points at a wave that runs later and every other dependency is wrapped
earlier.
