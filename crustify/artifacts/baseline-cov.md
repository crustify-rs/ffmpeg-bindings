# baseline-cov — existing safe FFmpeg bindings, measured against FFmpeg 9.0

Tests, Rust and C coverage, API coverage, the exported unsafe surface and
sanitizer use for the two maintained safe-wrapper crates. It complements
`ffmpeg-wrap/crustify/baseline.md` (`2026-09-10`), which scored Rust coverage and `libavutil` API
coverage only; that file is left as it was. Here each crate is measured whole,
against the FFmpeg release both target, with the `libavutil`-only view
(`src/util/`) repeated so the figures stay comparable with `ffmpeg-wrap/crustify/baseline.md` and
`dashboard.md`.

Measured `2026-10-12`; test coverage re-measured `2026-11-04`. **ftt** — `ffmpeg-the-third`. **next** — `ffmpeg-next`.
Figures in backticks; LoC excludes blank and `//` lines. Coverage is measured
as the campaign measures its own crates: C and Rust with LLVM's source-based coverage,
Rust without test code (section 2).

## 0. Subjects

| | ftt | next |
|---|---|---|
| checkout | `crustify-rs/ffmpeg-the-third` | `crustify-rs/rust-ffmpeg` |
| upstream | `github.com/shssoichiro/ffmpeg-the-third` | `github.com/zmwangx/rust-ffmpeg` |
| revision | `47eb652` (`2026-08-08`) | `9b9c0b9` (`2026-10-09`) |
| describe | `v5.0.0-2-g47eb652` | `v9.0.0-10-g9b9c0b9` |
| crate version | `6.0.0+ffmpeg-9.0` | `9.0.1` |
| sys crate | `ffmpeg-sys-the-third` `6.0.0+ffmpeg-9.0` (path dep) | `ffmpeg-sys-next` `9.0.1` (crates.io) |
| edition / MSRV | `2021` / `1.80.0` | `2024` |
| `.rs` files under `src/` | `152` | `139` |
| features | default + `static` | default + `static` |

Both checkouts were clean and are unmodified.

**FFmpeg** — `n9.0` (`d32b387`, `2026-08-03`; `libavutil 61.1`), built once from
source and shared by both crates, as static libraries with clang `23.1.2`:

```sh
configure --prefix=/ff/prefix --cc=/usr/lib/llvm-23/bin/clang \
  --cxx=/usr/lib/llvm-23/bin/clang++ \
  --extra-cflags="-fprofile-instr-generate -fcoverage-mapping" \
  --extra-ldflags="-fprofile-instr-generate" \
  --disable-autodetect --disable-programs --disable-doc \
  --enable-static --disable-shared --enable-pic
```

Every native component stays at its default; `--disable-autodetect` keeps out
every external library (zlib, libvpx, …) and every hardware backend. This build
serves the tests, the C and Rust coverage and the scans; the coverage was measured on
a rebuild of it, as section 2 describes.

Both sys crates were pointed at it with `PKG_CONFIG_PATH=/ff/prefix/lib/pkgconfig`
and the `static` feature, not their vendored `build` feature. The image has no
system `libav*` packages, and each sys crate's build-script output confirms the
link: `rustc-link-search=native=/ff/prefix/lib`, `rustc-link-lib=static=` for all
seven libraries, and `ffmpeg_9_0` enabled.

Environment: the `crustify` image (Ubuntu noble), `rustc 1.99.0` stable (LLVM
`23.1.1`), `cargo-llvm-cov 0.9.1`.

## 1. Tests

From `cargo test --features static -- --list`, and a full run:

| target | ftt | next |
|---|---:|---:|
| lib unit tests (`#[cfg(test)]` in `src/`) | `45` | `7` |
| ↳ under `util::` | `30` | `2` |
| ↳ under `codec::` / `format::` / `filter::` | `12` / `2` / `1` | `3` / `1` / `1` |
| `tests/stream_io.rs` | — | `21` |
| `tests/side_data_enum_validity.rs` | — | `8` |
| `tests/side_data_from_libavutil.rs` | — | `4` |
| `tests/audio_frame_slices.rs` | — | `4` |
| `tests/buffersink_reuse.rs` | — | `2` |
| `tests/send_rc_race.rs` | — | `2` |
| doctests | `2` | `0` |
| **total** | **`47`** | **`48`** |

