# `libavutil/dsp`: `rand` is no native equivalent of `lfg`

## What the earlier plan decided

`crustify/artifacts/subsystems.json`, `libavutil` / `dsp`, as the setup emitted it
(`crustify/tmp/scripts/decompose.py`, `NATIVE`):

```json
"objective": {"objective": "wrap", "rust_native_equivalent": ["rustfft", "rand"]}
```

## Evidence against it

`rust_native_equivalent` lists crates that could replace the subsystem outright, each
checked for its semantics. The dsp API that `rand` would stand for is `libavutil/lfg.h`:
`av_lfg_init`, `av_lfg_init_from_data`, `av_lfg_get`, `av_mlfg_get` and `av_bmg_get`.
These are specified generators, not "a source of random numbers":

- `av_lfg_get` is an additive lagged Fibonacci generator over a 64-entry state
  (`libavutil/lfg.h:54`, `c->state[(c->index-24) & 63] + c->state[(c->index-55) & 63]`),
  and `av_lfg_init` seeds it through MD5 (`av_md5_sum`, `libavutil/lfg.c:40`); `av_bmg_get`
  is a Box-Muller transform over that stream.
- Callers depend on the exact sequence: encoders and filters seed it with fixed values for
  dithering and noise, and their FATE references (`tests/ref/`, run against
  `crustify/fate-suite`) are bit-exact.

`rand` implements none of these generators (its `StdRng` is ChaCha12, `SmallRng` is
Xoshiro), so no `rand` API reproduces an `AVLFG` stream. It fails the semantics check, and
with it the "outright" criterion. `rustfft` stays: it computes the same complex DFTs as
`av_tx`'s FFT types (MIT/Apache-2.0, portable, SIMD-accelerated on x86_64 and aarch64),
though not the MDCT, DCT or fixed-point transforms.

Rerun: read `libavutil/lfg.h` and `libavutil/lfg.c`;
`grep -rln 'av_lfg_init' libavcodec libavfilter | head` lists the seeded callers.

## Edit and downstream effect

`"rust_native_equivalent": ["rustfft"]`. A recorded judgement only: no objective,
subsystem, wave or batch moves, and no schedule is re-emitted.

## Questions for the user

None.
