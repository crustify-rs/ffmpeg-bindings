# baseline-cov — existing safe FFmpeg bindings, measured against FFmpeg 9.0

Tests, Rust and C coverage, API coverage, the exported unsafe surface and
sanitizer use for the two maintained safe-wrapper crates. It complements
`ffmpeg-wrap/crustify/baseline.md` (`2026-09-10`), which scored Rust coverage and `libavutil` API
coverage only; that file is left as it was. Here each crate is measured whole,
against the FFmpeg release both target, with the `libavutil`-only view
(`src/util/`) repeated so the figures stay comparable with `ffmpeg-wrap/crustify/baseline.md` and
`results.md`.

Measured `2026-10-12`. **ftt** — `ffmpeg-the-third`. **next** — `ffmpeg-next`.
Figures in backticks; LoC excludes blank and `//` lines.

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
every external library (zlib, libvpx, …) and every hardware backend.

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

One instrumented run of each crate's whole suite. Rust and C share one profile:
rustc's `-Cinstrument-coverage` and clang's `-fprofile-instr-generate
-fcoverage-mapping` both emit LLVM 23 source-based coverage, the C objects link
into the test binaries, and one profiling runtime writes both into the same
`.profraw`. That the merged profile carries both was checked directly: C files
appear in the report with non-zero counts (for example next's `libavformat`, at
`5,425` covered lines).

```sh
export PKG_CONFIG_PATH=/ff/prefix/lib/pkgconfig
cargo llvm-cov --no-report --features static --no-fail-fast  # lib + tests/, no doctests
cargo llvm-cov report --json                                 # Rust, and the C linked in
llvm-cov export -summary-only -instr-profile <crate>.profdata \
  -object <each test binary> -object <every .o under /ff/build/lib*>   # whole C libraries
```

Doctests are not in the coverage run: `cargo llvm-cov` instruments them only on
nightly. That leaves out ftt's `2` doctests; next has none.

### Rust, the crate's own `src/`

| crate | scope | files (`0%`) | LoC | block | function |
|---|---|---:|---:|---:|---:|
| ftt | `src/` | `115` (`80`) | `15.66%` (`1300`/`8299`) | `18.77%` (`1839`/`9798`) | `16.27%` (`185`/`1137`) |
| ftt | `src/util/` | `35` (`18`) | `28.65%` (`836`/`2918`) | `31.17%` (`1126`/`3612`) | `26.14%` (`92`/`352`) |
| next | `src/` | `108` (`71`) | `19.23%` (`1634`/`8498`) | `19.88%` (`1953`/`9825`) | `21.72%` (`242`/`1114`) |
| next | `src/util/` | `32` (`15`) | `28.54%` (`790`/`2768`) | `27.84%` (`908`/`3262`) | `33.93%` (`113`/`333`) |

LoC is llvm-cov `Lines`, block is `Regions`, function is `Functions`, as in
`ffmpeg-wrap/crustify/baseline.md`.

### C, per FFmpeg library

Denominator: the whole library, every object file the build produced, whether
the test binaries linked it or not. Static linking pulls in only referenced
objects, but FFmpeg's codec, format and filter registration tables reference
nearly all of them: the linked view's C total is `617,081` lines for ftt and
`620,012` for next, against `621,915` here, so the two views differ by under
`1%`. Headers compiled into a library count with it.

| library | lines | ftt LoC | ftt function | next LoC | next function |
|---|---:|---:|---:|---:|---:|
| `libavutil` | `18,710` | `6.68%` (`1249`) | `8.47%` (`96`/`1133`) | `10.02%` (`1875`) | `15.53%` (`176`/`1133`) |
| `libavcodec` | `328,425` | `0.18%` (`575`) | `0.50%` (`53`/`10524`) | `0.32%` (`1041`) | `0.93%` (`98`/`10524`) |
| `libavformat` | `144,102` | `0.02%` (`28`) | `0.04%` (`2`/`4644`) | `3.76%` (`5425`) | `10.92%` (`507`/`4644`) |
| `libavfilter` | `103,965` | `0.02%` (`25`) | `0.12%` (`5`/`4346`) | `1.87%` (`1942`) | `3.87%` (`168`/`4346`) |
| `libavdevice` | `1,940` | `0.00%` (`0`) | `0.00%` (`0`/`79`) | `0.36%` (`7`) | `2.53%` (`2`/`79`) |
| `libswscale` | `22,375` | `0.00%` (`0`) | `0.00%` (`0`/`950`) | `0.00%` (`0`) | `0.00%` (`0`/`950`) |
| `libswresample` | `2,398` | `0.00%` (`0`) | `0.00%` (`0`/`78`) | `0.00%` (`0`) | `0.00%` (`0`/`78`) |
| **Σ** | **`621,915`** | **`0.30%`** (`1877`) | **`0.72%`** (`156`/`21754`) | **`1.65%`** (`10290`) | **`4.37%`** (`951`/`21754`) |