| run | ftt | next |
|---|---|---|
| lib | `43` pass, `2` fail | `7` pass |
| `tests/` | — | `41` pass |
| doctests | `1` pass, `1` fail | — |

All three ftt failures are environmental, and the same two lib failures
`ffmpeg-wrap/crustify/baseline.md` recorded: `codec::config::test::video_encoder` and the doctest on
`Supported<I>::all` (`src/codec/config.rs:34`) look up a VP9 encoder, and
`codec::config::test::alpha_modes` a PNG encoder. VP9 encoding needs libvpx and
PNG needs zlib, both kept out by `--disable-autodetect`. Nothing is ignored in
either crate.

## 2. Test coverage

Re-measured `2026-11-04` with the testing spec's Coverage method, the one the campaign's
`dashboard.md` Overview now uses, which replaces the gcov and `cargo llvm-cov` measurement
of `2026-10-12`. Those figures are not comparable with these. C and Rust are counted by
one instrument. The `n9.0` tree is built once more as section 0 records, clang 23 with
`-fprofile-instr-generate -fcoverage-mapping`, static, and both crates link it through
`PKG_CONFIG_PATH` and their `static` feature. Each crate is compiled on
`nightly-2026-10-08` (LLVM 23) with `-C instrument-coverage`, in a scratch copy whose test
items carry `#[coverage(off)]`, so the checkouts stay unmodified. Each suite runs alone into
its own profile: the `--lib` unit tests, each `tests/` binary and the doctests, and the
union merges them all. Every figure is the `TOTAL` line of the nightly's `llvm-cov report`,
with every test and doctest binary and the seven static archives as objects for every
suite. The archives carry every TU the build compiled, so that one no test binary links
counts at zero.

```sh
# from the ffmpeg-bindings checkout, scripts under crustify/scripts/coverage/, with
# b=crustify/tmp/baseline-ffmpeg holding the n9.0 build/ and prefix/, absolute
PKG_CONFIG_PATH=$b/prefix/lib/pkgconfig baseline.sh /crustify-rs/ffmpeg-the-third \
    ffmpeg-the-third $b/ftt nightly-2026-10-08 --features static
NIGHTLY=nightly-2026-10-08 report.py $b/ftt \
    "libavutil=$b/build/src/libavutil@$b/build,Rust=$b/ftt/tree/src" --c $b/prefix/lib/*.a
# and likewise rust-ffmpeg's ffmpeg-next into $b/next
```

The suites gave section 1's results, but for one more ftt failure under the nightly:
`util::dictionary::tests::test_failing` is a trybuild case whose expected stderr is
stable's. It runs no code, so it adds nothing to either figure.

### Rust, the crate's own `src/`, test code excluded

| crate | scope | lines | functions |
|---|---|---:|---:|
| ftt | `src/` | **`11.07%`** (`839`/`7581`) | `13.39%` (`147`/`1098`) |
| ftt | `src/util/` | **`19.23%`** (`486`/`2527`) | `20.25%` (`66`/`326`) |
| next | `src/` | **`18.73%`** (`1528`/`8158`) | `21.16%` (`234`/`1106`) |
| next | `src/util/` | **`27.72%`** (`746`/`2691`) | `33.53%` (`111`/`331`) |

Test items are excluded by the attribute rather than cut from the report. ftt keeps all its
tests inline, and its two doctests add `20` lines to its `--lib` figure of `10.80%`. next's
tests are almost all in `tests/`, where `stream_io` reaches the most, `8.97%`.

### C, per FFmpeg library

