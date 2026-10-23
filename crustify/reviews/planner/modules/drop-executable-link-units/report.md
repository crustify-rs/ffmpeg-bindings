# The executables leave the plan

Filed by the assistant, at the user's instruction (2026-10-23), in place of a planner run.
It answers the question of `executables-link-unit-depends-on`.

## What the earlier plan decided

Planners run 3 and 4 kept link units 7 and 8, the `ffmpeg` and `ffprobe` executables, as
link units with no modules, depending on the seven libraries:

```json
{"index": 7, "name": "ffmpeg", "kind": "executable", "linkage": null, "depends_on": {"scheduled": ["libavutil", "libswresample", "libswscale", "libavcodec", "libavformat", "libavfilter", "libavdevice"], "cut": []}, "modules": []}
{"index": 8, "name": "ffprobe", "kind": "executable", "linkage": null, "depends_on": {"scheduled": ["libavutil", "libswresample", "libswscale", "libavcodec", "libavformat", "libavfilter", "libavdevice"], "cut": []}, "modules": []}
```

## Evidence against it

The user's answer: the campaign's scope is the shared libraries and their installed
public API, which the executables do not publish.

## The edit and what it moves

Both link units are removed from `modules.json`; their indices 7 and 8 stay unused. They
held no module, wave or batch, so nothing is re-planned. The builds keep compiling the
executables, which the FATE baseline runs.