Block coverage follows the same shape: `0.25%` for ftt and `1.75%` for next over
all seven libraries, and `4.45%` and `6.31%` over `libavutil`.

The difference between the crates follows where their tests sit. All of ftt's
are lib unit tests, `30` of them under `util::`. next's integration tests
include `tests/stream_io.rs`, which exercises custom stream I/O through format
contexts, and `tests/buffersink_reuse.rs`, a filter-graph test. Those are the
tests that can reach `libavformat` (`10.92%` of its functions) and `libavfilter`.
Neither crate's tests reach `libswscale` or `libswresample` at all.

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

`crustify scan-unsafe`'s rustc driver (crustify working tree, `2026-10-12`),
default features. `scan-unsafe` sums every emitted crate except names ending in
`_sys`, and ftt's sys crate is `ffmpeg_sys_the_third`, which that rule misses.
So the driver was run exactly as `driver.measure` runs it (same toolchain,
`RUSTC`, `SYSROOT` and cache bust) and only the safe crate's emission was kept.
In both runs the safe crate was the only crate that emitted, so no sys-crate
figure can be in these totals.

| metric | ftt | next |
|---|---:|---:|
| `code_lines` | `10,988` | `12,122` |
| `unsafe` blocks | `667` | `633` |
| `unsafe impl` | `58` | `67` |
| fns, pub / private | `1,684` / `745` | `1,441` / `604` |
| **`unsafe fn`, pub** | **`82`** | **`111`** |
| `unsafe fn`, private | `13` | `7` |
| raw-pointer positions, args / rets / fields | `47` / `74` / `41` | `63` / `66` / `49` |
| **raw-pointer positions, pub** | **`109`** | **`116`** |
| raw-pointer positions, private | `53` | `62` |
| `*c_void` positions | `4` | `11` |
| calls to C | `264` | `304` |

"pub" is rustc's effective visibility: reachable from outside the crate. A
position is every raw pointer in a parameter, return or field type, nested ones
included. Doc-hidden items count like any other.

The totals above do not depend on any naming; the `static` feature runs used
for the split below reproduce them exactly.

### Seam and smell, by each crate's own conventions

The seam/smell split rests on the seam names the scan is given
(`scan-unsafe --seams`); the scanner assumes none. These are each crate's own,
read off its code: every exported `unsafe fn` and raw-pointer position was
listed by name from a scan with no seam file, and the names that convert
between a safe wrapper and an FFmpeg pointer were kept. One name per line in
`/data/marius.momeu/baseline-cov/seams/ffmpeg-the-third.txt` and
`.../ffmpeg-next.txt`:

| crate | seam fns |
|---|---|
| ftt | `wrap`, `from_raw`, `from_ptr` (adopt); `as_ptr`, `as_mut_ptr`, `into_raw` (expose / surrender); `str_from_c_ptr`, `optional_str_from_c_ptr` (C string to `&str`) |
| next | `wrap`, `wrap_with_interrupt`, `wrap_with_custom_io`, `wrap_with_custom_io_and_interrupt` (adopt); `as_ptr`, `as_mut_ptr` (expose); `own`, `disown` (hand ownership over / back) |

`new` is deliberately not a seam name, although some `new`s adopt a pointer
(`Destructor::new`): the name is too generic to sanction by, and several are
the safe constructors below. The C-ABI gateway is seam by structure.

| | ftt | next |
|---|---:|---:|
| exported `unsafe fn`: seam / smell | `73` / **`9`** | `100` / **`11`** |
| private `unsafe fn`: seam / smell | `10` / `3` | `3` / `4` |
| exported raw-pointer positions: seam / smell | `102` / **`7`** | `103` / **`13`** |
| private raw-pointer positions: seam / smell | `3` / `50` | `6` / `56` |