| library | lines | ftt lines | next lines | ftt functions | next functions |
|---|---:|---:|---:|---:|---:|
| `libavutil` | `17,651` | **`7.09%`** (`1251`) | **`10.71%`** (`1891`) | `8.97%` | `16.45%` |
| `libavcodec` | `327,606` | `0.18%` (`575`) | `0.32%` (`1047`) | `0.51%` | `0.93%` |
| `libavformat` | `144,091` | `0.02%` (`28`) | `3.85%` (`5552`) | `0.04%` | `10.92%` |
| `libavfilter` | `103,954` | `0.02%` (`25`) | `1.87%` (`1943`) | `0.12%` | `3.87%` |
| `libavdevice` | `1,940` | `0.00%` (`0`) | `0.36%` (`7`) | `0.00%` | `2.53%` |
| `libswscale` | `22,364` | `0.00%` (`0`) | `0.00%` (`0`) | `0.00%` | `0.00%` |
| `libswresample` | `2,384` | `0.00%` (`0`) | `0.00%` (`0`) | `0.00%` | `0.00%` |
| **all seven** | `619,990` | **`0.30%`** (`1879`) | **`1.68%`** (`10440`) | `0.72%` | `4.39%` |

Lines are counted over every `.c` and header file a coverage region maps code to, each
library over its own directory, so the denominators are larger than gcov's. llvm-cov drops
the counts of a few functions whose data mismatch between binaries, which moves a
denominator by at most `22` lines between the two crates; the table gives ftt's.

The difference between the crates follows where their tests sit. All of ftt's
are lib unit tests, `30` of them under `util::`. next's integration tests
include `tests/stream_io.rs`, which exercises custom stream I/O through format
contexts, and `tests/buffersink_reuse.rs`, a filter-graph test. Those are the
tests that can reach `libavformat` and `libavfilter`. Neither crate's tests
reach `libswscale` or `libswresample` at all.

## 3. API coverage

### Denominator — wavefront over FFmpeg 9.0

A CodeQL database of the `n9.0` tree, built with the configure line above minus
the coverage flags (`codeql database create --language=cpp`, `4m06s`), and
wavefront `ac2f85c` (`extract-ql`: `22` queries ok). One config per link unit:
that library's sources as `impl_files`, its `Makefile` `HEADERS` as
`api_headers`, and every other library out of scope; `compat/` joins
`libavutil`, as in this campaign's config. Queries as in `ffmpeg-wrap/crustify/baseline.md`'s Oracle
section:

```sh
wavefront <n9.0> --config <unit>/wavefront-config.json query types   --api-only
wavefront <n9.0> --config <unit>/wavefront-config.json query symbols --api-only --targeted-only
```

The `libavutil` header set is the campaign's exactly, less the two generated
headers (`avconfig.h`, `ffversion.h`), which hold only macros and so add nothing.

| link unit | headers | types | ↳ struct / enum | symbols | ↳ exported fns / inline fns / globals / callbacks |
|---|---:|---:|---|---:|---|
| `libavutil` | `100` | `182` | `133` / `49` | `669` | `588` / `66` / `10` / `5` |
| `libavcodec` | `25` | `50` | `36` / `14` | `157` | `157` / `0` / `0` / `0` |
| `libavformat` | `4` | `31` | `25` / `6` | `145` | `144` / `1` / `0` / `0` |
| `libavfilter` | `5` | `14` | `14` / `0` | `68` | `66` / `0` / `0` / `2` |
| `libavdevice` | `3` | `5` | `3` / `2` | `14` | `14` / `0` / `0` / `0` |
| `libswscale` | `3` | `9` | `3` / `6` | `40` | `40` / `0` / `0` / `0` |
| `libswresample` | `3` | `4` | `1` / `3` | `22` | `22` / `0` / `0` / `0` |
| **whole tree** | **`143`** | **`287`** | `207` / `80` | **`1,116`** | `1031` / `67` / `10` / `8` |

The whole-tree row is one config over all seven libraries. A type published by
several libraries' headers is counted once there, which is why the rows sum to
more types (`295`) than it.

