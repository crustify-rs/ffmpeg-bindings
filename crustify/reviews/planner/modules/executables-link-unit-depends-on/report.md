# The executables' link units record no dependency

## What the earlier plan decided

Planner run 3 kept `subsystems.json`'s link units 7 and 8, the executables, without modules
and with no dependency:

```json
{"index": 7, "name": "ffmpeg", "kind": "executable", "linkage": null, "depends_on": {"scheduled": [], "cut": []}, "modules": []}
{"index": 8, "name": "ffprobe", "kind": "executable", "linkage": null, "depends_on": {"scheduled": [], "cut": []}, "modules": []}
```

## Evidence against it

Both link every library of the campaign (the plain build's linker outputs):

```console
$ for e in ffmpeg ffprobe; do readelf -d crustify/builds/plain/$e | grep NEEDED; done
```

lists `libavdevice.so.63 libavfilter.so.12 libavformat.so.63 libavcodec.so.63
libswresample.so.7 libswscale.so.10 libavutil.so.61` for each (with libm, libc, and libz
for ffmpeg).

## The edit and what it moves

`depends_on.scheduled` of both is the seven libraries. They hold no module and so no wave:
nothing is re-planned.

## Question for the user

The campaign's scope is the libraries' public API, and the executables publish none, so
nothing in them is planned. Should the two link units be dropped from `modules.json`
(their indices 7 and 8 then stay unused), or kept as the record of the build's outputs?