The exported smell, by fn:

- **`unsafe fn`** — ftt: `is_empty` ×2, `new` ×2, `alloc` ×2, `empty`, and
  `Frame::as_ref` / `as_mut`, which hand out `&AVFrame` / `&mut AVFrame`, a
  reference into C memory; next: `new` ×4, `is_empty` ×2, `alloc` ×2,
  `destructor`, `new_with_interrupt`, `empty`. Mostly constructors over
  uninitialised frames and packets whose validity the caller must establish.
- **raw-pointer positions** — **safe** `pub fn new(ptr: *const ...)`
  constructors, which take an FFmpeg pointer without `unsafe`: the iterators
  over codec profiles, filter pads, packet side data and dictionaries in both,
  plus in next those over subtitle rects, sample rates, sample formats,
  channel layouts, frame rates and pixel formats (ftt `5`, next `12`), plus ftt's safe
  `from_av_rects(&[*mut AVSubtitleRect])` (`2`) and next's
  `new_with_interrupt` (`1`). A caller can reach these from safe code with any
  pointer, so this is the soundness-relevant part of each crate's surface.

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
| C LoC coverage, `libavutil` | `53.25%` (all tests) | `6.68%` | `10.02%` |
| Rust LoC coverage | `93.19%` (all tests) | `28.65%` (`src/util/`) | `28.54%` (`src/util/`) |
| wrapped / covered types | `101` of `181` (`56%`) | `23` of `182` (`12.6%`) | `26` of `182` (`14.3%`) |
| wrapped / covered symbols | `335` of `671` (`50%`) | `91` of `669` (`13.6%`) | `74` of `669` (`11.1%`) |
| `code_lines` | `10,616` | `10,988` (whole crate) | `12,122` (whole crate) |
| `unsafe fn` (pub) | `429` (`425`) | `95` (`82`) | `118` (`111`) |
| raw-pointer positions, args + rets | `491` | `121` | `129` |
| calls to C | `374` | `264` | `304` |

How to read it:

- The baselines' unsafe surface is the whole crate, across seven libraries;
  the campaign's is `libavutil` alone.
- The campaign's C coverage denominator is its coverage prebuild of
  `libavutil`'s TUs, not this `n9.0` build's `18,710` lines.
- The campaign's safety-audit figures come from an earlier driver that did not
  yet count raw-pointer fields, so args + rets is the comparable raw-pointer
  figure.
- The campaign's `unsafe fn` count is high by design: every `from_ptr` /
  `from_raw` seam on every wrapped type is an `unsafe fn`.

## 7. Notes and limitations

- **Relation to `ffmpeg-wrap/crustify/baseline.md`.**
  - That file measured Rust coverage only, scoped API coverage to `libavutil`
    against the campaign's `8.0-dev` oracle, and ran next at `3751e28` (`9.0.0`)
    with FFmpeg either from Ubuntu (`7.1.1`) or vendored (`9.0`, default
    configure).
  - Here next is at `9b9c0b9` (`9.0.1`), FFmpeg is one shared `n9.0` build with
    `--disable-autodetect`, and both crates link it statically.
  - ftt's revision is unchanged, and its Rust figures reproduce
    `ffmpeg-wrap/crustify/baseline.md`'s vendored-`9.0` row exactly (`15.66%` LoC, `18.77%` block,
    `16.27%` function).
  - next's moved from `16.26%` to `19.23%` LoC, with `12` more tests (`36` →
    `48`) and its `src/util/` LoC coverage moving from `20.70%` to `28.54%`.
    The ten commits between the two revisions (`v9.0.0..9b9c0b9`) add the
    Stereo3D/Spherical wrappers, extradata and video accessors, and soundness
    fixes; this file does not attribute the change to any one of them.
- **The coverage runs leave out doctests.** That is ftt's `2`.
- **`--disable-autodetect` costs ftt three test results.** It removes the VP9
  and PNG encoders that ftt's three failing tests expect.
- **Mismatched profile data.** `llvm-cov export` over the whole-library object
  set warned that `19` functions in next's profile had mismatched data, so they
  went uncounted. That is negligible against `21,754` functions.
- **API coverage is an upper bound** (see section 3), and its denominator sees
  only compiled code.
- **Tests are not split into UB / equiv / unit.** Neither crate is laid out in
  those categories, and that classification was deferred.