`libavutil` moved little from the campaign's `8.0-dev` oracle (`181` types,
`671` symbols) to `n9.0` (`182`, `669`). Like that oracle, this one sees only
compiled code, so headers for the hardware backends `--disable-autodetect`
leaves out are absent. That is presumably most of the gap between `libavcodec`'s
`157` functions here and the `181` that `ffmpeg-wrap/crustify/baseline.md`'s header parse of `8.0-dev`
found; I did not reconcile it here.

### Numerator — identifier scan

Exactly `ffmpeg-wrap/crustify/baseline.md`'s "Method — API coverage": an entity counts when the
crate's `src/**/*.rs` names it as a whole word, after removing `#[cfg(test)]`
modules (brace-matched; ftt's `#[cfg(all(test, feature = ..))]` modules
included) and `//` line comments. It is an upper bound: a mention scores whether
or not it is reachable from the safe API. Scanning is version-blind.

| link unit | API syms | ftt | next | both | ftt only | next only | union | union % |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| `libavutil` | `669` | `91` (`13.6%`) | `74` (`11.1%`) | `69` | `22` | `5` | `96` | `14.3%` |
| `libavcodec` | `157` | `38` (`24.2%`) | `35` (`22.3%`) | `35` | `3` | `0` | `38` | `24.2%` |
| `libavformat` | `145` | `27` (`18.6%`) | `30` (`20.7%`) | `25` | `2` | `5` | `32` | `22.1%` |
| `libavfilter` | `68` | `22` (`32.4%`) | `24` (`35.3%`) | `22` | `0` | `2` | `24` | `35.3%` |
| `libavdevice` | `14` | `10` (`71.4%`) | `10` (`71.4%`) | `10` | `0` | `0` | `10` | `71.4%` |
| `libswscale` | `40` | `17` (`42.5%`) | `17` (`42.5%`) | `17` | `0` | `0` | `17` | `42.5%` |
| `libswresample` | `22` | `8` (`36.4%`) | `8` (`36.4%`) | `8` | `0` | `0` | `8` | `36.4%` |
| **whole tree** | **`1,116`** | **`213`** (`19.1%`) | **`198`** (`17.7%`) | `186` | `27` | `12` | `225` | `20.2%` |

| link unit | API types | ftt | next | both | ftt only | next only | union | union % |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| `libavutil` | `182` | `23` (`12.6%`) | `26` (`14.3%`) | `20` | `3` | `6` | `29` | `15.9%` |
| `libavcodec` | `50` | `17` (`34.0%`) | `16` (`32.0%`) | `16` | `1` | `0` | `17` | `34.0%` |
| `libavformat` | `31` | `8` (`25.8%`) | `9` (`29.0%`) | `8` | `0` | `1` | `9` | `29.0%` |
| `libavfilter` | `14` | `5` (`35.7%`) | `5` (`35.7%`) | `5` | `0` | `0` | `5` | `35.7%` |
| `libavdevice` | `5` | `2` (`40.0%`) | `2` (`40.0%`) | `2` | `0` | `0` | `2` | `40.0%` |
| `libswscale` | `9` | `4` (`44.4%`) | `4` (`44.4%`) | `4` | `0` | `0` | `4` | `44.4%` |
| `libswresample` | `4` | `4` (`100%`) | `4` (`100%`) | `4` | `0` | `0` | `4` | `100%` |
| **whole tree** | **`287`** | **`60`** (`20.9%`) | **`63`** (`22.0%`) | `56` | `4` | `7` | `67` | `23.3%` |

