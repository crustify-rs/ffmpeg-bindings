# baseline-cov — existing safe FFmpeg bindings, measured against FFmpeg 9.0

Tests, Rust and C coverage, API coverage, the exported unsafe surface and
sanitizer use for the two maintained safe-wrapper crates. It complements
`ffmpeg-wrap/crustify/baseline.md` (`2026-09-10`), which scored Rust coverage and `libavutil` API
coverage only; that file is left as it was. Here each crate is measured whole,
against the FFmpeg release both target, with the `libavutil`-only view
(`src/util/`) repeated so the figures stay comparable with `ffmpeg-wrap/crustify/baseline.md` and
`results.md`.

Measured `2026-10-12`. **ftt** — `ffmpeg-the-third`. **next** — `ffmpeg-next`.
Figures in backticks; LoC excludes blank and `//` lines. Coverage is measured
as the campaign measures its own crates: Rust without test code, C with gcov
over a gcc `--coverage` build (section 2).

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
serves the tests, the Rust coverage and the scans. The C coverage comes from a
second build of the same tree with gcc's `--toolchain=gcov` (section 2).

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

Measured the way the campaign measures its own crates (`results.md`, "How the
rows were measured"), so the two are like for like:

- **Rust** is line and function coverage from `-C instrument-coverage` over the
  crate's own `src/`, excluding every line inside a `#[cfg(test)]` item, so the
  inline unit tests are not counted as code they cover themselves.
- **C** is `gcov` line coverage from a gcc `--coverage` build of the target —
  FFmpeg's `--toolchain=gcov`, as in the campaign's `coverage` prebuild — with a
  zero baseline from every `.gcno`, so a TU no test reaches still counts.

The two come from separate runs of the same whole suite, because the Rust side
needs LLVM's instrumentation and the C side gcc's.

### Rust, the crate's own `src/`, test code excluded

One LLVM-instrumented run (lib + `tests/`; doctests are not instrumented by
stable `cargo llvm-cov`, which leaves out ftt's `2`), exported as LCOV, then
every line inside a `#[cfg(test)]` item removed (brace-matched; the git2
baseline's `lcov_nontest.py`, with the source root as a parameter):

```sh
export PKG_CONFIG_PATH=/ff/prefix/lib/pkgconfig      # the clang-built n9.0 below
cargo llvm-cov --no-report --features static --no-fail-fast
cargo llvm-cov report --lcov --output-path rust-<crate>.lcov
python3 lcov_nontest.py rust-<crate>.lcov src=<crate>/src/ util=<crate>/src/util/
```

| crate | scope | files (`0%`) | test lines removed | LoC | function |
|---|---|---:|---:|---:|---:|
| ftt | `src/` | `115` (`80`) | `459` | **`10.93%`** (`854`/`7813`) | `13.38%` (`147`/`1099`) |
| ftt | `src/util/` | `35` (`18`) | `332` | **`19.59%`** (`506`/`2583`) | `20.49%` (`67`/`327`) |
| next | `src/` | `108` (`71`) | `51` | **`18.76%`** (`1580`/`8421`) | `21.16%` (`234`/`1106`) |
| next | `src/util/` | `32` (`15`) | `11` | **`28.32%`** (`779`/`2751`) | `33.53%` (`111`/`331`) |

LoC is LCOV line coverage, function is LCOV function coverage. LCOV carries no
regions, so there is no block figure after the cut.

The cut matters for ftt, which keeps all its tests inline: `459` of its
instrumented `src/` lines are test code, nearly all of it executed by the tests
it contains. With them counted, ftt read `15.66%` over `src/` and `28.65%` over
`src/util/`; next, whose tests are almost all in `tests/`, read `19.23%` and
`28.54%`. ftt's whole-file figures are exactly what `ffmpeg-wrap/crustify/baseline.md`
reported; whole-file figures overstate both crates, ftt most.

### C, per FFmpeg library, gcov

A second build of the same `n9.0` tree with gcc `13.3.0` and FFmpeg's gcov
toolchain, otherwise configured as the clang build:

```sh
configure --prefix=/ff/prefix-gcov --toolchain=gcov \
  --disable-autodetect --disable-programs --disable-doc \
  --enable-static --disable-shared --enable-pic
lcov --capture --initial -d /ff/build-gcov -o base.info      # zero baseline, every .gcno
lcov --zerocounters -d /ff/build-gcov
PKG_CONFIG_PATH=/ff/prefix-gcov/lib/pkgconfig \
RUSTFLAGS="-C link-arg=$(gcc -print-file-name=libgcov.a)" \
RUSTDOCFLAGS="-C link-arg=$(gcc -print-file-name=libgcov.a)" \
  cargo test --features static --no-fail-fast                # lib + tests/ + doctests
lcov --capture -d /ff/build-gcov -o run.info
lcov -a base.info -a run.info -o <crate>.info
```

The Rust side is uninstrumented here; `libgcov.a` is linked by path because the
link driver is clang, whose `--coverage` would bring LLVM's runtime instead. The
suites ran as in section 1: ftt `43` + `1` doctest pass with the same three
encoder failures, next `48` pass. Each library's denominator is the lines of its
compiled `.c` TUs, `2,112` across the seven (headers inlined into them reported
apart below):

| library | TUs | lines | ftt LoC (TUs at `0%`) | next LoC (TUs at `0%`) |
|---|---:|---:|---:|---:|
| `libavutil` | `99` | `13,626` | **`6.95%`** (`947`; `84`) | **`10.74%`** (`1464`; `73`) |
| `libavcodec` | `1,006` | `243,125` | `0.19%` (`462`; `994`) | `0.28%` (`674`; `990`) |
| `libavformat` | `523` | `110,740` | `0.02%` (`22`; `522`) | `3.70%` (`4094`; `243`) |
| `libavfilter` | `416` | `77,902` | `0.02%` (`17`; `414`) | `2.00%` (`1560`; `406`) |
| `libavdevice` | `15` | `1,449` | `0.00%` (`0`; `15`) | `0.35%` (`5`; `13`) |
| `libswscale` | `38` | `15,255` | `0.00%` (`0`; `38`) | `0.00%` (`0`; `38`) |
| `libswresample` | `15` | `1,927` | `0.00%` (`0`; `15`) | `0.00%` (`0`; `15`) |
| **Σ** | **`2,112`** | **`464,024`** | **`0.31%`** (`1448`) | **`1.68%`** (`7797`) |

Counting the header lines inlined into those TUs as well (`469,767` lines over
`2,305` files) moves the totals to `0.31%` and `1.70%`, and `libavutil` to
`6.86%` and `10.70%`.

The difference between the crates follows where their tests sit. All of ftt's
are lib unit tests, `30` of them under `util::`. next's integration tests
include `tests/stream_io.rs`, which exercises custom stream I/O through format
contexts, and `tests/buffersink_reuse.rs`, a filter-graph test. Those are the
tests that can reach `libavformat` and `libavfilter`. Neither crate's tests
reach `libswscale` or `libswresample` at all.

### Superseded: LLVM source-based C coverage

The first measurement read the C from the same LLVM run as the Rust, with the
clang build instrumented by `-fprofile-instr-generate -fcoverage-mapping` and
`llvm-cov` over every object file. It counts lines differently from gcov (any
line a mapped region touches, headers included), so it is not comparable with
the campaign. For the record it gave ftt / next `6.68%` / `10.02%` over
`libavutil` (`18,710` lines) and `0.30%` / `1.65%` over all seven libraries
(`621,915` lines); gcov puts them at `6.95%` / `10.74%` and `0.31%` / `1.68%`.

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

`crustify scan-unsafe`'s rustc driver at crustify `039884b`, which counts only
what the scanned crate defines, by rustc's effective visibility (reachable from
outside the crate) and, for fn signatures, by safety; methods implementing
another crate's trait are left out, since that crate fixed their signature.
Run with the `static` feature against the FFmpeg n9.0 prefix, keeping only the
safe crate's emission (`/data/marius.momeu/baseline-cov/scan_surface.py`):
`scan-unsafe` skips only crates named `*_sys`, and ftt's sys crate is
`ffmpeg_sys_the_third`. In both runs the safe crate was the only crate that
emitted.

| metric | ftt | next |
|---|---:|---:|
| `code_lines` | `10,988` | `12,122` |
| `safe_fns_pub` / `safe_fns_priv` | `1,254` / `472` | `1,049` / `387` |
| **`unsafe_fns_pub`** (public unsafe surface) | **`82`** | **`111`** |
| `unsafe_fns_priv` | `13` | `7` |
| raw-pointer positions, all six counters | `162` | `178` |
| ↳ `raw_ptr_ty_pub` / `_ty_priv` (fields) | **`41`** / `0` | **`49`** / `0` |
| ↳ `raw_ptr_fn_pub_safe` / `_fn_pub_unsafe` | **`43`** / `66` | **`20`** / `96` |
| ↳ `raw_ptr_fn_priv_safe` / `_fn_priv_unsafe` | `8` / `4` | `4` / `9` |
| **raw pointers on the safe public interface** (`_ty_pub` + `_fn_pub_safe`) | **`84`** | **`69`** |
| `*c_void` positions: `void_ptr_ty_pub` / `_fn_pub_safe` / `_fn_pub_unsafe` / private (`_ty_priv` + `_fn_priv_*`) | `3` / `0` / `0` / `1` | `1` / `2` / `0` / `8` |
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
- **raw pointers on the safe public interface** — fields: the pointer each
  public wrapper or iterator holds (ftt `41` over `31` types, next `49` over
  `34`; `Context` holds `5` in both). Safe fn signatures: ftt `43` (`as_ptr`
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

Figures from the earlier campaign's `ffmpeg-wrap/crustify/results.md` (branch `crustify/libavutil-gpt-5.6-sol`), restated, not re-measured; this campaign's own `results.md` is not compared yet. The campaign wraps
`libavutil` only, at FFmpeg `8.0-dev` (`1019f8f036`), so the like-for-like
columns here are the `libavutil` ones.

| | crustify `libavutil` (`8.0-dev`) | ftt (`9.0`) | next (`9.0`) |
|---|---:|---:|---:|
| tests | `1,028` (UB `334`, equiv `177`, unit `517`) | `47` (`30` lib tests in `util::`) | `48` (`2` lib tests in `util::`) |
| tests under any sanitizer | UB tests | `0` | `0` |
| C LoC coverage, `libavutil` (gcov) | `53.25%` (all tests) | `6.95%` | `10.74%` |
| Rust LoC coverage, test code excluded | `93.19%` (all tests) | `19.59%` (`src/util/`) | `28.32%` (`src/util/`) |
| wrapped / covered types | `101` of `181` (`56%`) | `23` of `182` (`12.6%`) | `26` of `182` (`14.3%`) |
| wrapped / covered symbols | `335` of `671` (`50%`) | `91` of `669` (`13.6%`) | `74` of `669` (`11.1%`) |
| `code_lines` | `10,616` | `10,988` (whole crate) | `12,122` (whole crate) |
| public `unsafe fn` (`unsafe_fns_pub`)¹ | `32` | `82` | `111` |
| raw pointers on the safe public interface (`raw_ptr_ty_pub` + `raw_ptr_fn_pub_safe`)¹ | `32` (`12` + `20`) | `84` (`41` + `43`) | `69` (`49` + `20`) |
| raw pointers in public `unsafe fn`s¹ | `19` | `66` | `96` |
| all raw-pointer positions (sum of the six `raw_ptr_*` partitions)¹ | `538` | `162` | `178` |
| calls to C | `374` | `264` | `304` |

How to read it:

- The baselines' unsafe surface is the whole crate, across seven libraries;
  the campaign's is `libavutil` alone.
- Both C figures are gcov line coverage over `libavutil`'s compiled TUs with a
  zero baseline, but over different builds: the campaign's `coverage` prebuild
  of `8.0-dev` (shared, `--enable-zlib`; `94` TUs, `12,558` lines) and this
  static `n9.0` build (`99` TUs, `13,626` lines).
- Both Rust figures exclude `#[cfg(test)]` code; the campaign's also excludes
  `tests/`, which these crates' `src/` scope already leaves out.
- ¹ The unsafe-surface rows are not restated: all three are fresh scans with
  the same driver (crustify `039884b`), the campaign's at the active campaign's
  tip (`ffmpeg-wrap-20261005`, `crustify/campaigns/20261005` at `1a441943aa`,
  `crustify/campaigns/20261005/scan-unsafe.json`). They count only what each
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
- **The Rust coverage run leaves out doctests**, ftt's `2`; stable
  `cargo llvm-cov` does not instrument them. The gcov run includes them.
- **`--disable-autodetect` costs ftt three test results.** It removes the VP9
  and PNG encoders that ftt's three failing tests expect.
- **C coverage builds differ from the test build.** The C figures come from the
  gcc build, the Rust ones and the test results from the clang build; both are
  the same `n9.0` tree and configure options, and the suites gave the same
  results under both.
- **The `.c`-only C denominator.** Lines of headers inlined into a TU are
  reported apart; counting them changes no figure by more than `0.1`pp.
- **API coverage is an upper bound** (see section 3), and its denominator sees
  only compiled code.
- **Tests are not split into UB / equiv / unit.** Neither crate is laid out in
  those categories, and that classification was deferred.
