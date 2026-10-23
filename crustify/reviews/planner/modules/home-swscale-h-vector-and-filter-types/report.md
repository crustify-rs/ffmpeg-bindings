# swscale.h's SwsVector and SwsFilter belong to libswscale/utils.c

## What the earlier plan decided

Planner run 3 bundled `libswscale/swscale.h` with `libswscale/swscale.c` (libswscale module 2),
which owned it but for 32 functions defined elsewhere, and so owned `SwsVector` and
`SwsFilter`, scheduled in swscale.c's waves 390 (`SwsVector`) and 392 (`SwsContext`,
`SwsFilter`).

## Evidence against it

```console
$ git show 749e05641d:crustify/artifacts/modules.json > /tmp/m3.json
$ python3 crustify/reviews/planner/modules/home-avcodec-h-parser-and-hwaccel-types/type_homes.py \
    /tmp/m3.json SwsVector SwsFilter
SwsVector: owner libswscale/swscale.c; owner files name it 0x, access its fields 0x; field accesses by in-scope file: [('libswscale/utils.c', 58)]
SwsFilter: owner libswscale/swscale.c; owner files name it 2x, access its fields 0x; field accesses by in-scope file: [('libswscale/utils.c', 52)]
```

`grep -c SwsFilter c/libswscale/swscale.c` prints 0; the two mentions are prototypes in
`swscale_internal.h` of `ff_sws_init_single_context`, defined in utils.c. Both lifecycles
and all their functionality are `libswscale/utils.c`'s: `sws_allocVec`, `sws_freeVec`,
`sws_getGaussianVec`, `sws_scaleVec`, `sws_normalizeVec` (`c/libswscale/utils.c:1956`,
`:2146`), `sws_getDefaultFilter` and `sws_freeFilter` (`:2167`, `:2155`), and the filters'
use in `ff_sws_init_single_context` and `sws_getContext` (`:1137`, `:1838`).

## The edit and what it moves

- `modules.json`: both are `included` in utils.c's (4) `swscale.h` entry and `excluded` from
  swscale.c's; utils.c's dependency on swscale.c drops from 18 edges to 8.
- Configurations: `crustify/wavefront/configs/libswscale/{2,4}/wavefront-config.json`.
- `schedule.json`: swscale.c's waves 390-393 become 610-611 (`SwsContext`); utils.c's 398-399
  become 612-617 (`SwsVector`; `SwsFilter` with the vector functions; the context and filter
  functions). The kept waves 394 and 400 depend on the new ones.

Rerun: as in `../home-avcodec-h-parser-and-hwaccel-types/report.md`.