`libavutil` reproduces `ffmpeg-wrap/crustify/baseline.md` for ftt (`91` symbols, `23` types) and for
next's symbols (`74`, the same five next-only symbols). next's types rose from
`20` to `26`: the six it alone covers are `AVStereo3D`, `AVStereo3DType`,
`AVStereo3DView`, `AVStereo3DPrimaryEye`, `AVSphericalMapping` and
`AVSphericalProjection`, from `dba9764` ("Add wrappers for Stereo3D/Spherical
structures"), which postdates the `3751e28` that `ffmpeg-wrap/crustify/baseline.md` measured. ftt's
three are `AVAlphaMode`, `AVChannel` and `AVChannelCustom`.

Both crates cover the small link units well and the large ones thinly. That is
the shape of a media pipeline: open, demux, decode, filter, scale, resample,
encode, mux. The `libavutil` gap `ffmpeg-wrap/crustify/baseline.md` describes still holds:
cryptography, hashes, buffers and FIFOs, string helpers, `hwcontext` and the
HDR side-data family stay untouched.

## 4. Unsafe surface

`crustify ffimap`'s rustc driver at crustify `7a84ee6`, which counts only
what the scanned crate defines, by rustc's effective visibility (reachable from
outside the crate) and, for fn signatures, by safety; methods implementing
another crate's trait are left out, since that crate fixed their signature.
Run with the `static` feature against the FFmpeg n9.0 prefix, keeping only the
safe crate's emission (`/data/marius.momeu/baseline-cov/scan_surface.py`):
`ffimap` skips only crates named `*_sys`, and ftt's sys crate is
`ffmpeg_sys_the_third`. In both runs the safe crate was the only crate that
emitted.

| metric | ftt | next |
|---|---:|---:|
| `code_lines` | `10,988` | `12,122` |
| `safe_fns_pub` / `safe_fns_priv` | `1,254` / `472` | `1,049` / `387` |
| **`unsafe_fns_pub`** (public unsafe surface) | **`82`** | **`111`** |
| `unsafe_fns_priv` | `13` | `7` |
| raw-pointer positions, all six counters | `162` | `178` |
| ↳ `raw_ptr_ty_pub_fields_pub` / `_ty_pub_fields_priv` / `_ty_priv` (fields) | **`0`** / `41` / `0` | **`0`** / `49` / `0` |
| ↳ `raw_ptr_fn_pub_safe` / `_fn_pub_unsafe` | **`43`** / `66` | **`20`** / `96` |
| ↳ `raw_ptr_fn_priv_safe` / `_fn_priv_unsafe` | `8` / `4` | `4` / `9` |
| **raw pointers on the safe public interface** (`_ty_pub_fields_pub` + `_fn_pub_safe`) | **`43`** | **`20`** |
| `*c_void` positions: `void_ptr_ty_pub_fields_priv` / `_fn_pub_safe` / `_fn_pub_unsafe` / private (`_ty_priv` + `_fn_priv_*`) | `3` / `0` / `0` / `1` | `1` / `2` / `0` / `8` |
| `unsafe` blocks | `667` | `633` |
| `unsafe impl` | `58` | `67` |
| calls to C | `264` | `304` |

What the public figures are made of, by name:

- **`unsafe_fns_pub`** — ftt: `wrap` `22`, `as_ptr` `19`, `as_mut_ptr` `18`,
  `from_raw` `12`, then `is_empty`, `from_ptr`, `new`, `alloc` (`2` each),
  `empty`, and `Frame::as_ref` / `as_mut`, which hand out `&AVFrame` /
  `&mut AVFrame`, a reference into C memory. next: `wrap` `38`, `as_ptr`
  `29`, `as_mut_ptr` `26`, `new` `4`, `wrap_with_*` `5`, `is_empty` `2`,
  `alloc` `2`, `own`, `disown`, `destructor`, `new_with_interrupt`, `empty`.
  Both crates mark the pointer accessors `as_ptr` / `as_mut_ptr` `unsafe`,
  which std does not (`Vec::as_ptr` is safe): over half of each figure is
  accessors that hand out a pointer rather than take one.
- **raw pointers on the safe public interface** — fn signatures only: no
  public type has a public raw-pointer field. The pointers public wrappers and
  iterators hold are private (`raw_ptr_ty_pub_fields_priv`: ftt `41` over `31`
  types, next `49` over `34`; `Context` holds `5` in both). Safe fn
  signatures: ftt `43` (`as_ptr`
  `24`, `as_mut_ptr` `12` on types that also have the `unsafe` ones, `new` `4`,
  `from_av_rects` `2`, `into_raw` `1`), next `20` (`new` `11`, `as_ptr` `6`,
  `as_mut_ptr` `3`). The safe `pub fn new(ptr: *const ...)` constructors take
  an FFmpeg pointer without `unsafe` — the iterators over codec profiles,
  filter pads, packet side data and dictionaries in both, plus in next those
  over subtitle rects, sample rates, sample formats, channel layouts, frame
  rates and pixel formats — and so does ftt's `from_av_rects(&[*mut
  AVSubtitleRect])`. A caller reaches them from safe code with any pointer:
  the soundness-relevant part of each surface.
- **`raw_ptr_fn_pub_unsafe`** — the parameters and returns of `wrap`,
  `from_raw` and the `unsafe` `as_ptr` / `as_mut_ptr` (ftt `66`, next `96`).

The scan is name-blind: these are read off the `--sites` of the counters, not
used to classify.

## 5. Sanitizers

| instrument | ftt | next |
|---|---:|---:|
| ASan | `0` | `0` |
| MSan | `0` | `0` |
| TSan | `0` | `0` |
| UBSan | `0` | `0` |
| Miri | `0` | `0` |
| BSan | `0` | `0` |

No test in either crate targets any instrument:

- **CI.** ftt's `.github/workflows/build.yml` runs `cargo fmt --check`,
  `cargo clippy`, `cargo build --all-targets` and `cargo test` over a matrix of
  FFmpeg versions on Linux, macOS and Windows. next's runs `cargo build
  --examples`, `cargo test --tests --examples`, `cargo clippy` and `cargo fmt`
  in `jrottenberg/ffmpeg` containers for FFmpeg `3.4` onwards, plus macOS and
  Windows jobs. `release.yml` in each only
  publishes. Neither passes `-Zsanitizer`, runs `cargo miri`, sets a sanitizer
  `RUSTFLAGS`, or uses valgrind.
- **Source.** Neither crate has a `cfg(miri)` or `cfg_attr(miri, ..)` site, and
  no `.rs`, `.toml`, `.yml`, `.sh` or `.md` file mentions a sanitizer, Miri or
  valgrind, with one exception: `rust-ffmpeg/tests/stream_io.rs:541`, a comment
  in `read_buffer_handed_to_the_stream_is_initialized_and_readable` noting that
  reading uninitialised memory would be UB and that "under Miri/ASan this test
  would catch a regression". It describes what such a run could catch. Nothing
  configures one, so the test runs only as an ordinary `cargo test`.

## 6. Against this campaign

Figures from the earlier campaign's `ffmpeg-wrap/crustify/results.md` (branch `crustify/libavutil-gpt-5.6-sol`), restated, not re-measured; this campaign's own `dashboard.md` is not compared yet. The campaign wraps
`libavutil` only, at FFmpeg `8.0-dev` (`1019f8f036`), so the like-for-like
columns here are the `libavutil` ones.

| | crustify `libavutil` (`8.0-dev`) | ftt (`9.0`) | next (`9.0`) |
|---|---:|---:|---:|
| tests | `1,028` (UB `334`, equiv `177`, unit `517`) | `47` (`30` lib tests in `util::`) | `48` (`2` lib tests in `util::`) |
| tests under any sanitizer | UB tests | `0` | `0` |
| C LoC coverage, `libavutil`, source-based | see below | `7.09%` | `10.71%` |
| Rust LoC coverage, test code excluded, source-based | see below | `19.23%` (`src/util/`) | `27.72%` (`src/util/`) |
| wrapped / covered types | `101` of `181` (`56%`) | `23` of `182` (`12.6%`) | `26` of `182` (`14.3%`) |
| wrapped / covered symbols | `335` of `671` (`50%`) | `91` of `669` (`13.6%`) | `74` of `669` (`11.1%`) |
| `code_lines` | `10,616` | `10,988` (whole crate) | `12,122` (whole crate) |
| public `unsafe fn` (`unsafe_fns_pub`)¹ | `32` | `82` | `111` |
| raw pointers on the safe public interface (`raw_ptr_ty_pub_fields_pub` + `raw_ptr_fn_pub_safe`)¹ | `20` (`0` + `20`) | `43` (`0` + `43`) | `20` (`0` + `20`) |
| raw pointers held in private fields of public types (`raw_ptr_ty_pub_fields_priv`)¹ | `12` | `41` | `49` |
| raw pointers in public `unsafe fn`s¹ | `19` | `66` | `96` |
| all raw-pointer positions (sum of the six `raw_ptr_*` partitions)¹ | `538` | `162` | `178` |
| calls to C | `374` | `264` | `304` |

How to read it:

- The baselines' unsafe surface is the whole crate, across seven libraries;
  the campaign's is `libavutil` alone.
- The earlier campaign's coverage was gcov's, and is not restated. The active campaign's,
  measured by section 2's method on `2026-11-04` at `d7652675f5e`, reaches `67.20`% of
  `libavutil`'s lines (`12417`/`18477`, its `coverage` variant of `8.0-dev`, shared,
  `--enable-zlib`) with its four suites, against ftt's `7.09`% and next's `10.71`% of
  `n9.0`'s `17,651`; and `99.07`% of its own non-test Rust lines, against `11.07`% and
  `18.73`% of the crates' whole `src/`.
- Both Rust figures exclude `#[cfg(test)]` code; the campaign's also excludes
  `tests/`, which these crates' `src/` scope already leaves out.
- ¹ The unsafe-surface rows are not restated: all three are fresh scans with
  the same driver (crustify `7a84ee6`), the campaign's at the active campaign's
  tip (`ffmpeg-wrap-20261005`, `crustify/main` at `f90c4f03bb`,
  `crustify/artifacts/ffimap.json`). They count only what each
  crate defines, so ffibox's own conversions and the `pub(crate)` constructors
  `define_ctype!` emits are outside the campaign's public figures by
  construction, while the baselines' own `wrap` / `from_raw` / `as_ptr` are
  inside theirs.

## 7. Notes and limitations

- **Relation to `ffmpeg-wrap/crustify/baseline.md`.**
  - That file measured Rust coverage only, scoped API coverage to `libavutil`
    against the campaign's `8.0-dev` oracle, and ran next at `3751e28` (`9.0.0`)
    with FFmpeg either from Ubuntu (`7.1.1`) or vendored (`9.0`, default
    configure).
  - Here next is at `9b9c0b9` (`9.0.1`), FFmpeg is one shared `n9.0` build with
    `--disable-autodetect`, and both crates link it statically.
  - ftt's revision is unchanged, and its whole-file Rust figures reproduce
    `ffmpeg-wrap/crustify/baseline.md`'s vendored-`9.0` row exactly (`15.66%` LoC, `18.77%` block,
    `16.27%` function). That file counted test code; without it ftt is at
    `10.93%` (section 2).
  - next's whole-file figure moved from `16.26%` to `19.23%` LoC, with `12`
    more tests (`36` → `48`) and its `src/util/` LoC coverage moving from
    `20.70%` to `28.54%` (`18.76%` and `28.32%` without test code).
    The ten commits between the two revisions (`v9.0.0..9b9c0b9`) add the
    Stereo3D/Spherical wrappers, extradata and video accessors, and soundness
    fixes; this file does not attribute the change to any one of them.
- **Doctests are a suite of their own.** They run on the nightly, instrumented like the
  test binaries, and count in both the C and the Rust union.
- **`--disable-autodetect` costs ftt three test results.** It removes the VP9
  and PNG encoders that ftt's three failing tests expect.
- **One build serves the C and the Rust figures.** The `n9.0` clang build is instrumented
  for both, so both come from one run of each suite.
- **The C denominator includes headers.** A header's lines count toward the library whose
  directory holds it, wherever they are compiled.
- **API coverage is an upper bound** (see section 3), and its denominator sees
  only compiled code.
- **Tests are not split into UB / equiv / unit.** Neither crate is laid out in
  those categories, and that classification was deferred.
