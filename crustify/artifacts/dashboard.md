# crustify — `ffmpeg / public API`

Copy this template into the campaign checkout, then fill it as work lands.

## Campaign

- **target repo** — `ffmpeg` @ `1019f8f036602a8464185baa4857654337eeca14`
- **target** — `whole public API of libavutil, libswresample, libswscale, libavcodec, libavformat, libavfilter, libavdevice, excluding deprecated items`
- **campaign objective** — `wrap`
- **`impl_files`** — `3181 files: the built TUs and reachable private headers of the 7 libraries (crustify/wavefront/configs/wavefront-config.json)`
- **`api_headers`** — `122 installed headers, excluding the platform/vendor hwcontext_*.h backends, libavcodec/{d3d11va,dxva2,jni,mediacodec,qsv,vdpau,videotoolbox}.h and the generated avconfig.h/ffversion.h`
- **agent backend** — `claude`; `codex` for `libavutil/crypto`
- **model** — `openrouter/anthropic/claude-opus-5.5`; `openrouter/openai/gpt-6.1-sol` for `libavutil/crypto`
- **billing** — `api`
- **`--max-types`** — `2`
- **`--max-syms`** — `25`
- **`--max-loc`** — `500`
- **`--min-fields`** — `20`
- **assistant parallelism** — up to `16` concurrent batch processes from `libavutil/options`; `10` before
- **branch** — `crustify/main`, tip `<sha>`
- **deps** — crustify `12b8cab` (`main`, plus an uncommitted `translator.md` edit), ffibox `196c5fb` (`main`)
- **build version** — `3`
- **C test baseline** — `5437/5437, 0 skipped, and no disabled tests`

## Review pass

Review batches run by `crustify spawn-reviewer`, LLM-as-a-Judge over each landed wave.

- **agent backend** — `claude`
- **model** — `openrouter/anthropic/claude-opus-5.5` from `libavutil/options`; `openrouter/anthropic/claude-fable-5.1` before (see Notes)
- **billing** — `api`
- **`--max-types`** — `2` (wave 0); the wave's types from wave 1
- **`--max-syms`** — `25` (wave 0); the wave's symbols from wave 1
- **`--max-loc`** — `500` (wave 0); unbounded from wave 1
- **`--min-fields`** — `20`
- **assistant parallelism** — up to `16` concurrent batch processes from `libavutil/options`; `10` before
- **branch** — `crustify/reviews/libavutil/hwcontext/wave-2`, tip `2a1f7d6d5a`
- **agents** — `51` (one stopped by the provider), over `23` wave(s)

`rv`-prefixed columns below carry the review pass; the unprefixed ones remain
the campaign's.

## Legend

- `objective` — what the batch's agents were told to do: `wrap`, `port`, or
  `raw lifetime`. The Types tables are split by it, so it appears as a column
  only in the Symbols `Batches` table, which mixes the two
- `subsystem` — `<link-unit>/<subsystem>`, or `raw-lifetime-void` /
  `raw-lifetime-string`. In the Types and Symbols tables it names the batch's subsystem,
  the same name as its Overview row, and with `wave` and `batch` keys the row to
  the subsystem's `schedule.json`
- `wave` / `batch` — the batch's position in its subsystem's plan: the index
  of its wave in `waves` and of the batch within that wave's `batches`, so a
  batch row joins to its subsystem's `schedule.json`, to the agent that landed it, and to its
  Overview subsystem row. Schema-v2 plans carry `steps` and `plan_items`
  rather than indexed waves and batches, so historical rows leave both blank
- `types` / `symbols` — scheduler units in the batch. Callbacks are scheduled
  in symbol batches and counted there
- `fields` — in-scope fields: the field accessors the oracle assigned to that
  type batch, not the type's full declared field count
- `strategies emitted` — in `Raw lifetime discovery`, the release and clone
  strategies that tier's batch emitted for its `void` or string primitives
- `$` / `wall` / `loc` — that agent's computed cost, its elapsed time, and the
  `.rs` insertions of its landing commit. `wall` is `ended_at − started_at` from
  the agent's own `usage.json`, so it INCLUDES the per-worktree C rebuild
- `$/type` / `$/symbol` / `$/field` / `$/loc` — that row's `$` over its units,
  its in-scope fields, or its `loc`
- `tokens` / `rv tokens` / `batch tokens` / `review tokens` / `total tokens` — the
  tokens behind the `$` cell to their left, in millions: input + output + cache read +
  cache write, summed over the agent's `usage.json` requests (the figure `crustify usage`
  reports). Cache reads dominate it
- `subsystem wall` — from its first batch launch through its final review
  and regression gate
- `batch` / `review` — in the Overview, the computed cost of the subsystem's
  translation batches and of its review batches; `—` where it ran none
- `total` — `batch` + `review`; for the assistant, its own cost
- `assistant` — the Overview row for every assistant session of the campaign, priced
  from the `assistant-*.usage.json` records in `crustify/artifacts/assistant/`; `total` is their sum. A
  session that died before writing its record is recovered from its transcript (Notes)
- `Σ recorded agents` / `Σ campaign` — the batch and review agents alone, then those plus the
  assistant. `$/type` and `$/sym` are given for the agents only, since the assistant's
  cost cannot be split by unit kind
- `$/type` / `$/sym` — in the Overview, a subsystem's cost over the types or
  symbols it was scheduled for; `—` where it was scheduled for none
- `+LoC` — the landed batch's Rust-source insertions/deletions relative to its
  wave's base
- `cov (C/Rust)` — in a `Batches` table, the absolute line coverage right after
  that batch landed, from ONE run of all four test workloads together (their
  union): C over the subsystem link unit's TUs in the `coverage` prebuild,
  Rust over the authored non-test sources. Its **Σ** cell is the coverage at the
  table's last landing, not a sum
- `+UB safe tests` / `+UB unsafe tests` / `+equiv tests` / `+unit tests` — in a
  `Batches …: tests` table, the tests added by that landed batch; each parenthesized pair is its C/Rust
  line-coverage change in percentage points. Negative coverage deltas are valid
  when the landed source adds more executable lines than the new tests cover
- `+lifecycle` … `+misc` — in a `Batches — review: PoC + reports` table, the
  defect reports that review batch filed under
  `crustify/reviews/<artifact-dir>/<category>/`, one per defect it reported;
  `+UB safe` / `+UB unsafe` are its `ub_safe` and `ub_unsafe` categories.
  `+lifecycle` covers type and raw-lifetime items only, so it reads `—` for
  symbols. `total` is the row's sum
- `+rejected PoCs` — the `ub_safe`, `ub_unsafe` and `equiv` PoCs that review
  batch filed which the assistant rejected after rerunning them: they did not
  reproduce, or did not show a real bug, and their fixes were reverted. They
  stay counted in their categories; `total` minus this column is the confirmed
  defects
- `rv $` / `rv wall` / `rv loc` — the REVIEW agent's cost, elapsed time, and net
  `.rs` line delta (`+ins/-del`) of its landing commit. Under subscription
  billing `rv $` is an API-equivalent comparison value, not a charged amount
- `UB safe tests` / `UB unsafe tests` / `Equiv tests` / `Unit tests` — counts of
  `#[test]` functions in the `tests/ub_safe`, `tests/ub_unsafe` and `tests/equiv`
  test binaries and in the library's `#[cfg(test)]` modules. Each
  coverage pair comes from running only that workload, against the
  coverage-instrumented target C sources and authored Rust sources; state any
  excluded files or generated code in Notes. The four workloads bound different
  obligations, so report them apart and never sum them. Report the UB
  workload once, from a single coverage run on the plain build: it executes once
  per prepared sanitizer, but those runs share one test set and re-counting them
  inflates the figure. Name the instruments it ran under in Notes
- `Total tests` — the four counts added. Its coverage pair is NOT the four
  pairs added: it comes from one run of all four workloads together and is the
  union of the lines any of them reached, which is smaller than the sum wherever
  two workloads cover the same line. Deriving it arithmetically overstates it.
  It is a reach figure only — it mixes four verdict sources and so bounds no
  single obligation, which is why the four stay reported apart above it
- `total types` / `total symbols` — the units the target's public API exports, deprecated
  items excluded: every type, and every symbol and callback, scheduled across all the
  subsystems' `schedule.json`, raw-lifetime items aside. They are fixed for the campaign
- `wrapped types` / `wrapped symbols` — scheduled units whose `Wraps:` or `Replaces:` anchor
  is present at the campaign tip; the parenthesized share is of `total types` /
  `total symbols`

Every table below is a heading, a model line and the table. All prose belongs
in Notes.

## Overview

- **Rust LoC, non-test** — `<n>`
- **Rust LoC, tests** — `<n>`
- **UB safe tests** — `815` (`58.82`% C LoC coverage, `80.92`% Rust LoC coverage)
- **UB unsafe tests** — `181` (`29.83`% C LoC coverage, `30.02`% Rust LoC coverage)
- **Equiv tests** — `718` (`74.02`% C LoC coverage, `86.39`% Rust LoC coverage)
- **Unit tests** — `967` (`47.65`% C LoC coverage, `84.81`% Rust LoC coverage)
- **Total tests** — `2681` (`75.12`% C LoC coverage, `99.68`% Rust LoC coverage)
- **C LoC** — `<n>`
- **total types** — `290`
- **total symbols** — `1126`
- **ported types** — `<n>`
- **ported symbols** — `<n>`
- **wrapped types** — `82` (`28.28`% of total types)
- **wrapped symbols** — `516` (`45.83`% of total symbols)

Implementation `openrouter/anthropic/claude-opus-5.5` via `claude` (`openrouter/openai/gpt-6.1-sol` via `codex` for `libavutil/crypto`); review
`openrouter/anthropic/claude-opus-5.5` via `claude`.

| subsystem | nr types | nr symbols | subsystem wall | batch | batch tokens | review | review tokens | total | total tokens | $/type | $/sym |
|---|---:|---:|---|---:|---:|---:|---:|---:|---:|---:|---:|
| `raw-lifetime-void` | `0` | `6` | `78m48s` | `$8.46` | `15.34M` | `$4.69` | `11.85M` | `$13.15` | `27.19M` | — | `$2.19` |
| `raw-lifetime-string` | `0` | `3` | `48m03s` | `$3.77` | `8.84M` | `$5.25` | `13.08M` | `$9.02` | `21.92M` | — | `$3.01` |
| `libavutil/core` | `21` | `159` | `47h12m32s` | `$214.04` | `537.07M` | `$211.19` | `247.77M` | `$425.23` | `784.84M` | `$7.10` | `$1.74` |
| `libavutil/formats` | `19` | `100` | `21h42m45s` | `$142.10` | `365.22M` | `$336.68` | `288.82M` | `$478.78` | `654.04M` | `$11.31` | `$2.64` |
| `libavutil/options` | `6` | `78` | `6h15m34s` | `$122.01` | `366.96M` | `$63.52` | `181.86M` | `$185.53` | `548.82M` | `$6.15` | `$1.91` |
| `libavutil/crypto` | `19` | `103` | `4h26m50s` | `$46.14` | `171.31M` | `$38.22` | `108.22M` | `$84.36` | `279.52M` | `$2.48` | `$0.36` |
| `libavutil/buffer-frame` | `12` | `58` | `10h58m03s` | `$130.04` | `372.78M` | `$89.40` | `226.96M` | `$219.44` | `599.74M` | `$8.59` | `$2.01` |
| `libavutil/hwcontext` | `5` | `18` | `5h15m53s` | `$66.73` | `200.49M` | `$51.09` | `132.67M` | `$117.82` | `333.17M` | `$14.64` | `$2.48` |
| assistant | — | — | — | — | — | — | — | `$500.92` | `772.74M` | — | — |
| **Σ recorded agents** | **`82`** | **`525`** | **`97h58m`** | **`$733.32`** | **`2038.01M`** | **`$800.05`** | **`1211.23M`** | **`$1533.36`** | **`3249.24M`** | **`$7.61`** | **`$1.73`** |
| **Σ campaign** | **`82`** | **`525`** | **`97h58m`** | **`$733.32`** | **`2038.01M`** | **`$800.05`** | **`1211.23M`** | **`$2034.28`** | **`4021.98M`** | — | — |

## Raw lifetime discovery

`openrouter/anthropic/claude-opus-5.5` via `claude`.

| tier | strategies emitted | $ | tokens | wall |
|---|---|---|---|---|
| void | `3` | `$8.46` | `15.34M` | `57m23s` |
| string | `2` | `$3.77` | `8.84M` | `19m11s` |
| **Σ** | **`5`** | **`$12.23`** | **`24.18M`** | **`76m34s`** |

### Review

`openrouter/anthropic/claude-opus-5.5` via `claude`.

| tier | rv loc | rv $ | rv tokens | rv wall |
|---|---|---|---|---|
| void | `+241/-4` | `$4.69` | `11.85M` | `17m36s` |
| string | `+86/-17` | `$5.25` | `13.08M` | `26m33s` |
| **Σ** | **`+327/-21`** | **`$9.94`** | **`24.93M`** | **`44m09s`** |

## Types

### Batches — wrap

`openrouter/anthropic/claude-opus-5.5` via `claude`.

| subsystem | wave | batch | types | fields | +LoC | cov (C/Rust) | $ | tokens | wall | $/type | $/field |
|---|---|---|---|---|---|---|---|---|---|---|---|
|`libavutil/core`|`0`|`0`| `2` | `0` |`+649/-0`| `2.18`% / `99.41`% | `$2.13` | `4.18M` | `10m25s` | `$1.06` | `—` |
|`libavutil/core`|`0`|`1`| `2` | `0` |`+569/-0`| `2.92`% / `99.63`% | `$2.77` | `5.54M` | `16m06s` | `$1.39` | `—` |
|`libavutil/core`|`0`|`2`| `2` | `0` |`+1154/-0`| `9.10`% / `99.85`% | `$7.87` | `19.56M` | `32m39s` | `$3.94` | `—` |
|`libavutil/core`|`0`|`3`| `2` | `2` |`+848/-2`| `9.22`% / `99.86`% | `$4.39` | `11.34M` | `28m48s` | `$2.19` | `$2.19` |
|`libavutil/core`|`0`|`4`| `2` | `0` |`+504/-0`| `3.29`% / `99.75`% | `$4.53` | `10.80M` | `23m25s` | `$2.27` | `—` |
|`libavutil/core`|`0`|`5`| `2` | `6` |`+650/-2`| `2.92`% / `99.72`% | `$3.35` | `7.35M` | `20m31s` | `$1.68` | `$0.56` |
|`libavutil/core`|`0`|`6`| `2` | `8` |`+773/-6`| `3.72`% / `99.82`% | `$6.15` | `15.08M` | `29m17s` | `$3.08` | `$0.77` |
|`libavutil/core`|`0`|`7`| `1` | `2` |`+494/-0`| `2.36`% / `99.56`% | `$2.61` | `5.60M` | `14m49s` | `$2.61` | `$1.30` |
|`libavutil/core`|`0`|`8`| `2` | `2` |`+741/-2`| `3.72`% / `99.78`% | `$5.48` | `14.17M` | `25m56s` | `$2.74` | `$2.74` |
|`libavutil/core`|`0`|`9`| `1` | `1` |`+396/-0`| `2.36`% / `99.48`% | `$2.95` | `6.66M` | `12m24s` | `$2.95` | `$2.95` |
|`libavutil/core`|`1`|`0`| `2` | `18` |`+1555/-8`| `19.58`% / `99.86`% | `$13.38` | `40.32M` | `43m37s` | `$6.69` | `$0.74` |
|`libavutil/core`|`1`|`1`| `1` | `5` |`+500/-2`| `19.23`% / `99.85`% | `$3.10` | `6.73M` | `20m22s` | `$3.10` | `$0.62` |
|`libavutil/formats`|`0`|`0`| `2` | `0` |`+986/-0`| `21.36`% / `99.89`% | `$3.17` | `6.78M` | `17m21s` | `$1.58` | `—` |
|`libavutil/formats`|`0`|`1`| `2` | `0` |`+828/-1`| `24.24`% / `99.90`% | `$4.44` | `10.50M` | `32m31s` | `$2.22` | `—` |
|`libavutil/formats`|`0`|`2`| `2` | `0` |`+763/-0`| `22.00`% / `99.90`% | `$3.78` | `9.14M` | `23m25s` | `$1.89` | `—` |
|`libavutil/formats`|`0`|`3`| `2` | `0` |`+787/-0`| `20.84`% / `99.89`% | `$2.58` | `5.10M` | `14m47s` | `$1.29` | `—` |
|`libavutil/formats`|`0`|`4`| `2` | `5` |`+832/-0`| `22.52`% / `99.90`% | `$4.57` | `10.61M` | `25m31s` | `$2.29` | `$0.91` |
|`libavutil/formats`|`0`|`5`| `1` | `0` |`+938/-1`| `21.38`% / `99.89`% | `$3.94` | `9.25M` | `19m56s` | `$3.94` | `—` |
|`libavutil/formats`|`0`|`6`| `1` | `0` |`+536/-0`| `20.67`% / `99.89`% | `$1.92` | `3.77M` | `10m56s` | `$1.92` | `—` |
|`libavutil/formats`|`1`|`0`| `1` | `2` |`+724/-1`| `26.72`% / `99.91`% | `$3.05` | `6.72M` | `20m35s` | `$3.05` | `$1.52` |
|`libavutil/formats`|`1`|`1`| `2` | `6` |`+1579/-1`| `27.44`% / `99.92`% | `$7.73` | `20.66M` | `28m13s` | `$3.87` | `$1.29` |
|`libavutil/formats`|`1`|`2`| `1` | `7` |`+1533/-2`| `29.92`% / `99.91`% | `$9.07` | `17.20M` | `48m40s` | `$9.07` | `$1.30` |
|`libavutil/formats`|`2`|`0`| `2` | `7` |`+2312/-2`| `33.33`% / `99.90`% | `$12.48` | `37.00M` | `42m35s` | `$6.24` | `$1.78` |
|`libavutil/formats`|`3`|`0`| `1` | `2` |`+839/-0`| `33.33`% / `99.81`% | `$3.16` | `6.95M` | `25m09s` | `$3.16` | `$1.58` |
|`libavutil/options`|`0`|`0`| `2` | `2` |`+642/-0`| `34.11`% / `99.82`% | `$6.92` | `18.66M` | `28m39s` | `$3.46` | `$3.46` |
|`libavutil/options`|`0`|`1`| `2` | `4` |`+1512/-12`| `38.71`% / `99.83`% | `$15.45` | `47.79M` | `67m35s` | `$7.72` | `$3.86` |
|`libavutil/options`|`0`|`2`| `2` | `6` |`+949/-65`| `33.65`% / `99.82`% | `$6.20` | `16.61M` | `24m06s` | `$3.10` | `$1.03` |
|`libavutil/crypto`|`0`|`0`| `2` | `0` |`+490/-0`| `56.91`% / `99.74`% | `$3.22` | `15.32M` | `28m29s` | `$1.61` | `—` |
|`libavutil/crypto`|`0`|`1`| `2` | `2` |`+406/-3`| `63.72`% / `99.74`% | `$5.13` | `19.76M` | `50m01s` | `$2.56` | `$2.56` |
|`libavutil/crypto`|`0`|`2`| `2` | `0` |`+371/-0`| `51.92`% / `99.79`% | `$2.04` | `8.50M` | `17m22s` | `$1.02` | `—` |
|`libavutil/crypto`|`0`|`3`| `2` | `2` |`+471/-1`| `61.54`% / `99.74`% | `$4.58` | `14.68M` | `45m12s` | `$2.29` | `$2.29` |
|`libavutil/crypto`|`0`|`4`| `2` | `0` |`+284/-0`| `55.67`% / `99.79`% | `$2.13` | `9.36M` | `21m23s` | `$1.06` | `—` |
|`libavutil/crypto`|`0`|`5`| `2` | `0` |`+487/-0`| `55.78`% / `99.79`% | `$2.55` | `10.55M` | `24m52s` | `$1.27` | `—` |
|`libavutil/crypto`|`0`|`6`| `2` | `3` |`+513/-3`| `63.94`% / `99.72`% | `$4.99` | `12.96M` | `55m45s` | `$2.50` | `$1.66` |
|`libavutil/crypto`|`0`|`7`| `2` | `0` |`+517/-0`| `50.80`% / `99.80`% | `$1.85` | `7.13M` | `15m21s` | `$0.93` | `—` |
|`libavutil/crypto`|`0`|`8`| `2` | `0` |`+462/-4`| `60.45`% / `99.74`% | `$5.13` | `18.48M` | `39m57s` | `$2.56` | `—` |
|`libavutil/crypto`|`0`|`9`| `1` | `1` |`+279/-0`| `49.08`% / `99.84`% | `$1.43` | `5.34M` | `10m11s` | `$1.43` | `$1.43` |
|`libavutil/buffer-frame`|`0`|`0`| `1` | `0` |`+333/-0`| `66.28`% / `99.74`% | `$3.64` | `9.27M` | `26m08s` | `$3.64` | `—` |
|`libavutil/buffer-frame`|`0`|`1`| `2` | `0` |`+890/-0`| `68.44`% / `99.75`% | `$7.52` | `20.83M` | `48m24s` | `$3.76` | `—` |
|`libavutil/buffer-frame`|`0`|`2`| `1` | `0` |`+602/-0`| `65.88`% / `99.74`% | `$3.17` | `7.73M` | `16m22s` | `$3.17` | `—` |
|`libavutil/buffer-frame`|`0`|`3`| `2` | `2` |`+873/-0`| `67.42`% / `99.75`% | `$7.24` | `20.72M` | `32m40s` | `$3.62` | `$3.62` |
|`libavutil/buffer-frame`|`0`|`4`| `2` | `2` |`+1139/-3`| `67.62`% / `99.75`% | `$6.58` | `18.48M` | `37m01s` | `$3.29` | `$3.29` |
|`libavutil/buffer-frame`|`1`|`0`| `2` | `9` |`+1912/-64`| `68.55`% / `99.71`% | `$14.36` | `42.25M` | `51m42s` | `$7.18` | `$1.60` |
|`libavutil/buffer-frame`|`2`|`0`| `1` | `5` |`+1707/-4`| `69.34`% / `99.73`% | `$9.77` | `27.27M` | `38m32s` | `$9.77` | `$1.95` |
|`libavutil/buffer-frame`|`3`|`0`| `1` | `40` |`+3228/-230`| `71.01`% / `99.70`% | `$19.23` | `59.69M` | `64m31s` | `$19.23` | `$0.48` |
|`libavutil/hwcontext`|`0`|`0`| `2` | `0` |`+548/-0`| `72.49`% / `99.70`% | `$2.84` | `6.64M` | `12m09s` | `$1.42` | `—` |
|`libavutil/hwcontext`|`1`|`0`| `2` | `11` |`+2151/-10`| `75.07`% / `99.68`% | `$25.46` | `84.94M` | `85m00s` | `$12.73` | `$2.31` |
|`libavutil/hwcontext`|`2`|`0`| `1` | `12` |`+1844/-170`| `75.08`% / `99.69`% | `$11.06` | `33.26M` | `41m54s` | `$11.06` | `$0.92` |
| **Σ** |  |  | **`82`** | **`174`** | **`+44100/-599`** | **`75.08`% / `99.69`%** | **`$291.09`** | **`827.23M`** | — | **`$3.55`** | **`$1.67`** |

### Batches — wrap: tests

`openrouter/anthropic/claude-opus-5.5` via `claude`.

| subsystem | wave | batch | +UB safe tests (+C/+Rust pp) | +UB unsafe tests (+C/+Rust pp) | +equiv tests (+C/+Rust pp) | +unit tests (+C/+Rust pp) |
|---|---|---|---|---|---|---|
|`libavutil/core`|`0`|`0`|`+3` (`+0.00`/`+12.03` pp)|`+0` (`+0.00`/`+0.00` pp)|`+3` (`+1.69`/`-29.59` pp)|`+9` (`+0.00`/`-0.59` pp)|
|`libavutil/core`|`0`|`1`|`+8` (`+0.00`/`-0.93` pp)|`+0` (`+0.00`/`+0.00` pp)|`+11` (`+0.57`/`-2.30` pp)|`+9` (`+0.00`/`+0.07` pp)|
|`libavutil/core`|`0`|`2`|`+0` (`+0.00`/`-10.96` pp)|`+10` (`+1.10`/`+7.15` pp)|`+5` (`+0.74`/`-3.89` pp)|`+12` (`+5.51`/`-3.02` pp)|
|`libavutil/core`|`0`|`3`|`+14` (`+0.00`/`+2.69` pp)|`+0` (`+0.00`/`-0.70` pp)|`+10` (`+0.37`/`-1.40` pp)|`+15` (`+0.00`/`-0.10` pp)|
|`libavutil/core`|`0`|`4`|`+4` (`+0.00`/`+1.78` pp)|`+0` (`+0.00`/`+0.00` pp)|`+9` (`+0.37`/`-1.69` pp)|`+8` (`+0.00`/`+0.03` pp)|
|`libavutil/core`|`0`|`5`|`+9` (`+0.00`/`-1.08` pp)|`+0` (`+0.00`/`+0.00` pp)|`+6` (`+0.00`/`+0.85` pp)|`+7` (`+0.00`/`+0.10` pp)|
|`libavutil/core`|`0`|`6`|`+9` (`+0.00`/`-4.18` pp)|`+0` (`+0.00`/`+0.00` pp)|`+7` (`+0.00`/`+2.75` pp)|`+12` (`+0.00`/`+0.04` pp)|
|`libavutil/core`|`0`|`7`|`+9` (`+0.00`/`-3.59` pp)|`+0` (`+0.00`/`+0.00` pp)|`+8` (`+0.00`/`+0.19` pp)|`+9` (`+0.00`/`+0.08` pp)|
|`libavutil/core`|`0`|`8`|`+10` (`+0.00`/`-1.32` pp)|`+0` (`+0.00`/`+0.00` pp)|`+8` (`+0.43`/`+1.09` pp)|`+7` (`+0.00`/`+0.03` pp)|
|`libavutil/core`|`0`|`9`|`+10` (`+0.00`/`+9.78` pp)|`+0` (`+0.00`/`+0.00` pp)|`+5` (`+0.18`/`+1.84` pp)|`+4` (`+0.00`/`+0.07` pp)|
|`libavutil/core`|`1`|`0`|`+8` (`+0.08`/`+0.48` pp)|`+4` (`+0.00`/`+1.21` pp)|`+8` (`+0.37`/`+0.75` pp)|`+11` (`+0.34`/`-0.65` pp)|
|`libavutil/core`|`1`|`1`|`+8` (`+0.00`/`-0.02` pp)|`+5` (`+0.00`/`+0.70` pp)|`+3` (`+0.00`/`-0.37` pp)|`+5` (`+0.00`/`+0.35` pp)|
|`libavutil/formats`|`0`|`0`|`+7` (`+0.00`/`-0.42` pp)|`+0` (`+0.00`/`-0.41` pp)|`+13` (`+0.52`/`-1.16` pp)|`+18` (`+0.00`/`+0.43` pp)|
|`libavutil/formats`|`0`|`1`|`+12` (`+0.00`/`+0.37` pp)|`+0` (`+0.00`/`-0.19` pp)|`+11` (`+1.72`/`+0.29` pp)|`+14` (`+0.00`/`+0.19` pp)|
|`libavutil/formats`|`0`|`2`|`+4` (`+0.00`/`+0.18` pp)|`+0` (`+0.00`/`-0.23` pp)|`+10` (`+0.66`/`+0.46` pp)|`+13` (`+0.00`/`+0.24` pp)|
|`libavutil/formats`|`0`|`3`|`+4` (`+0.00`/`-0.38` pp)|`+0` (`+0.00`/`-0.36` pp)|`+7` (`+0.17`/`+0.62` pp)|`+12` (`+0.00`/`+0.37` pp)|
|`libavutil/formats`|`0`|`4`|`+14` (`+0.00`/`+0.55` pp)|`+0` (`+0.00`/`-0.38` pp)|`+4` (`+0.52`/`-0.72` pp)|`+10` (`+0.00`/`+0.39` pp)|
|`libavutil/formats`|`0`|`5`|`+5` (`+0.00`/`+0.07` pp)|`+0` (`+0.00`/`-0.09` pp)|`+7` (`+0.02`/`-0.03` pp)|`+7` (`+0.00`/`+0.10` pp)|
|`libavutil/formats`|`0`|`6`|`+8` (`+0.00`/`+0.30` pp)|`+0` (`+0.00`/`-0.17` pp)|`+9` (`+0.10`/`-0.66` pp)|`+7` (`+0.00`/`+0.18` pp)|
|`libavutil/formats`|`1`|`0`|`+12` (`+0.00`/`+0.10` pp)|`+0` (`+0.00`/`-0.17` pp)|`+4` (`+0.20`/`-0.14` pp)|`+9` (`+0.00`/`+0.18` pp)|
|`libavutil/formats`|`1`|`1`|`+24` (`+0.00`/`+0.36` pp)|`+0` (`+0.00`/`-0.47` pp)|`+7` (`+0.72`/`-0.28` pp)|`+23` (`+0.00`/`+0.51` pp)|
|`libavutil/formats`|`1`|`2`|`+15` (`+0.00`/`+0.06` pp)|`+5` (`+0.01`/`+2.62` pp)|`+3` (`+0.45`/`-0.45` pp)|`+11` (`+0.00`/`+0.27` pp)|
|`libavutil/formats`|`2`|`0`|`+21` (`+2.18`/`+1.06` pp)|`+0` (`+0.00`/`-0.44` pp)|`+9` (`+0.84`/`-0.09` pp)|`+23` (`+2.24`/`+0.65` pp)|
|`libavutil/formats`|`3`|`0`|`+11` (`+0.00`/`+0.07` pp)|`+0` (`+0.00`/`-0.09` pp)|`+4` (`+0.00`/`-0.08` pp)|`+9` (`+0.00`/`+0.14` pp)|
|`libavutil/options`|`0`|`0`|`+8` (`+0.40`/`-0.12` pp)|`+0` (`+0.00`/`-0.12` pp)|`+4` (`+0.72`/`-0.16` pp)|`+8` (`+0.30`/`+0.15` pp)|
|`libavutil/options`|`0`|`1`|`+10` (`+1.24`/`+0.01` pp)|`+5` (`+0.85`/`+0.36` pp)|`+8` (`+1.99`/`-0.20` pp)|`+12` (`+1.13`/`+0.24` pp)|
|`libavutil/options`|`0`|`2`|`+3` (`+0.00`/`-1.06` pp)|`+5` (`+0.00`/`+1.25` pp)|`+4` (`+0.07`/`+0.08` pp)|`+9` (`+0.00`/`+0.24` pp)|
|`libavutil/crypto`|`0`|`0`|`+4` (`+0.88`/`-0.07` pp)|`+0` (`+0.02`/`-0.20` pp)|`+2` (`+1.13`/`-0.10` pp)|`+8` (`+0.59`/`-0.04` pp)|
|`libavutil/crypto`|`0`|`1`|`+4` (`+1.19`/`+0.07` pp)|`+0` (`-0.02`/`-0.19` pp)|`+2` (`+2.18`/`+0.07` pp)|`+6` (`+1.17`/`+0.07` pp)|
|`libavutil/crypto`|`0`|`2`|`+5` (`+0.31`/`-0.03` pp)|`+0` (`+0.02`/`-0.18` pp)|`+3` (`+1.11`/`-0.22` pp)|`+5` (`+0.31`/`+0.08` pp)|
|`libavutil/crypto`|`0`|`3`|`+7` (`+2.01`/`+0.11` pp)|`+0` (`+0.02`/`-0.18` pp)|`+2` (`+1.08`/`+0.09` pp)|`+5` (`+0.86`/`+0.06` pp)|
|`libavutil/crypto`|`0`|`4`|`+3` (`+0.95`/`-0.02` pp)|`+0` (`+0.00`/`-0.12` pp)|`+3` (`+3.75`/`-0.01` pp)|`+3` (`+0.28`/`+0.02` pp)|
|`libavutil/crypto`|`0`|`5`|`+4` (`+0.98`/`+0.11` pp)|`+0` (`-0.02`/`-0.18` pp)|`+4` (`+0.11`/`+0.09` pp)|`+8` (`+0.84`/`+0.04` pp)|
|`libavutil/crypto`|`0`|`6`|`+6` (`+0.14`/`-0.00` pp)|`+0` (`+0.00`/`-0.24` pp)|`+4` (`+0.22`/`+0.00` pp)|`+6` (`+0.35`/`+0.10` pp)|
|`libavutil/crypto`|`0`|`7`|`+4` (`+0.67`/`+0.00` pp)|`+0` (`+0.00`/`-0.20` pp)|`+4` (`+1.73`/`-0.17` pp)|`+6` (`+0.28`/`-0.23` pp)|
|`libavutil/crypto`|`0`|`8`|`+2` (`+0.80`/`-0.00` pp)|`+0` (`+0.00`/`-0.13` pp)|`+2` (`+1.72`/`-0.00` pp)|`+5` (`+0.68`/`+0.06` pp)|
|`libavutil/crypto`|`0`|`9`|`+4` (`+0.10`/`+0.01` pp)|`+0` (`-0.02`/`-0.10` pp)|`+2` (`+1.08`/`+0.01` pp)|`+3` (`+0.02`/`-0.15` pp)|
|`libavutil/buffer-frame`|`0`|`0`|`+3` (`+0.00`/`+0.07` pp)|`+0` (`+0.00`/`-0.10` pp)|`+3` (`+0.41`/`-0.21` pp)|`+5` (`+0.00`/`+0.07` pp)|
|`libavutil/buffer-frame`|`0`|`1`|`+5` (`+0.22`/`-0.24` pp)|`+7` (`+1.00`/`+0.20` pp)|`+4` (`+0.78`/`-0.00` pp)|`+8` (`+1.09`/`+0.06` pp)|
|`libavutil/buffer-frame`|`0`|`2`|`+4` (`+0.00`/`-0.25` pp)|`+0` (`+0.00`/`-0.15` pp)|`+4` (`+0.79`/`+0.08` pp)|`+7` (`+0.00`/`+0.10` pp)|
|`libavutil/buffer-frame`|`0`|`3`|`+7` (`+0.64`/`+0.05` pp)|`+0` (`+0.00`/`-0.17` pp)|`+10` (`+1.14`/`-0.05` pp)|`+9` (`+0.66`/`+0.11` pp)|
|`libavutil/buffer-frame`|`0`|`4`|`+13` (`+0.00`/`-0.14` pp)|`+5` (`+0.03`/`+0.30` pp)|`+5` (`+0.20`/`-0.41` pp)|`+10` (`+0.00`/`+0.25` pp)|
|`libavutil/buffer-frame`|`1`|`0`|`+14` (`+0.45`/`+0.04` pp)|`+4` (`+0.14`/`+0.17` pp)|`+6` (`+0.04`/`-0.28` pp)|`+12` (`+0.09`/`+0.08` pp)|
|`libavutil/buffer-frame`|`2`|`0`|`+15` (`+0.50`/`+0.42` pp)|`+6` (`+0.65`/`+0.87` pp)|`+3` (`+0.40`/`-0.48` pp)|`+8` (`+0.34`/`-0.04` pp)|
|`libavutil/buffer-frame`|`3`|`0`|`+13` (`+1.76`/`-0.01` pp)|`+7` (`+2.64`/`+1.12` pp)|`+3` (`+1.08`/`-0.31` pp)|`+14` (`+1.59`/`-1.16` pp)|
|`libavutil/hwcontext`|`0`|`0`|`+6` (`+0.00`/`-0.03` pp)|`+0` (`-0.02`/`-0.10` pp)|`+6` (`+0.10`/`+0.05` pp)|`+9` (`+0.00`/`+0.06` pp)|
|`libavutil/hwcontext`|`1`|`0`|`+4` (`+0.05`/`-0.86` pp)|`+14` (`+0.23`/`+0.90` pp)|`+7` (`+0.15`/`+0.14` pp)|`+18` (`+0.15`/`+0.31` pp)|
|`libavutil/hwcontext`|`2`|`0`|`+0` (`+0.00`/`-1.43` pp)|`+10` (`+0.77`/`+1.36` pp)|`+5` (`+0.02`/`-0.14` pp)|`+13` (`+0.01`/`+0.26` pp)|
| **Σ** |  |  | **`+377` (`+15.55`/`+3.62` pp)** | **`+87` (`+7.40`/`+12.15` pp)** | **`+271` (`+32.63`/`-36.14` pp)** | **`+461` (`+18.83`/`+0.80` pp)** |

### Batches — port

`<provider>/<model>` via `<backend>`.

| subsystem | wave | batch | types | fields | +LoC | cov (C/Rust) | $ | tokens | wall | $/type | $/field |
|---|---|---|---|---|---|---|---|---|---|---|---|
|`<link-unit>/<subsystem>`|`<n>`|`<n>`| `<n>` | `<n>` |`+<n>/-<n>`| `<n>`% / `<n>`% | `$<n>` | `<n>M` | `<n>m<n>s` | `$<n>` | `$<n>` |
| **Σ** |  |  | **`<n>`** | **`<n>`** | **`+<n>/-<n>`** | **`<n>`% / `<n>`%** | **`$<n>`** | **`<n>M`** | — | **`$<n>`** | **`$<n>`** |

### Batches — port: tests

`<provider>/<model>` via `<backend>`.

| subsystem | wave | batch | +UB safe tests (+C/+Rust pp) | +UB unsafe tests (+C/+Rust pp) | +equiv tests (+C/+Rust pp) | +unit tests (+C/+Rust pp) |
|---|---|---|---|---|---|---|
|`<link-unit>/<subsystem>`|`<n>`|`<n>`|`+<n>` (`+<n>`/`+<n>` pp)|`+<n>` (`+<n>`/`+<n>` pp)|`+<n>` (`+<n>`/`+<n>` pp)|`+<n>` (`+<n>`/`+<n>` pp)|
| **Σ** |  |  | **`+<n>` (`+<n>`/`+<n>` pp)** | **`+<n>` (`+<n>`/`+<n>` pp)** | **`+<n>` (`+<n>`/`+<n>` pp)** | **`+<n>` (`+<n>`/`+<n>` pp)** |

### Batches — review

`openrouter/anthropic/claude-fable-5.1` via `claude` (wave 0); `openrouter/openai/gpt-6-astra` via `codex` (wave 1).

| subsystem | wave | batch | types | +LoC | cov (C/Rust) | rv loc | rv $ | rv tokens | rv wall | rv $/type |
|---|---|---|---|---|---|---|---|---|---|---|
|`libavutil/core`|`0`|`0`| `2` |`+265/-0`| `19.00`% / `99.84`% | `+350/-0` | `$8.17` | `12.80M` | `30m15s` | `$4.08` |
|`libavutil/core`|`0`|`1`| `2` |`+0/-0`| — | `+0/-0` | `$4.58` | `5.34M` | `17m23s` | `$2.29` |
|`libavutil/core`|`0`|`2`| `2` |`+147/-3`| `18.96`% / `99.84`% | `+239/-15` | `$10.68` | `15.89M` | `28m27s` | `$5.34` |
|`libavutil/core`|`0`|`3`| `2` |`+138/-2`| `19.00`% / `99.84`% | `+168/-2` | `$10.67` | `16.15M` | `33m26s` | `$5.33` |
|`libavutil/core`|`0`|`4`| `2` |`+3/-0`| `18.96`% / `99.83`% | `+14/-0` | `$4.92` | `5.12M` | `18m50s` | `$2.46` |
|`libavutil/core`|`0`|`5`| `2` |`+399/-7`| `19.00`% / `99.84`% | `+524/-8` | `$10.30` | `16.43M` | `37m29s` | `$5.15` |
|`libavutil/core`|`0`|`6`| `2` |`+228/-0`| `18.96`% / `99.84`% | `+303/-6` | `$8.43` | `11.81M` | `25m31s` | `$4.21` |
|`libavutil/core`|`0`|`7`| `1` |`+97/-1`| `18.96`% / `99.83`% | `+127/-1` | `$5.99` | `7.45M` | `20m21s` | `$5.99` |
|`libavutil/core`|`0`|`8`| `2` |`+41/-6`| `19.00`% / `99.84`% | `+62/-10` | `$8.73` | `11.67M` | `34m59s` | `$4.37` |
|`libavutil/core`|`0`|`9`| `1` |`+24/-7`| `18.96`% / `99.83`% | `+31/-10` | `$4.68` | `5.09M` | `16m27s` | `$4.68` |
|`libavutil/core`|`1`|`0`| `3` |`+98/-0`| `20.40`% / `99.88`% | `+132/-8` | `$13.17` | `8.09M` | `13m21s` | `$4.39` |
|`libavutil/formats`|`0`|`0`| `12` |`+134/-0`| `26.52`% / `99.91`% | `+186/-0` | `$21.19` | `20.76M` | `44m08s` | `$1.77` |
|`libavutil/formats`|`1`|`0`| `4` |`+63/-13`| `32.36`% / `99.88`% | `+98/-37` | `$54.06` | `45.63M` | `153m50s` | `$13.52` |
|`libavutil/formats`|`2`|`0`| `2` |`+772/-20`| `33.33`% / `99.81`% | `+1038/-21` | `$43.74` | `49.97M` | `98m46s` | `$21.87` |
|`libavutil/formats`|`3`|`0`| `1` |`+76/-1`| `33.58`% / `99.82`% | `+93/-1` | `$36.04` | `18.84M` | `135m32s` | `$36.04` |
|`libavutil/options`|`0`|`0`| `6` |`+209/-116`| `44.83`% / `99.83`% | `+291/-195` | `$8.34` | `21.52M` | `32m55s` | `$1.39` |
|`libavutil/crypto`|`0`|`0`| `19` |`+229/-33`| `63.96`% / `99.72`% | `+378/-64` | `$14.00` | `42.13M` | `42m30s` | `$0.74` |
|`libavutil/buffer-frame`|`0`|`0`| `8` |`+81/-2`| `68.51`% / `99.74`% | `+116/-6` | `$7.57` | `21.87M` | `30m49s` | `$0.95` |
|`libavutil/buffer-frame`|`1`|`0`| `2` |`+46/-0`| `68.55`% / `99.72`% | `+84/-10` | `$7.14` | `20.13M` | `26m23s` | `$3.57` |
|`libavutil/buffer-frame`|`2`|`0`| `1` |`+2/-2`| `69.34`% / `99.73`% | `+10/-8` | `$6.38` | `18.11M` | `26m57s` | `$6.38` |
|`libavutil/buffer-frame`|`3`|`0`| `1` |`+258/-3`| `71.09`% / `99.70`% | `+377/-18` | `$10.42` | `30.90M` | `32m45s` | `$10.42` |
|`libavutil/hwcontext`|`0`|`0`| `2` |`+0/-0`| `72.49`% / `99.70`% | `+15/-8` | `$2.28` | `4.71M` | `9m50s` | `$1.14` |
|`libavutil/hwcontext`|`1`|`0`| `2` |`+19/-0`| `75.07`% / `99.68`% | `+40/-8` | `$6.88` | `20.45M` | `22m16s` | `$3.44` |
|`libavutil/hwcontext`|`2`|`0`| `1` |`+291/-28`| `75.12`% / `99.69`% | `+460/-79` | `$24.69` | `55.21M` | `116m00s` | `$24.69` |
| **Σ** |  |  | **`82`** | **`+3620/-244`** | **`75.12`% / `99.69`%** | **`+5136/-515`** | **`$333.05`** | **`486.07M`** | — | **`$4.06`** |

### Batches — review: tests

`openrouter/anthropic/claude-fable-5.1` via `claude` (wave 0); `openrouter/openai/gpt-6-astra` via `codex` (wave 1).

| subsystem | wave | batch | +UB safe tests (+C/+Rust pp) | +UB unsafe tests (+C/+Rust pp) | +equiv tests (+C/+Rust pp) | +unit tests (+C/+Rust pp) |
|---|---|---|---|---|---|---|
|`libavutil/core`|`0`|`0`|`+1` (`+0.00`/`+0.07` pp)|`+0` (`+0.00`/`-0.04` pp)|`+4` (`+0.04`/`+0.06` pp)|`+1` (`+0.00`/`+0.04` pp)|
|`libavutil/core`|`0`|`1`|`+0` (`+0.00`/`+0.00` pp)|`+0` (`+0.00`/`+0.00` pp)|`+0` (`+0.00`/`+0.00` pp)|`+0` (`+0.00`/`+0.00` pp)|
|`libavutil/core`|`0`|`2`|`+0` (`+0.00`/`+0.00` pp)|`+2` (`+0.00`/`+0.00` pp)|`+2` (`+0.00`/`+0.00` pp)|`+1` (`+0.00`/`+0.00` pp)|
|`libavutil/core`|`0`|`3`|`+2` (`+0.00`/`+0.00` pp)|`+0` (`+0.00`/`+0.00` pp)|`+1` (`+0.00`/`+0.00` pp)|`+1` (`+0.00`/`+0.00` pp)|
|`libavutil/core`|`0`|`4`|`+0` (`+0.00`/`+0.00` pp)|`+0` (`+0.00`/`+0.00` pp)|`+0` (`+0.00`/`+0.00` pp)|`+0` (`+0.00`/`+0.00` pp)|
|`libavutil/core`|`0`|`5`|`+5` (`+0.00`/`+0.07` pp)|`+0` (`+0.00`/`-0.21` pp)|`+2` (`+0.00`/`-0.07` pp)|`+3` (`+0.00`/`+0.25` pp)|
|`libavutil/core`|`0`|`6`|`+0` (`+0.00`/`-0.39` pp)|`+0` (`+0.00`/`-0.06` pp)|`+3` (`+0.00`/`-0.40` pp)|`+2` (`+0.00`/`+0.07` pp)|
|`libavutil/core`|`0`|`7`|`+2` (`+0.00`/`+0.00` pp)|`+0` (`+0.00`/`+0.00` pp)|`+1` (`+0.00`/`+0.00` pp)|`+0` (`+0.00`/`+0.00` pp)|
|`libavutil/core`|`0`|`8`|`+0` (`+0.00`/`-0.13` pp)|`+0` (`+0.00`/`-0.02` pp)|`+0` (`+0.00`/`-0.13` pp)|`+1` (`+0.00`/`+0.02` pp)|
|`libavutil/core`|`0`|`9`|`+0` (`+0.00`/`+0.13` pp)|`+0` (`+0.00`/`+0.02` pp)|`+0` (`+0.00`/`+0.13` pp)|`+1` (`+0.00`/`-0.02` pp)|
|`libavutil/core`|`1`|`0`|`+2` (`+0.00`/`+0.00` pp)|`+0` (`+0.00`/`+0.00` pp)|`+1` (`+0.00`/`+0.00` pp)|`+1` (`+0.00`/`+0.00` pp)|
|`libavutil/formats`|`0`|`0`|`+0` (`+0.00`/`-0.29` pp)|`+0` (`+0.00`/`-0.03` pp)|`+3` (`+0.00`/`+0.08` pp)|`+4` (`+0.00`/`+0.04` pp)|
|`libavutil/formats`|`1`|`0`|`+0` (`+0.00`/`-0.10` pp)|`+0` (`+0.00`/`-0.01` pp)|`+0` (`+0.00`/`-0.10` pp)|`+1` (`+0.00`/`+0.02` pp)|
|`libavutil/formats`|`2`|`0`|`+3` (`+0.00`/`-0.05` pp)|`+0` (`+0.00`/`-0.03` pp)|`+4` (`+0.00`/`-0.04` pp)|`+3` (`+0.01`/`-0.04` pp)|
|`libavutil/formats`|`3`|`0`|`+0` (`+0.00`/`+0.02` pp)|`+0` (`+0.00`/`+0.00` pp)|`+1` (`+0.00`/`+0.00` pp)|`+0` (`+0.00`/`+0.00` pp)|
|`libavutil/options`|`0`|`0`|`+4` (`+0.00`/`+0.03` pp)|`-5` (`-0.06`/`-0.50` pp)|`+1` (`-0.01`/`+0.00` pp)|`+0` (`+0.00`/`+0.00` pp)|
|`libavutil/crypto`|`0`|`0`|`+1` (`+0.00`/`+0.03` pp)|`+0` (`+0.02`/`-0.05` pp)|`+2` (`+0.02`/`+0.02` pp)|`+2` (`+0.00`/`+0.04` pp)|
|`libavutil/buffer-frame`|`0`|`0`|`+0` (`+0.00`/`+0.00` pp)|`+1` (`+0.07`/`+0.03` pp)|`+0` (`+0.00`/`+0.00` pp)|`+1` (`+0.01`/`+0.00` pp)|
|`libavutil/buffer-frame`|`1`|`0`|`+0` (`+0.00`/`-0.01` pp)|`+0` (`-0.02`/`+0.00` pp)|`+0` (`+0.00`/`+0.00` pp)|`+1` (`+0.00`/`+0.09` pp)|
|`libavutil/buffer-frame`|`2`|`0`|`+0` (`+0.00`/`+0.00` pp)|`+0` (`+0.00`/`+0.00` pp)|`+0` (`+0.01`/`+0.00` pp)|`+0` (`+0.00`/`+0.00` pp)|
|`libavutil/buffer-frame`|`3`|`0`|`+0` (`+0.00`/`+0.00` pp)|`+2` (`+0.04`/`+0.22` pp)|`+1` (`+0.38`/`+0.12` pp)|`+3` (`+0.42`/`+0.35` pp)|
|`libavutil/hwcontext`|`0`|`0`|`+0` (`+0.00`/`-0.01` pp)|`+0` (`+0.02`/`+0.00` pp)|`+0` (`+0.00`/`+0.00` pp)|`+0` (`+0.00`/`+0.00` pp)|
|`libavutil/hwcontext`|`1`|`0`|`+0` (`+0.00`/`-0.01` pp)|`+0` (`+0.00`/`+0.00` pp)|`+0` (`+0.00`/`+0.00` pp)|`+1` (`+0.00`/`+0.00` pp)|
|`libavutil/hwcontext`|`2`|`0`|`+1` (`+0.00`/`+0.02` pp)|`-1` (`+0.02`/`+0.06` pp)|`+1` (`+0.00`/`+0.11` pp)|`+2` (`+0.05`/`+0.04` pp)|
| **Σ** |  |  | **`+21` (`+0.00`/`-0.62` pp)** | **`-1` (`+0.09`/`-0.62` pp)** | **`+27` (`+0.44`/`-0.22` pp)** | **`+29` (`+0.49`/`+0.90` pp)** |

### Batches — review: PoC + reports

`openrouter/anthropic/claude-fable-5.1` via `claude` (wave 0); `openrouter/openai/gpt-6-astra` via `codex` (wave 1).

| subsystem | wave | batch | +lifecycle | +unsafe | +UB safe | +UB unsafe | +equiv | +internal | +conventions | +misc | total | +rejected PoCs |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `libavutil/core` | `0` | `0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+1` | `1` | `0` |
| `libavutil/core` | `0` | `1` | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `0` | `0` |
| `libavutil/core` | `0` | `2` | `+0` | `+1` | `+0` | `+0` | `+0` | `+0` | `+0` | `+3` | `4` | `0` |
| `libavutil/core` | `0` | `3` | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+1` | `1` | `0` |
| `libavutil/core` | `0` | `4` | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+1` | `1` | `0` |
| `libavutil/core` | `0` | `5` | `+2` | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+1` | `3` | `0` |
| `libavutil/core` | `0` | `6` | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+1` | `1` | `0` |
| `libavutil/core` | `0` | `7` | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `0` | `0` |
| `libavutil/core` | `0` | `8` | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+1` | `1` | `0` |
| `libavutil/core` | `0` | `9` | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+1` | `1` | `0` |
| `libavutil/core` | `1` | `0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+2` | `2` | `0` |
| `libavutil/formats` | `0` | `0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+1` | `1` | `0` |
| `libavutil/formats` | `1` | `0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+1` | `+0` | `+2` | `3` | `0` |
| `libavutil/formats` | `2` | `0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+2` | `2` | `0` |
| `libavutil/formats` | `3` | `0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `0` | `0` |
| `libavutil/options` | `0` | `0` | `+0` | `+1` | `+0` | `+0` | `+0` | `+0` | `+1` | `+0` | `2` | `0` |
| `libavutil/crypto` | `0` | `0` | `+1` | `+2` | `+0` | `+0` | `+0` | `+0` | `+2` | `+3` | `8` | `0` |
| `libavutil/buffer-frame` | `0` | `0` | `+0` | `+1` | `+0` | `+0` | `+0` | `+0` | `+1` | `+0` | `2` | `0` |
| `libavutil/buffer-frame` | `1` | `0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+1` | `1` | `0` |
| `libavutil/buffer-frame` | `2` | `0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+1` | `+1` | `2` | `0` |
| `libavutil/buffer-frame` | `3` | `0` | `+0` | `+1` | `+0` | `+2` | `+0` | `+0` | `+0` | `+0` | `3` | `0` |
| `libavutil/hwcontext` | `0` | `0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+1` | `1` | `0` |
| `libavutil/hwcontext` | `1` | `0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+1` | `1` | `0` |
| `libavutil/hwcontext` | `2` | `0` | `+0` | `+1` | `+0` | `+0` | `+0` | `+0` | `+0` | `+1` | `2` | `0` |
| **Σ** |  |  | **`+3`** | **`+7`** | **`+0`** | **`+2`** | **`+0`** | **`+1`** | **`+5`** | **`+25`** | **`43`** | **`0`** |

## Symbols

### Batches

`openrouter/anthropic/claude-opus-5.5` via `claude`.

| subsystem | wave | batch | objective | symbols | loc | +LoC | cov (C/Rust) | $ | tokens | wall | $/symbol | $/loc |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
|`libavutil/core`|`0`|`10`| wrap | `25` | `1283` |`+1283/-3`| `11.21`% / `99.90`% | `$17.79` | `38.33M` | `86m15s` | `$0.71` | `$0.014` |
|`libavutil/core`|`0`|`11`| wrap | `25` | `1877` |`+1877/-14`| `12.54`% / `99.92`% | `$28.38` | `60.96M` | `124m29s` | `$1.14` | `$0.015` |
|`libavutil/core`|`0`|`12`| wrap | `25` | `2896` |`+2896/-176`| `18.96`% / `99.83`% | `$45.49` | `96.70M` | `178m41s` | `$1.82` | `$0.016` |
|`libavutil/core`|`0`|`13`| wrap | `25` | `2611` |`+2611/-3`| `18.00`% / `99.81`% | `$27.60` | `99.28M` | `155m09s` | `$1.10` | `$0.011` |
|`libavutil/core`|`0`|`14`| wrap | `21` | `1561` |`+1561/-1`| `10.37`% / `99.89`% | `$11.38` | `28.95M` | `62m02s` | `$0.54` | `$0.007` |
|`libavutil/core`|`1`|`2`| wrap | `25` | `2218` |`+2218/-1`| `20.40`% / `99.88`% | `$15.41` | `41.58M` | `63m58s` | `$0.62` | `$0.007` |
|`libavutil/core`|`1`|`3`| wrap | `1` | `264` |`+264/-0`| `19.23`% / `99.84`% | `$1.71` | `3.08M` | `11m10s` | `$1.71` | `$0.006` |
|`libavutil/core`|`2`|`0`| wrap | `12` | `925` |`+925/-18`| `20.57`% / `99.88`% | `$7.58` | `20.89M` | `31m30s` | `$0.63` | `$0.008` |
|`libavutil/formats`|`0`|`7`| wrap | `11` | `1926` |`+1926/-2`| `26.52`% / `99.91`% | `$10.68` | `27.80M` | `37m38s` | `$0.97` | `$0.006` |
|`libavutil/formats`|`1`|`3`| wrap | `25` | `1994` |`+1994/-25`| `29.46`% / `99.90`% | `$10.88` | `33.44M` | `37m00s` | `$0.44` | `$0.005` |
|`libavutil/formats`|`1`|`4`| wrap | `22` | `2770` |`+2770/-3`| `32.36`% / `99.88`% | `$20.24` | `50.83M` | `65m20s` | `$0.92` | `$0.007` |
|`libavutil/formats`|`1`|`5`| wrap | `9` | `1993` |`+1993/-11`| `28.16`% / `99.90`% | `$9.38` | `24.81M` | `33m44s` | `$1.04` | `$0.005` |
|`libavutil/formats`|`2`|`1`| wrap | `13` | `2102` |`+2102/-21`| `32.48`% / `99.89`% | `$10.70` | `28.71M` | `34m54s` | `$0.82` | `$0.005` |
|`libavutil/formats`|`3`|`1`| wrap | `14` | `802` |`+802/-5`| `33.58`% / `99.82`% | `$8.96` | `26.52M` | `29m47s` | `$0.64` | `$0.011` |
|`libavutil/formats`|`3`|`2`| wrap | `4` | `678` |`+678/-0`| `33.58`% / `99.81`% | `$6.61` | `18.90M` | `29m44s` | `$1.65` | `$0.010` |
|`libavutil/formats`|`4`|`0`| wrap | `2` | `492` |`+492/-14`| `33.58`% / `99.82`% | `$4.76` | `10.55M` | `28m11s` | `$2.38` | `$0.010` |
|`libavutil/options`|`0`|`3`| wrap | `24` | `3451` |`+3451/-4`| `44.83`% / `99.83`% | `$31.79` | `88.53M` | `94m11s` | `$1.32` | `$0.009` |
|`libavutil/options`|`0`|`4`| wrap | `5` | `1890` |`+1890/-5`| `36.70`% / `99.83`% | `$10.23` | `29.51M` | `38m09s` | `$2.05` | `$0.005` |
|`libavutil/options`|`1`|`0`| wrap | `22` | `2525` |`+2525/-3`| `46.37`% / `99.83`% | `$16.52` | `51.31M` | `39m25s` | `$0.75` | `$0.007` |
|`libavutil/options`|`1`|`1`| wrap | `13` | `3012` |`+3012/-16`| `47.89`% / `99.84`% | `$21.33` | `73.19M` | `49m00s` | `$1.64` | `$0.007` |
|`libavutil/options`|`2`|`0`| wrap | `14` | `1543` |`+1543/-185`| `47.98`% / `99.84`% | `$13.57` | `41.35M` | `40m30s` | `$0.97` | `$0.009` |
|`libavutil/crypto`|`0`|`10`| wrap | `24` | `1215` |`+1215/-0`| `58.73`% / `99.74`% | `$4.39` | `13.29M` | `33m01s` | `$0.18` | `$0.004` |
|`libavutil/crypto`|`1`|`0`| wrap | `25` | `1329` |`+1329/-65`| `64.83`% / `99.73`% | `$2.22` | `8.55M` | `16m31s` | `$0.09` | `$0.002` |
|`libavutil/crypto`|`1`|`1`| wrap | `25` | `633` |`+633/-22`| `65.09`% / `99.74`% | `$2.75` | `12.75M` | `26m25s` | `$0.11` | `$0.004` |
|`libavutil/crypto`|`1`|`2`| wrap | `25` | `788` |`+788/-80`| `64.83`% / `99.74`% | `$2.67` | `11.63M` | `20m42s` | `$0.11` | `$0.003` |
|`libavutil/crypto`|`1`|`3`| wrap | `4` | `427` |`+427/-8`| `63.96`% / `99.73`% | `$1.05` | `3.02M` | `7m33s` | `$0.26` | `$0.002` |
|`libavutil/buffer-frame`|`0`|`5`| wrap | `7` | `716` |`+716/-2`| `68.51`% / `99.75`% | `$10.78` | `29.49M` | `57m40s` | `$1.54` | `$0.015` |
|`libavutil/buffer-frame`|`1`|`1`| wrap | `10` | `1437` |`+1437/-4`| `68.51`% / `99.71`% | `$9.19` | `23.99M` | `33m46s` | `$0.92` | `$0.006` |
|`libavutil/buffer-frame`|`2`|`1`| wrap | `15` | `1322` |`+1322/-58`| `68.94`% / `99.72`% | `$9.87` | `25.86M` | `33m05s` | `$0.66` | `$0.007` |
|`libavutil/buffer-frame`|`3`|`1`| wrap | `8` | `949` |`+949/-31`| `69.46`% / `99.73`% | `$6.13` | `15.26M` | `28m18s` | `$0.77` | `$0.006` |
|`libavutil/buffer-frame`|`4`|`0`| wrap | `18` | `2634` |`+2634/-29`| `72.39`% / `99.69`% | `$22.57` | `71.96M` | `63m09s` | `$1.25` | `$0.009` |
|`libavutil/hwcontext`|`1`|`1`| wrap | `14` | `2390` |`+2390/-5`| `74.92`% / `99.68`% | `$16.16` | `47.13M` | `60m11s` | `$1.15` | `$0.007` |
|`libavutil/hwcontext`|`1`|`2`| wrap | `2` | `1858` |`+1858/-14`| `72.96`% / `99.71`% | `$9.31` | `24.09M` | `39m54s` | `$4.66` | `$0.005` |
|`libavutil/hwcontext`|`2`|`1`| wrap | `2` | `0` |`+0/-0`| `75.07`% / `99.69`% | `$1.88` | `4.44M` | `12m41s` | `$0.94` | `—` |
| **Σ** |  |  |  | **`516`** | **`54511`** | **`+54511/-828`** | **`75.07`% / `99.69`%** | **`$429.96`** | **`1186.68M`** | — | **`$0.83`** | **`$0.008`** |

### Batches: tests

`openrouter/anthropic/claude-opus-5.5` via `claude`.

| subsystem | wave | batch | +UB safe tests (+C/+Rust pp) | +UB unsafe tests (+C/+Rust pp) | +equiv tests (+C/+Rust pp) | +unit tests (+C/+Rust pp) |
|---|---|---|---|---|---|---|
|`libavutil/core`|`0`|`10`|`+16` (`+0.82`/`+2.12` pp)|`+6` (`+0.02`/`-0.15` pp)|`+21` (`+0.85`/`+2.50` pp)|`+18` (`+0.79`/`+0.97` pp)|
|`libavutil/core`|`0`|`11`|`+20` (`+1.27`/`+1.73` pp)|`+7` (`+0.53`/`+1.21` pp)|`+29` (`+1.49`/`+2.32` pp)|`+13` (`+0.74`/`-3.14` pp)|
|`libavutil/core`|`0`|`12`|`+20` (`+1.66`/`-1.25` pp)|`+13` (`+0.45`/`+6.43` pp)|`+21` (`+0.90`/`-0.27` pp)|`+22` (`+1.07`/`-0.87` pp)|
|`libavutil/core`|`0`|`13`|`+27` (`+6.37`/`+2.23` pp)|`+0` (`+0.00`/`-1.58` pp)|`+27` (`+6.86`/`+3.53` pp)|`+37` (`+1.83`/`-0.72` pp)|
|`libavutil/core`|`0`|`14`|`+20` (`+2.05`/`+3.38` pp)|`+5` (`+1.19`/`-0.79` pp)|`+26` (`+1.05`/`+4.59` pp)|`+12` (`+0.46`/`-6.57` pp)|
|`libavutil/core`|`1`|`2`|`+21` (`+1.48`/`+0.52` pp)|`+6` (`+0.02`/`-1.00` pp)|`+28` (`+0.91`/`+0.12` pp)|`+30` (`+1.54`/`+1.41` pp)|
|`libavutil/core`|`1`|`3`|`+4` (`+0.10`/`+0.09` pp)|`+0` (`+0.00`/`-0.05` pp)|`+4` (`+0.10`/`+0.08` pp)|`+2` (`+0.10`/`+0.21` pp)|
|`libavutil/core`|`2`|`0`|`+5` (`+0.21`/`+0.17` pp)|`+0` (`+0.00`/`-0.50` pp)|`+4` (`+0.17`/`-0.22` pp)|`+8` (`+0.13`/`-0.37` pp)|
|`libavutil/formats`|`0`|`7`|`+11` (`+0.53`/`-0.56` pp)|`+4` (`+0.15`/`-0.34` pp)|`+8` (`+0.29`/`-1.11` pp)|`+26` (`+2.51`/`-0.02` pp)|
|`libavutil/formats`|`1`|`3`|`+16` (`+3.89`/`+0.11` pp)|`+0` (`+0.00`/`-0.35` pp)|`+27` (`+3.14`/`+0.84` pp)|`+17` (`+1.78`/`+0.38` pp)|
|`libavutil/formats`|`1`|`4`|`+11` (`+2.83`/`+1.00` pp)|`+4` (`+0.22`/`-0.01` pp)|`+15` (`+2.44`/`+1.71` pp)|`+22` (`+1.39`/`-5.50` pp)|
|`libavutil/formats`|`1`|`5`|`+13` (`+0.91`/`+1.55` pp)|`+0` (`+0.00`/`-0.90` pp)|`+11` (`+0.86`/`+1.86` pp)|`+24` (`+0.79`/`+1.03` pp)|
|`libavutil/formats`|`2`|`1`|`+14` (`+0.68`/`+1.12` pp)|`+0` (`+0.00`/`-0.54` pp)|`+16` (`+0.12`/`+1.05` pp)|`+14` (`+0.06`/`-0.56` pp)|
|`libavutil/formats`|`3`|`1`|`+5` (`+0.92`/`+0.21` pp)|`+0` (`+0.00`/`-0.16` pp)|`+4` (`+0.00`/`+0.19` pp)|`+4` (`+0.70`/`+0.12` pp)|
|`libavutil/formats`|`3`|`2`|`+7` (`+0.44`/`+0.06` pp)|`+0` (`+0.00`/`-0.05` pp)|`+5` (`+0.25`/`+0.08` pp)|`+3` (`+0.06`/`-0.01` pp)|
|`libavutil/formats`|`4`|`0`|`+4` (`+0.20`/`+0.03` pp)|`+0` (`+0.00`/`-0.02` pp)|`+5` (`+0.00`/`+0.04` pp)|`+2` (`+0.18`/`-0.04` pp)|
|`libavutil/options`|`0`|`3`|`+15` (`+1.97`/`-1.75` pp)|`+5` (`+10.46`/`+8.97` pp)|`+11` (`+6.00`/`+1.09` pp)|`+24` (`+2.82`/`-0.96` pp)|
|`libavutil/options`|`0`|`4`|`+11` (`+1.91`/`-0.06` pp)|`+11` (`+6.41`/`+0.82` pp)|`+18` (`+4.89`/`+0.22` pp)|`+12` (`+1.25`/`-0.19` pp)|
|`libavutil/options`|`1`|`0`|`+12` (`+1.47`/`-0.67` pp)|`+0` (`-0.02`/`-1.06` pp)|`+10` (`+0.99`/`+0.64` pp)|`+20` (`+1.50`/`-0.12` pp)|
|`libavutil/options`|`1`|`1`|`+10` (`+0.74`/`-0.23` pp)|`+5` (`+2.09`/`+5.72` pp)|`+11` (`+1.61`/`+0.65` pp)|`+14` (`+1.09`/`-0.49` pp)|
|`libavutil/options`|`2`|`0`|`+2` (`+0.36`/`+1.43` pp)|`+3` (`+0.06`/`+1.97` pp)|`+4` (`+0.06`/`+0.34` pp)|`+8` (`+0.22`/`+0.13` pp)|
|`libavutil/crypto`|`0`|`10`|`+21` (`+2.07`/`+0.26` pp)|`+0` (`-0.02`/`-0.46` pp)|`+19` (`+1.80`/`+0.19` pp)|`+5` (`+0.06`/`-1.02` pp)|
|`libavutil/crypto`|`1`|`0`|`+8` (`+2.25`/`+0.36` pp)|`+0` (`+0.00`/`-0.75` pp)|`+6` (`+0.55`/`+0.29` pp)|`+9` (`+1.21`/`-0.68` pp)|
|`libavutil/crypto`|`1`|`1`|`+6` (`+1.06`/`+0.19` pp)|`+0` (`+0.00`/`-0.40` pp)|`+3` (`+0.26`/`+0.16` pp)|`+3` (`+0.39`/`-0.15` pp)|
|`libavutil/crypto`|`1`|`2`|`+9` (`+2.00`/`+0.32` pp)|`+0` (`+0.00`/`-0.42` pp)|`+3` (`+0.00`/`+0.19` pp)|`+6` (`+0.09`/`-0.69` pp)|
|`libavutil/crypto`|`1`|`3`|`+3` (`+0.99`/`+0.06` pp)|`+0` (`+0.00`/`-0.21` pp)|`+1` (`-0.01`/`+0.02` pp)|`+2` (`+0.00`/`-0.41` pp)|
|`libavutil/buffer-frame`|`0`|`5`|`+13` (`+0.16`/`+0.11` pp)|`+0` (`+0.00`/`-0.11` pp)|`+4` (`+0.06`/`+0.06` pp)|`+7` (`+0.09`/`+0.03` pp)|
|`libavutil/buffer-frame`|`1`|`1`|`+13` (`+0.57`/`+0.22` pp)|`+0` (`-0.02`/`-0.48` pp)|`+5` (`+0.00`/`-0.18` pp)|`+6` (`+0.00`/`-0.65` pp)|
|`libavutil/buffer-frame`|`2`|`1`|`+11` (`+0.83`/`+0.13` pp)|`+0` (`+0.00`/`-0.02` pp)|`+3` (`+0.41`/`+0.20` pp)|`+4` (`+0.37`/`+0.18` pp)|
|`libavutil/buffer-frame`|`3`|`1`|`+9` (`+0.43`/`+0.16` pp)|`+0` (`-0.02`/`-0.29` pp)|`+3` (`+0.13`/`+0.26` pp)|`+6` (`+0.38`/`+0.02` pp)|
|`libavutil/buffer-frame`|`4`|`0`|`+12` (`+1.38`/`+0.44` pp)|`+0` (`+0.00`/`-0.06` pp)|`+9` (`+1.18`/`+0.26` pp)|`+18` (`+1.31`/`+0.78` pp)|
|`libavutil/hwcontext`|`1`|`1`|`+6` (`+0.26`/`-1.77` pp)|`+0` (`+0.00`/`-0.75` pp)|`+10` (`+1.94`/`-0.18` pp)|`+19` (`+2.02`/`+0.40` pp)|
|`libavutil/hwcontext`|`1`|`2`|`+2` (`+0.00`/`-0.83` pp)|`+17` (`+0.29`/`+1.82` pp)|`+10` (`+0.47`/`+0.07` pp)|`+17` (`+0.18`/`+0.14` pp)|
|`libavutil/hwcontext`|`2`|`1`|`+0` (`+0.00`/`-0.01` pp)|`+0` (`+0.00`/`+0.00` pp)|`+0` (`+0.00`/`+0.00` pp)|`+0` (`+0.00`/`+0.00` pp)|
| **Σ** |  |  | **`+377` (`+42.81`/`+10.87` pp)** | **`+86` (`+21.80`/`+15.49` pp)** | **`+381` (`+39.76`/`+21.58` pp)** | **`+434` (`+27.11`/`-17.35` pp)** |

### Batches — review

`openrouter/anthropic/claude-fable-5.1` via `claude`.

| subsystem | wave | batch | symbols | +LoC | cov (C/Rust) | rv loc | rv $ | rv tokens | rv wall | rv $/symbol |
|---|---|---|---|---|---|---|---|---|---|---|
|`libavutil/core`|`0`|`10`| `25` |`+45/-1`| `19.02`% / `99.84`% | `+79/-14` | `$18.09` | `15.13M` | `52m22s` | `$0.72` |
|`libavutil/core`|`0`|`11`| `25` |`+75/-0`| `19.02`% / `99.84`% | `+97/-0` | `$16.16` | `13.19M` | `43m21s` | `$0.65` |
|`libavutil/core`|`0`|`12`| `25` |`+145/-6`| `19.14`% / `99.84`% | `+208/-18` | `$26.29` | `31.03M` | `57m24s` | `$1.05` |
|`libavutil/core`|`0`|`13`| `25` |`+163/-9`| `19.14`% / `99.84`% | `+230/-19` | `$25.37` | `33.63M` | `53m00s` | `$1.01` |
|`libavutil/core`|`0`|`14`| `21` |`+65/-12`| `19.00`% / `99.84`% | `+93/-23` | `$10.99` | `12.63M` | `32m35s` | `$0.52` |
|`libavutil/core`|`1`|`1`| `26` |`+56/-5`| `20.40`% / `99.88`% | `+89/-13` | `$10.27` | `11.69M` | `29m53s` | `$0.39` |
|`libavutil/core`|`2`|`0`| `12` |`+0/-0`| `20.57`% / `99.88`% | `+37/-6` | `$10.25` | `13.48M` | `20m24s` | `$0.85` |
|`libavutil/formats`|`0`|`1`| `11` |`+40/-0`| `26.52`% / `99.91`% | `+59/-0` | `$71.89` | `52.22M` | `155m57s` | `$6.54` |
|`libavutil/formats`|`1`|`1`| `56` |`+386/-23`| `32.36`% / `99.88`% | `+470/-28` | `$47.36` | `43.85M` | `107m37s` | `$0.85` |
|`libavutil/formats`|`2`|`1`| `13` |`+385/-0`| `33.33`% / `99.90`% | `+445/-3` | `$12.88` | `18.91M` | `29m40s` | `$0.99` |
|`libavutil/formats`|`3`|`1`| `18` |`+296/-3`| `33.58`% / `99.82`% | `+369/-6` | `$37.27` | `29.62M` | `92m19s` | `$2.07` |
|`libavutil/formats`|`4`|`0`| `2` |`+44/-0`| `33.58`% / `99.82`% | `+78/-7` | `$12.25` | `9.02M` | `57m14s` | `$6.12` |
|`libavutil/options`|`0`|`1`| `29` |`+1112/-350`| `45.39`% / `99.82`% | `+1381/-507` | `$26.49` | `76.11M` | `73m04s` | `$0.91` |
|`libavutil/options`|`1`|`0`| `35` |`+240/-2`| `47.89`% / `99.84`% | `+338/-9` | `$13.35` | `45.11M` | `26m24s` | `$0.38` |
|`libavutil/options`|`2`|`0`| `14` |`+251/-1`| `47.99`% / `99.84`% | `+335/-10` | `$15.34` | `39.12M` | `81m25s` | `$1.10` |
|`libavutil/crypto`|`0`|`1`| `24` |`+199/-89`| `63.94`% / `99.72`% | `+295/-146` | `$5.82` | `16.08M` | `26m34s` | `$0.24` |
|`libavutil/crypto`|`1`|`0`| `79` |`+439/-218`| `65.09`% / `99.74`% | `+551/-263` | `$18.39` | `50.01M` | `131m00s` | `$0.23` |
|`libavutil/buffer-frame`|`0`|`1`| `7` |`+8/-0`| `68.51`% / `99.74`% | `+35/-5` | `$5.52` | `14.01M` | `18m00s` | `$0.79` |
|`libavutil/buffer-frame`|`1`|`1`| `10` |`+137/-0`| `68.55`% / `99.72`% | `+193/-13` | `$8.21` | `21.17M` | `39m19s` | `$0.82` |
|`libavutil/buffer-frame`|`2`|`1`| `15` |`+312/-26`| `69.34`% / `99.73`% | `+472/-48` | `$10.72` | `32.99M` | `32m58s` | `$0.71` |
|`libavutil/buffer-frame`|`3`|`1`| `8` |`+194/-19`| `71.01`% / `99.70`% | `+254/-27` | `$6.66` | `19.47M` | `20m44s` | `$0.83` |
|`libavutil/buffer-frame`|`4`|`0`| `18` |`+157/-22`| `72.39`% / `99.69`% | `+240/-39` | `$26.77` | `48.31M` | `211m34s` | `$1.49` |
|`libavutil/hwcontext`|`1`|`1`| `16` |`+599/-45`| `75.07`% / `99.69`% | `+885/-116` | `$12.97` | `40.61M` | `34m52s` | `$0.81` |
|`libavutil/hwcontext`|`2`|`1`| `2` |`+118/-0`| `75.08`% / `99.69`% | `+154/-0` | `$4.26` | `11.69M` | `15m42s` | `$2.13` |
| **Σ** |  |  | **`516`** | **`+5466/-831`** | **`75.08`% / `99.69`%** | **`+7387/-1320`** | **`$453.57`** | **`699.08M`** | — | **`$0.88`** |

### Batches — review: tests

`openrouter/anthropic/claude-fable-5.1` via `claude`.

| subsystem | wave | batch | +UB safe tests (+C/+Rust pp) | +UB unsafe tests (+C/+Rust pp) | +equiv tests (+C/+Rust pp) | +unit tests (+C/+Rust pp) |
|---|---|---|---|---|---|---|
|`libavutil/core`|`0`|`10`|`+0` (`+0.00`/`-0.08` pp)|`+1` (`+0.00`/`+0.10` pp)|`+0` (`+0.00`/`+0.02` pp)|`+1` (`+0.00`/`+0.01` pp)|
|`libavutil/core`|`0`|`11`|`+0` (`+0.00`/`+0.00` pp)|`+0` (`+0.00`/`+0.00` pp)|`+1` (`+0.02`/`+0.00` pp)|`+0` (`+0.00`/`+0.00` pp)|
|`libavutil/core`|`0`|`12`|`+1` (`+0.00`/`+0.29` pp)|`+0` (`+0.00`/`-0.15` pp)|`+0` (`+0.00`/`+0.21` pp)|`+3` (`+0.00`/`+0.18` pp)|
|`libavutil/core`|`0`|`13`|`+0` (`+0.00`/`-0.05` pp)|`+0` (`+0.00`/`-0.20` pp)|`+2` (`+0.10`/`+0.24` pp)|`+2` (`+0.20`/`+0.23` pp)|
|`libavutil/core`|`0`|`14`|`+0` (`+0.00`/`+0.00` pp)|`+0` (`+0.00`/`+0.00` pp)|`+1` (`+0.00`/`+0.00` pp)|`+0` (`+0.00`/`+0.00` pp)|
|`libavutil/core`|`1`|`1`|`+0` (`+0.00`/`+0.05` pp)|`+0` (`+0.00`/`-0.03` pp)|`+1` (`+0.00`/`+0.05` pp)|`+0` (`+0.00`/`+0.03` pp)|
|`libavutil/core`|`2`|`0`|`+0` (`+0.00`/`+0.00` pp)|`+0` (`+0.00`/`+0.00` pp)|`+0` (`+0.00`/`+0.00` pp)|`+0` (`+0.00`/`+0.00` pp)|
|`libavutil/formats`|`0`|`1`|`+0` (`+0.00`/`+0.00` pp)|`+0` (`+0.00`/`+0.00` pp)|`+0` (`+0.00`/`+0.00` pp)|`+0` (`+0.00`/`+0.00` pp)|
|`libavutil/formats`|`1`|`1`|`+0` (`+0.00`/`+0.02` pp)|`+0` (`+0.00`/`-0.00` pp)|`+1` (`+0.00`/`+0.00` pp)|`+0` (`+0.00`/`+0.00` pp)|
|`libavutil/formats`|`2`|`1`|`+0` (`+0.00`/`+0.02` pp)|`+0` (`+0.00`/`+0.00` pp)|`+1` (`+0.00`/`+0.16` pp)|`+0` (`+0.00`/`+0.00` pp)|
|`libavutil/formats`|`3`|`1`|`+1` (`+0.00`/`+0.09` pp)|`+0` (`+0.00`/`-0.08` pp)|`+0` (`+0.00`/`+0.05` pp)|`+5` (`+0.31`/`+0.11` pp)|
|`libavutil/formats`|`4`|`0`|`+0` (`+0.00`/`-0.02` pp)|`+0` (`+0.00`/`+0.00` pp)|`+1` (`+0.01`/`+0.00` pp)|`+0` (`+0.00`/`+0.00` pp)|
|`libavutil/options`|`0`|`1`|`+1` (`+0.00`/`-0.41` pp)|`+4` (`+0.13`/`+1.59` pp)|`+1` (`+0.60`/`+0.30` pp)|`+4` (`+0.06`/`-0.07` pp)|
|`libavutil/options`|`1`|`0`|`+0` (`+0.00`/`-0.04` pp)|`+1` (`+0.26`/`+1.19` pp)|`+1` (`-0.01`/`+0.02` pp)|`+1` (`+0.06`/`+0.02` pp)|
|`libavutil/options`|`2`|`0`|`+1` (`+0.00`/`+0.01` pp)|`+1` (`+0.00`/`+0.03` pp)|`+2` (`+0.02`/`+0.03` pp)|`+1` (`+0.00`/`+0.00` pp)|
|`libavutil/crypto`|`0`|`1`|`+0` (`+0.00`/`+0.01` pp)|`+0` (`+0.00`/`-0.14` pp)|`+2` (`+0.00`/`+0.05` pp)|`+2` (`+0.00`/`-0.01` pp)|
|`libavutil/crypto`|`1`|`0`|`+0` (`+0.00`/`+0.09` pp)|`+0` (`+0.00`/`+0.01` pp)|`+2` (`+0.00`/`+0.08` pp)|`+1` (`+0.00`/`+0.12` pp)|
|`libavutil/buffer-frame`|`0`|`1`|`+0` (`+0.00`/`-0.02` pp)|`+0` (`+0.00`/`-0.01` pp)|`+0` (`+0.00`/`-0.01` pp)|`+0` (`+0.00`/`-0.01` pp)|
|`libavutil/buffer-frame`|`1`|`1`|`+3` (`+0.00`/`+0.00` pp)|`+0` (`+0.02`/`+0.00` pp)|`+0` (`+0.00`/`+0.00` pp)|`+0` (`+0.00`/`+0.00` pp)|
|`libavutil/buffer-frame`|`2`|`1`|`+3` (`+0.00`/`+0.01` pp)|`+2` (`+0.11`/`+0.47` pp)|`+1` (`+0.00`/`+0.06` pp)|`+1` (`+0.02`/`+0.06` pp)|
|`libavutil/buffer-frame`|`3`|`1`|`+1` (`-0.03`/`+0.02` pp)|`+0` (`+0.00`/`-0.06` pp)|`+1` (`+0.00`/`+0.03` pp)|`+0` (`+0.00`/`-0.19` pp)|
|`libavutil/buffer-frame`|`4`|`0`|`+1` (`+0.02`/`+0.12` pp)|`+0` (`+0.02`/`+0.07` pp)|`+0` (`+0.00`/`-0.03` pp)|`+1` (`+0.00`/`+0.01` pp)|
|`libavutil/hwcontext`|`1`|`1`|`+0` (`+0.00`/`-0.10` pp)|`+0` (`+0.02`/`-0.08` pp)|`+3` (`+0.00`/`+0.00` pp)|`+5` (`+0.11`/`+0.20` pp)|
|`libavutil/hwcontext`|`2`|`1`|`+0` (`+0.00`/`+0.00` pp)|`+0` (`+0.00`/`+0.00` pp)|`+1` (`-0.01`/`+0.00` pp)|`+0` (`+0.00`/`+0.00` pp)|
| **Σ** |  |  | **`+12` (`-0.01`/`+0.02` pp)** | **`+9` (`+0.56`/`+2.72` pp)** | **`+22` (`+0.73`/`+1.26` pp)** | **`+27` (`+0.76`/`+0.69` pp)** |

### Batches — review: PoC + reports

`openrouter/anthropic/claude-fable-5.1` via `claude`.

| subsystem | wave | batch | +lifecycle | +unsafe | +UB safe | +UB unsafe | +equiv | +internal | +conventions | +misc | total | +rejected PoCs |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `libavutil/core` | `0` | `10` | — | `+1` | `+0` | `+0` | `+0` | `+0` | `+0` | `+1` | `2` | `0` |
| `libavutil/core` | `0` | `11` | — | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `0` | `0` |
| `libavutil/core` | `0` | `12` | — | `+0` | `+1` | `+0` | `+0` | `+0` | `+1` | `+1` | `3` | `0` |
| `libavutil/core` | `0` | `13` | — | `+0` | `+0` | `+0` | `+1` | `+0` | `+0` | `+0` | `1` | `0` |
| `libavutil/core` | `0` | `14` | — | `+0` | `+0` | `+0` | `+0` | `+0` | `+2` | `+1` | `3` | `0` |
| `libavutil/core` | `1` | `1` | — | `+0` | `+0` | `+0` | `+1` | `+0` | `+0` | `+0` | `1` | `0` |
| `libavutil/core` | `2` | `0` | — | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+2` | `2` | `0` |
| `libavutil/formats` | `0` | `1` | — | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+1` | `1` | `0` |
| `libavutil/formats` | `1` | `1` | — | `+0` | `+0` | `+0` | `+0` | `+0` | `+1` | `+1` | `2` | `0` |
| `libavutil/formats` | `2` | `1` | — | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+1` | `1` | `0` |
| `libavutil/formats` | `3` | `1` | — | `+0` | `+1` | `+0` | `+0` | `+0` | `+0` | `+1` | `2` | `0` |
| `libavutil/formats` | `4` | `0` | — | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+1` | `1` | `0` |
| `libavutil/options` | `0` | `1` | — | `+2` | `+0` | `+3` | `+0` | `+0` | `+0` | `+0` | `5` | `0` |
| `libavutil/options` | `1` | `0` | — | `+0` | `+0` | `+1` | `+0` | `+0` | `+0` | `+1` | `2` | `0` |
| `libavutil/options` | `2` | `0` | — | `+0` | `+1` | `+0` | `+0` | `+0` | `+0` | `+1` | `2` | `0` |
| `libavutil/crypto` | `0` | `1` | — | `+0` | `+0` | `+0` | `+1` | `+0` | `+3` | `+0` | `4` | `0` |
| `libavutil/crypto` | `1` | `0` | — | `+0` | `+0` | `+0` | `+1` | `+1` | `+2` | `+2` | `6` | `0` |
| `libavutil/buffer-frame` | `0` | `1` | — | `+1` | `+0` | `+1` | `+0` | `+0` | `+0` | `+0` | `2` | `0` |
| `libavutil/buffer-frame` | `1` | `1` | — | `+0` | `+0` | `+0` | `+0` | `+0` | `+1` | `+1` | `2` | `0` |
| `libavutil/buffer-frame` | `2` | `1` | — | `+0` | `+0` | `+1` | `+0` | `+0` | `+0` | `+1` | `2` | `0` |
| `libavutil/buffer-frame` | `3` | `1` | — | `+1` | `+1` | `+0` | `+0` | `+0` | `+0` | `+0` | `2` | `0` |
| `libavutil/buffer-frame` | `4` | `0` | — | `+0` | `+1` | `+0` | `+0` | `+0` | `+0` | `+2` | `3` | `0` |
| `libavutil/hwcontext` | `1` | `1` | — | `+2` | `+0` | `+0` | `+0` | `+0` | `+0` | `+1` | `3` | `0` |
| `libavutil/hwcontext` | `2` | `1` | — | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `0` | `0` |
| **Σ** |  |  | — | **`+7`** | **`+5`** | **`+6`** | **`+4`** | **`+1`** | **`+10`** | **`+19`** | **`52`** | **`0`** |

## Safety audit

Deterministic `crustify ffimap <workdir>` (`ffimap.json`); no model.

### Final tally overview

| | final (`<sha>`) |
|---|---|
| unsafe loc | `<n>` |
| % of loc | `<n>`% |
| blocks | `<n>` |
| % in `impl T` | `<n>`% |
| `unsafe fn` | `<n>` |
| ...public | `<n>` |
| raw ptrs on the safe public interface (types + safe fns) | `<n>` |
| void ptrs on the safe public interface (types + safe fns) | `<n>` |
| FFI calls | `<n>` |
| `&`/`&mut` on a wrapper | `<n>` |
| field proj outside an accessor | `<n>` |

### All metrics

| metric | final (`<sha>`) | `<link-unit>/<subsystem>` (`<sha>`) | … |
|---|---|---|---|
| `code_lines` | `<n>` | `<n>` | … |
| `total_stmts` | `<n>` | `<n>` | … |
| `safe_fns_pub` | `<n>` | `<n>` | … |
| `safe_fns_priv` | `<n>` | `<n>` | … |
| `unsafe_blocks` | `<n>` | `<n>` | … |
| `unsafe_block_stmts` | `<n>` | `<n>` | … |
| `unsafe_block_lines` | `<n>` | `<n>` | … |
| `unsafe_block_code_lines` | `<n>` | `<n>` | … |
| `unsafe_blocks_wrapper_impl` | `<n>` | `<n>` | … |
| `unsafe_blocks_ffi_export` | `<n>` | `<n>` | … |
| `unsafe_fns` | `<n>` | `<n>` | … |
| **`unsafe_fns_pub`** | **`<n>`** | `<n>` | … |
| `unsafe_fns_priv` | `<n>` | `<n>` | … |
| `unsafe_impls` / `unsafe_traits` | `<n>` / `<n>` | `<n>` / `<n>` | … |
| `ty_wrapper` | `<n>` | `<n>` | … |
| `ty_wrapper_borrow_sanctioned` | `<n>` | `<n>` | … |
| **`ty_wrapper_borrow_non_sanctioned`** | **`<n>`** | `<n>` | … |
| `ty_wrapper_by_val_fn_sig_pub` | `<n>` | `<n>` | … |
| `ty_wrapper_by_val_fn_sig_priv` | `<n>` | `<n>` | … |
| `ty_wrapper_by_val_ty_pub_fields_pub` | `<n>` | `<n>` | … |
| `ty_wrapper_by_val_ty_pub_fields_priv` | `<n>` | `<n>` | … |
| `ty_wrapper_by_val_ty_priv` | `<n>` | `<n>` | … |
| `ty_wrapper_std_owner_fn_sig_pub` | `<n>` | `<n>` | … |
| `ty_wrapper_std_owner_fn_sig_priv` | `<n>` | `<n>` | … |
| `ty_wrapper_std_owner_ty_pub_fields_pub` | `<n>` | `<n>` | … |
| `ty_wrapper_std_owner_ty_pub_fields_priv` | `<n>` | `<n>` | … |
| `ty_wrapper_std_owner_ty_priv` | `<n>` | `<n>` | … |
| `ty_non_wrapper_borrow_pub_safe` | `<n>` | `<n>` | … |
| `ty_non_wrapper_borrow_pub_unsafe` | `<n>` | `<n>` | … |
| `ty_non_wrapper_borrow_priv_safe` | `<n>` | `<n>` | … |
| `ty_non_wrapper_borrow_priv_unsafe` | `<n>` | `<n>` | … |
| **`raw_ptr_ty_pub_fields_pub`** | **`<n>`** | `<n>` | … |
| `raw_ptr_ty_pub_fields_priv` | `<n>` | `<n>` | … |
| `raw_ptr_ty_priv` | `<n>` | `<n>` | … |
| **`raw_ptr_fn_pub_safe`** | **`<n>`** | `<n>` | … |
| `raw_ptr_fn_pub_unsafe` | `<n>` | `<n>` | … |
| `raw_ptr_fn_priv_safe` | `<n>` | `<n>` | … |
| `raw_ptr_fn_priv_unsafe` | `<n>` | `<n>` | … |
| `raw_ptr_has_ty_wrapper` | `<n>` | `<n>` | … |
| `raw_ptr_derefs` | `<n>` | `<n>` | … |
| `raw_ptr_derefs_outside_impl` | `<n>` | `<n>` | … |
| `void_ptr_ty_pub_fields_pub` | `<n>` | `<n>` | … |
| `void_ptr_ty_pub_fields_priv` | `<n>` | `<n>` | … |
| `void_ptr_ty_priv` | `<n>` | `<n>` | … |
| `void_ptr_fn_pub_safe` | `<n>` | `<n>` | … |
| `void_ptr_fn_pub_unsafe` | `<n>` | `<n>` | … |
| `void_ptr_fn_priv_safe` | `<n>` | `<n>` | … |
| `void_ptr_fn_priv_unsafe` | `<n>` | `<n>` | … |
| `ty_field_proj_wrapped` | `<n>` | `<n>` | … |
| `ty_field_proj_outside_impl` | `<n>` | `<n>` | … |
| `ty_field_borrow_wrapped` | `<n>` | `<n>` | … |
| `deref_impl_on_wrapper` | `<n>` | `<n>` | … |
| `ffi_calls` | `<n>` | `<n>` | … |

## Planning

`anthropic/claude-opus-5-5` via Claude Code. One row per planner run, in order; its reports are
the corrections it made to what earlier runs committed.

| run | modules changed | items moved | wavefront artifacts changed | reports | $ | tokens | wall |
|---|---:|---:|---|---:|---|---|---|
| 1 | `<n>` | `<n>` | `<files>` | `<n>` | `$<n>` | `<n>M` | `<n>m<n>s` |
| 2 | `<n>` | `<n>` | `<files>` | `<n>` | `$<n>` | `<n>M` | `<n>m<n>s` |
| 3 | 160 | 674 | `configs/wavefront-config.json`, `configs/<link-unit>/<index>/wavefront-config.json` (160 added, 22 per-subsystem removed) | 0 | `$<n>` | `<n>M` | `<n>m<n>s` |
| **Σ** | **`<n>`** | **`<n>`** | | **`<n>`** | **`$<n>`** | **`<n>M`** | **`<n>m<n>s`** |

## Notes

The only prose outside the setup and legend above: pitfalls, findings, and the
context each table cannot carry. One `###` subsection per finding, titled by
what it is about. Describe the EXPERIMENT and its results — a fix made to
crustify or ffibox along the way belongs in that repo's history, not here.

> Gate misses and anything the oracle and `translate` disagreed on; a wave that
> was superseded and why; what each wave's diff actually contained beyond its
> row counts; where a metric moved and what moved it; what the judge found and
> whether it held. Everything else stays in the tables.

State where the LoC figures come from, and note that all of them exclude
comments and blank lines.

`C LoC` is `wavefront query dag --name <every scheduled entity> --loc`:
the oracle's translated-LoC view, a function seed valued at its body LoC and a
type seed at its field and op count. It reports the seeds only, with no closure
expansion, so it is the C the campaign translated rather than the surface it
was drawn from. Give the defining files' and the whole target's raw totals
beside it for scale.

`Rust LoC` is counted from source over the authored `.rs` files under
`crustify/rust`, excluding anything generated into `target/`, and split by
`#[cfg(test)]` module.

Say why that non-test figure differs from the `code_lines` the Safety audit
reports. The audit measures the union of HIR definition spans, so it counts
only what sits inside an item — no `use`, `mod` or free-standing attribute
lines — and by construction cannot see `cfg`-disabled code, which is why it
yields no test figure. Its number is the right denominator for the unsafe
ratios and the wrong one for how much Rust was written; the two must not be
added.

Say plainly that the Rust-to-C ratio is not like-for-like in any measure,
because the Rust carries tests, `// SAFETY:` justifications, `ffi_export`
gateways and scaffolding with no C counterpart.

The four unit counts are de-duplicated over ENTITIES, not scheduled units: an
entity appears once, under the last objective it ran, so a type wrapped and
later ported counts as ported and never in both. Name the entities that took
both paths. Callbacks count with symbols.

Some of it is structural and belongs here every time: whether a review wave
reused the translation batches or the oracle re-batched it under distinct caps;
which units a review schedule dropped and why; which sub-campaigns the Overview
lists but no table details, and the cost that leaves unaccounted; and any column
a campaign could not fill, said once rather than left as a field of em-dashes.

### Sanitizer variants

`crustify/artifacts/build.json` (version 3) carries `plain`, `coverage`, `asan-ubsan`, `tsan` and `bsan`.
The first container denied `personality(ADDR_NO_RANDOMIZE)`, so every TSan runtime and the
bsan shadow runtime aborted at startup; after the container was restarted with seccomp
unconfined, the `tsan` and `bsan` variants were added.

FATE under `tsan` (clang 20, `TSAN_OPTIONS=halt_on_error=0`): 5637/5639. The 202 tests beyond
the `plain` baseline's 5437 are `filter-pixdesc` variants that FATE enables under this clang
configuration and not the gcc `plain` one. `fate-api-h264-slice` reports a pre-existing data
race in slice-threaded H.264 CABAC decoding (`h264_cabac.c:2392` write against
`decode_cabac_field_decoding_flag`'s read at `h264_cabac.c:1299`, on the shared per-picture
state). Race tests that drive libavcodec's slice threads can hit it without any Rust defect.
`fate-source` fails for a campaign reason: see the next subsection.

The `bsan` variant links the runtime once, into executables only, and exports it
(`-rdynamic`); linking it into every shared library gave each library its own shadow mapping,
and the second one aborted. Its runtime (bsan 0.1.0, `3f210d63ae`) reports a false
use-after-free when `realloc(NULL, n)` runs in a non-inlined function and the result is freed
later. This correct program reproduces it:

```c
#include <stdlib.h>
__attribute__((noinline)) void *wrap(void *p, size_t n){ return realloc(p, n); }
int main(void){ char *p = wrap(NULL, 8); p[0] = 1; int r = p[0]; free(p); return r - 1; }
```

`av_realloc` hits it, so the instrumented C executables (`ffmpeg`, FATE tools) abort and no bsan
FATE run exists. Rust test binaries under `cargo bsan` with `CRUSTIFY_FFMPEG_VARIANT=bsan` run
clean against it, `av_strdup` → `av_realloc(NULL, n)` included. Rust TSan runs need nightly:
`CRUSTIFY_FFMPEG_VARIANT=tsan RUSTFLAGS=-Zsanitizer=thread cargo +nightly test -Zbuild-std
--target x86_64-unknown-linux-gnu`.

### `fate-source` regression from the `-sys` scaffolding

FFmpeg's `fate-source` requires every tracked `.c`/`.h` file to carry a licence header and
every header a path-derived include guard. The scaffolded `crustify/rust/*-sys/shim.c`,
`shim.h` and `wrapper.h` have neither: `shim.h` guards on `CRUSTIFY_LIBAVUTIL_SHIM_H` instead of
`CRUSTIFY_RUST_LIBAVUTIL_SYS_SHIM_H`, and `wrapper.h` has no guard. The `plain` baseline (5437/5437)
ran before they were committed, so `fate-source` fails against the current tree.

### Deprecated API exclusion

FFmpeg has no configure switch for deprecated APIs: `FF_API_*` is hard-defined in each
`version_major.h`, and changing it breaks ABI and FATE, so the C builds keep them. They are
excluded at schedule time instead. `crustify/wavefront/deprecated.json` diffs clang ASTs of the
staged public headers as shipped and with every `FF_API_*` forced to 0, plus `DeprecatedAttr`:
2 symbols (`av_init_packet`, `av_assert0_fpu`) passed to `--skip`, 9 fields removed from
`field_anchors` (`AVCodecContext.intra_dc_precision`, six `AVDOVIDmLevel11` fields,
`AVStreamGroup.lcevc`, `AVStreamGroupLayeredVideo.lcevc_index`) and 13 FF_API-gated macros.

### Platform and vendor headers out of scope

Installed but not built on this Linux host, so neither linkable nor testable, and left out of
`api_headers`: `libavutil/hwcontext_{amf,cuda,d3d11va,d3d12va,drm,dxva2,mediacodec,oh,opencl,qsv,vaapi,vdpau,videotoolbox,vulkan}.h`
and `libavcodec/{d3d11va,dxva2,jni,mediacodec,qsv,vdpau,videotoolbox}.h`. `jni.c`,
`mediacodec.c` and `hwcontext_stub.c` still compile as ENOSYS/NULL stubs and stay homed in
their subsystems. The generated `avconfig.h` and `ffversion.h` live in the build tree, not the
repo, and reach Rust only through bindgen.

### C test baseline

`make -k -j48 fate` against the `plain` build with the full fate-suite samples: 5437/5437
pass, none disabled. `fate-list` enumerates 5438; the extra `fate-hwdevice` belongs to
`fate-hw` only, not `fate`. The shared-library build needs `LD_LIBRARY_PATH` over its `lib*/`
dirs, which is why `build.json` reached version 2; the `tsan` and `bsan` variants took it to 3.

### Subsystem decomposition and cycle cuts

22 subsystems over 7 shared-library link units, ordered by the linker's NEEDED graph
(libavutil, libswresample, libswscale, libavcodec, libavformat, libavfilter, libavdevice).
`libavcodec/codecs`, `libavformat/formats`, `libavfilter/filters` and `libavdevice/devices`
hold the internal TUs, publish no API and are not scheduled. Cuts implied by the order
(consumer scheduled before producer):

- `libavutil/metadata` → `libavcodec/codecs`: 20 edges
- `libavutil/dsp` → `libavcodec/codecs`: 2 edges
- `libavutil/metadata` → `libavcodec/core`: 2 edges
- `libavutil/core` → `libavutil/options`: 5 edges
- `libavutil/core` → `libavutil/crypto`: 5 edges
- `libavutil/formats` → `libavutil/options`: 1 edges
- `libavutil/buffer-frame` → `libavutil/hwcontext`: 2 edges
- `libswscale/swscale` → `libavfilter/filters`: 4 edges
- `libavcodec/codecs` → `libavfilter/filters`: 36 edges
- `libavcodec/codecs` → `libavformat/formats`: 22 edges
- `libavcodec/core` → `libavcodec/bsf`: 8 edges
- `libavcodec/core` → `libavcodec/utils`: 29 edges
- `libavcodec/core` → `libavcodec/codecs`: 232 edges
- `libavcodec/codecs` → `libavcodec/utils`: 74 edges
- `libavcodec/codecs` → `libavcodec/bsf`: 7 edges
- `libavformat/core` → `libavdevice/core`: 2 edges
- `libavformat/avio` → `libavformat/formats`: 103 edges
- `libavformat/core` → `libavformat/formats`: 443 edges
- `libavfilter/core` → `libavfilter/filters`: 363 edges
- `libavdevice/core` → `libavdevice/devices`: 1 edges

The three cross-link-unit cuts come from libavutil compiling libavcodec's inline
`get_bits.h`/`put_bits.h`/`mathops.h` and using `AV_INPUT_BUFFER_PADDING_SIZE`; they are
header-level inclusions, not link dependencies.

### Planned shape

`schedule.json`: 20 sub-campaigns (2 raw-lifetime, 18 subsystems), 70 waves, 273 batches:
290 types with 1387 field anchors, 1119 symbols, 7 callbacks, homed in 106 `.rs` files. Each
per-subsystem wavefront schedule used `--file <api_headers + TUs> --transitive
--api-headers-only`. Items that `--transitive` pulled into more than one sub-campaign (120)
stay with the first scheduled one. Out-of-tree libc types (`tm`, `_IO_FILE`) were dropped.
Field anchors are C field identifiers: a member path through a named nested aggregate
(`u.mask`) anchors on its top-level field (`u`), and a C11 unnamed member adds no segment
(`(unknown field).l1` is `l1`). The `void` and `string` raw lifetimes home in
`libavutil/mem.rs`, beside the primitives their strategies call.
Wavefront was `557055d` (`scheduling/campaign-default-caps`).

### Bindgen type ownership

Each `-sys` crate allowlists every type its in-scope public headers define, from an
orchestrator-owned `api-types.txt`, and blocklists its dependencies' headers and type names,
so each C type has one Rust type. Functions and constants remain agent-owned in
`allowlist.txt`. Without this, upper link units would have to edit lower crates' allowlists
from parallel batches.

### Review model

Review uses `openrouter/anthropic/claude-opus-5.5` via `claude` instead of the `openrouter/openai/gpt-5.6-sol`
named in `crustify/TASK.md`, at the user's direction: the provider's cybersecurity content
filter stops the `codex` reviewer on this workload.

### `raw-lifetime-void`

The translation batch (`42bb68057f`, `+775/-0` `.rs`) bound `av_free` as the release policy
`Heap` for single objects (`CDrop`) and arrays (`impl_clendrop!`), and `av_memdup` as the
array clone strategy (`impl_clenclone!`): the 3 strategies counted above. It also bound the
allocators `av_malloc` (`CAlloc`), `av_mallocz` (`CAllocZeroed`), `av_malloc_array` and
`av_calloc`, behind `HeapBox`/`HeapVec` and the safe `alloc_array`, `alloc_array_from_fn`,
`alloc_array_zeroed` and `alloc_cell_array_zeroed`, declaring the 16-byte alignment `mem.c`
guarantees in every configuration and rejecting over-aligned element types at compile time.
`av_freep` got no wrapper, since dropping the owner does its job, and `av_realloc` no lifecycle role.
It added 10 unit, 9 equivalence and 15 `ub_safe` tests (4 of them trybuild rejections) and 2
`compile_fail` doctests, and submitted the primitives' ownership findings to wavefront. The
`symbols` count of the Overview row is the 6 primitives it anchored.

The scheduled workset name `void` names no C entity, so `ffimap --name void` matches no
site; both batch dirs keep that output (`ffimap.void.txt`) and a scan seeded with the 6
anchored primitives (`ffimap.json`). Against the empty base, the translation batch moved
the `unsafe fn` smell by `+5` and the raw-pointer smell by `+5`, and the review by `+1` and `0`.
The scan's `unsafe_fns_pub` (3, then 4) counts the `unsafe fn` methods of ffibox's unsafe
policy traits implemented on `Heap`; neither batch added public unsafe API. The two raw-pointer
positions are the private `free_len`/`memdup` adapters whose signatures ffibox's macros fix.

Under `bsan`, freeing an `av_calloc` block after a Rust byte view of 32 KiB or more was taken
over it grew the process without bound (about 180 GB in 12 minutes), while 4 KiB views ran
clean; the translator attributes it to the bsan runtime, and the equivalence tests keep
successful allocations at 4 KiB or less.

The review (`6ae24c32fa`, `b8afddd2e6`) filed three defects, all confirmed on rerun:

- `lifecycle/void`: `av_memdup` was bound only as the array cloner, so `HeapBox<T>` could not
  be cloned although FFmpeg copies single objects with it (`dnxhdenc.c`, `mpegvideo.c`). Fixed
  with a `CDupClone` for `T: CCell + Copy`, under the same 16-byte alignment bound.
- `wavefront/av-calloc-av-memdup-scalar-paths`: the `av_calloc` return and the `av_memdup`
  source and return were recorded as arrays only; resubmitted with their single-object paths.
- `ub_unsafe/ffibox-cvec-clone-ignores-align`: ffibox's `CVec::clone` copies through
  `CLenClone::c_clone_len` without the `align_of::<T>() <= ALIGN` check that `from_slice` and
  `to_cvec` apply, so safe `clone()` + `as_slice()` forms a misaligned `&[u32]` from a buffer
  adopted within `from_raw_parts`'s contract. Rerun: the debug build aborts (exit 134) and Miri
  reports the unaligned reference; against the ffibox patch branch the reproducer is rejected at
  compile time (`E0080`). The crate's `Heap` declares `ALIGN = 16` and bounds every safe
  `HeapVec` element by it, so its safe surface does not reach the bug.

No reproducer was rejected. The promoted tip passes `clippy`, the plain test suites and the
`asan-ubsan` suites.

### `raw-lifetime-string`

The translation batch (`271811c5bb`, `+429/-4` `.rs`) added `HeapString`, ffibox's `CStrBox`
under a separate `StringHeap` policy: `av_free` releases it and `av_strdup` clones it, the 2
strategies counted above. `Heap` could not carry the string clone beside its singleton
`CDupClone` (`E0119`). Safe constructors wrap `av_strdup` (`string_from_c_str`) and
`av_strndup` (`string_from_bytes_until_nul`); the `symbols` count of the Overview row is
these 3 anchored primitives. It added 5 unit, 6 `ub_safe` (cross-thread drop and clone,
non-UTF-8 and interior-NUL input), 2 trybuild and 6 equivalence tests, and recorded
`av_strdup`/`av_strndup` as string cloners and `av_asprintf`, `av_append_path_component`,
`av_get_token` and `av_strireplace` as string constructors in wavefront. Those four stay
scheduled as `libavutil/core` symbols. Seeded with `av_free av_strdup av_strndup`, the scan
moved the `unsafe fn` smell by `+2` and the raw-pointer smell by `0`; the review moved
neither.

The review (`a1b075fd7d`, `8cda7b6b96`) filed three reports and no reproducer, so none was
rerun or rejected:

- `wavefront/string-out-param-constructors`: six functions that return a new string through
  a `char **` out-parameter (`av_escape`, `av_dict_get_string`, `av_opt_serialize`,
  `av_opt_get`, `av_opt_get_key_value`, `av_bprint_finalize`) had no record; resubmitted.
  Wavefront's schema cannot mark an out-slot's contents as a string, so
  `--lifetime-for string` still does not list them.
- `conventions/string-policy-hand-written-impls`: the `av_strdup` clone is now bound through
  ffibox's `impl_cdupclone_str!`; the `av_free` drop stays hand-written (`/// Handwritten`)
  because the macro needs a raw-pointer adapter that the scan counts.
- `misc/string-max-alloc-limit-docs`: the constructors' docs were off by one on the
  `av_max_alloc` limit (C allocates length + 1). Fixed, with an equivalence test that lowers
  the limit in a re-executed child process on both sides of the boundary.

The review also observed that `av_bprint_finalize` on an `AV_BPRINT_SIZE_COUNT_ONLY` buffer
returns a string with no NUL: the buffer's `size` is 0, so `real_size` is 0 and
`av_memdup(str, 0)` hands back a 1-byte allocation with nothing copied. It reproduced a
heap-buffer-overflow in `strlen` on the `asan-ubsan` build, and recorded the case in the
function's wavefront note for the batch that wraps it. The promoted tip passes `clippy`, the
plain test suites and the `asan-ubsan` suites.

### ffibox `695d52d`

Both raw-lifetime tiers were translated and reviewed against ffibox `508e478`. After the
string tier, ffibox `main` took the `CVec::clone` alignment fix (`695d52d`), and the gates were
rerun at `46e92abb02` against it: `clippy` clean, and the plain, `asan-ubsan`, `tsan` and `bsan`
suites pass. The void review's reproducer no longer compiles (`E0080`).

### `libavutil/core` wave 0

Wave 0 is 15 batches: 10 type batches (18 types, 21 scheduled field anchors) and 5 symbol
batches (121 symbols), all `wrap`. The interrupted session launched batches 0–2 and 4–9 at
02:54Z on 2026-10-08; all nine landed by 03:27Z. It never prepared batch 3 or batches 10–14 and
left no record of why. The resumed session launched those six at 18:46Z from the wave tip
`61e011df35`; they landed between 19:15Z and 21:45Z. All 139 scheduled items carry their anchor
on the wave tip `5d0c0b3b31`. Translation cost `$172.86` for 424.5M tokens and 13h41m of agent
wall time.

The wave tip passes `clippy --all-targets -D warnings`, the plain suites (`compile_fail`
included) and the `asan-ubsan`, `tsan` and `bsan` suites: 211 unit, 213 equivalence, 200
`ub_safe` and 41 `ub_unsafe` tests under `bsan`.

What the diffs contained beyond the row counts:

- Batch 2 (`AVOption`, `AVOptionRanges`) also anchored `av_opt_query_ranges`,
  `av_opt_query_ranges_default` and `av_opt_freep_ranges`, which `schedule.json` places in
  `libavutil/options` wave 2 batch 0, and 16 `Field:` anchors on the two types, which the
  schedule gave no `field_anchors`. The `fields` column counts scheduled anchors only, so the
  row reads `0`. The empty `field_anchors` on two structs with public fields looks like a gap in
  the schedule. The three functions count as done when that options batch runs.
- The 25-symbol batches took 86–179 minutes against 10–33 for the type batches. Batches 12 and 13
  share homes (`log.rs`, `avstring.rs`) with siblings that landed first, rebased 3 and 6 times
  and revalidated the full sanitizer matrix after each.

How the rows were measured. Every per-batch figure compares the batch's landing commit with its
parent, so it is that batch's own change. The toolchain commit below has no row.

- `+LoC` (and the Symbols `loc`, its insertions) counts `.rs` lines under `crustify/rust`,
  tests included, excluding blank and comment-only lines.
- Test counts are `#[test]` functions: unit under `crustify/rust/ffmpeg` outside `tests/`,
  and `tests/{ub_safe,ub_unsafe,equiv}` excluding `compile_fail/` programs.
- C coverage is line coverage over the 12,558 executable lines of libavutil's 94 compiled TUs,
  from the `coverage` prebuild through `gcov`. `.gcda` output is redirected with `GCOV_PREFIX`.
  Header inlines compiled into the `-sys` shim (`av_int2float`, `av_clip_*`, `av_bswap*`) are
  not instrumented and count nowhere.
- Rust coverage is line coverage from `-C instrument-coverage` (nightly `1d81eb4ad`) over the
  authored non-test sources of `crustify/rust/ffmpeg`, excluding `tests/` and `#[cfg(test)]`
  modules. Paths are normalised first, so `tests/common` helpers the library's unit tests include
  through `#[path = "../tests/common/.."]` stay excluded (see "Rust coverage recomputed without
  test helpers"). All four test binaries serve as coverage objects for every workload, so the four
  workloads share one denominator per commit.
- Each workload ran alone, on the plain build: unit is `--lib`, the others their test binary,
  all with `--skip compile_fail`.
- `cov (C/Rust)` is the all-workloads run (the union) at the landing commit, over the same
  C and Rust denominators. It is cumulative in landing order, not row order: the type batches
  landed as 0, 9, 7, 1, 5, 4, 8, 6, 2, then 3 after the resume, and the symbol batches as 14,
  10, 11, 13, 12. Each Σ `cov` is the table's last landing overall, now in wave 1 (see below).
- Σ coverage cells in the `: tests` tables add the per-row deltas. Denominators grow with every landing, so a Σ is
  not the wave-level change, and negative Rust deltas mark landings whose new lines a given
  workload does not reach.
- Smell deltas come from unseeded scans (`ffimap.tree.json`) at the landing commit and
  its parent. They sum to `+13` (`unsafe fn`) and `+15` (raw pointer) over the base's 8 and 5,
  which equals the wave tip's 21 and 20. The seeded scans of batches 0, 1 and 4 matched no site
  (`ffimap.txt`).

### Toolchain drift before the wave-0 resume

Between the interrupted session and the resume, the container's toolchains changed:

- nightly went from 1.100 to `1d81eb4ad` (2026-10-07) at 03:34Z;
- the bsan toolchain went from clang 23.1.0 to 23.1.3, runtime `3f210d63ae` to `de6ede0149`,
  at 03:36Z;
- stable `rustc` was reinstalled at 16:11Z;
- `zlib1g-dev` disappeared, while the zlib runtime stayed.

Effects and fixes:

- **Stable `rustc`.** It added a "struct named `Rounding` exists in another namespace" note to
  one trybuild `.stderr` (batch 4), and its clippy flagged a deliberate `Clone::clone` on a
  `Copy` type in batch 7's tests. Commit `61e011df35` re-blessed the `.stderr` and allowed
  `clone_on_copy` on that statement. Both are test-only.
- **BSan prebuild.** It was rebuilt with the unchanged `build.json` commands (still version 3)
  after reinstalling `zlib1g-dev`; its `CONFIG_*` set is identical. The new runtime no longer
  reports the `realloc(NULL, n)` false use-after-free. Instead it aborts multi-threaded suites
  when its garbage collector waits more than 10 s for a thread to reach a safepoint;
  `BSAN_OPTIONS=gc_hang_threshold_ms=600000` lets every suite pass.
- **`crustify ffimap`.** Its `rustc_private` driver no longer built on the new nightly, so
  it reported no counts. The fix, crustify `4efc0cc` (merged into `main` on 2026-10-09), ports
  the driver and pins it to `nightly-2026-10-07`. Every wave-0 scan comes from
  that driver. Its scan of the wave base reproduces the raw-lifetime smells recorded above
  (8 and 5).

The six resumed translators lost time to the broken scan and the BSan abort before working
around them. The interrupted run's translators also wrote `.gcda` counters straight into the
shared `coverage` prebuild (92 files, last at 03:08Z); the coverage figures above never read
them.

### `libavutil/core` review wave 0: Fable 5.1 experiment

At the user's direction, `libavutil/core` review wave 0 ran on
`openrouter/anthropic/claude-fable-5.1` (api, `claude` backend) instead of the configured Opus 5.5,
as an experiment. It also ran right after wave-0 translation rather than at sub-campaign end,
so a reviewed wave 0 is the base of `core` wave 1. All other reviews stay on Opus 5.5 and the
after-each-sub-campaign policy. The review tables' model line names Fable because every row in
them is from this wave. The review reused the 15 translation batches unchanged
(`objective: review`), with no re-batching and no units dropped.

Results:

- **Landings:** 14 of 15 reviewers landed 27 commits on
  `crustify/reviews/libavutil/core/wave-0` (tip `4cc197e17e`). The batch-1 reviewer
  checked `AVEscapeMode` and `AVMediaType` under every sanitizer, found nothing and landed no
  commit; its row reads `+0/-0` and `—` for `cov`.
- **Reports:** 29 defect reports plus 4 batch summaries. The PoC tables count 22. They have no
  `wavefront` column, so the 7 `wavefront/` reports (record corrections, submitted to the
  ownership store) are counted nowhere above. Three of those correct union types recorded as
  `kind: struct` (`av_alias64`, `av_intfloat32`, `unaligned_64`).
- **Reproducers:** all five runnable ones were rerun on their affected commit and on the
  reviewed tip, and all five hold, so none is rejected:
  - `ub_safe/memcpy-backptr-block-length-overflow` (batch 12). On `5d0c0b3b31`, safe
    `copy_back_reference` with a distance of 2^30 and a run one byte longer makes C's `int`
    block length overflow (UBSan, `mem.c:466`). `memcpy` then runs with a negative size
    (ASan). On `4cc197e17e` it fills the run and exits 0: the wrapper now splits such runs
    into calls within the bound.
  - `equiv/parse-cpu-caps-prefixed-operand-rounds` (batch 13). `parse_cpu_caps` rounded `+`/`-`
    operands where C truncates them, and rejected some that C accepts: 10 inputs diverged on
    `9c3345929b`, and all agree on the reviewed tip.
  - `unsafe/default-item-name-alignment-unstated` (batch 10). A misaligned context that
    satisfied the old `# Safety` text triggers UBSan's misaligned load at `log.c:243` on
    `4ec92d5001`. The contract now requires alignment.
  - `misc/option-ranges-box-not-send-sync` (batch 2): 15 `E0277` errors on `ab1d2518bf`; builds
    on the reviewed tip.
  - `misc/ffibox-cslice-to-cvec-plain-only` (batch 3): `E0599` against ffibox `695d52d`. The fix,
    ffibox `196c5fb`, was merged into `main` on 2026-10-09; the reproducer then builds and exits 0.
- **Summary:** one UB reachable from safe code, one equivalence divergence, one incomplete
  `unsafe fn` contract. Every one is in a symbol batch. The ten type batches yielded only
  missing traits (`Hash`, `Debug`, `Send`/`Sync`), missing borrowed views, documentation and
  wavefront records.
- **Effect on the tree:** the review landings changed neither smell, and the reviewed tip's
  unseeded scan equals the wave tip's (21 and 20). The `asan-ubsan`, `tsan` and `bsan` suites
  and `clippy` pass on the reviewed tip. Under `bsan` that is 227 unit, 230 equivalence, 211
  `ub_safe` and 44 `ub_unsafe` tests. The 2 GiB `copy_back_reference` regression is skipped
  under `bsan` and `tsan`.
- **Measurement:** review rows use the same method as translation rows, over each batch's
  commit range on the review branch. `+LoC` counts code lines; `rv loc` is raw `.rs`
  insertions/deletions, comments included.
- **Promotion:** the reviewed tip was promoted to `crustify/subsystems/libavutil/core`.

Cost: `$174.05` for 213.3M tokens and 8h21m of agent wall time: `$77.15` on type batches,
`$96.90` on symbol batches. That is about the wave's translation cost (`$172.86`), for half the
tokens: Fable 5.1 is priced at 2.5× Opus 5.5 per token ($10/$50 against $4/$20 per million input
and output tokens). The Opus reviews so far cover only the two raw-lifetime tiers (`$4.69` and
`$5.25`, one batch each, 6 reports, one runnable reproducer), too different a workload to
compare against. The like-for-like comparison is Opus 5.5 reviewing `core` waves 1–2.

About 23% of the reviewers' wall time (~115 of 502 min) went to waiting on BorrowSanitizer
runs, and about 38% of the six resumed translators' (~243 of 635 min), measured from the
agents' transcripts. Most of it is the full suite rerun after each rebase.

### `libavutil/core` wave 1

Wave 1 is 4 `wrap` batches based on the reviewed wave-0 tip `4cc197e17e`:

- batch 0: `AVBPrint` and `AVClass`, 18 fields;
- batch 1: `ff_pad_helper_AVBPrint`, 5 fields;
- batch 2: 25 symbols;
- batch 3: `av_ts_make_time_string2`.

All four landed between 00:42Z and 01:36Z on 2026-10-09, and every scheduled item and field
carries its anchor on the wave tip `b2cff27cd6`. Translation cost `$33.60` for 91.7M tokens and
2h19m of agent wall time.

Notes on the wave:

- `av_make_q`, scheduled in batch 2, was already anchored by wave 0's `AVRational` batch
  (`2823f72b7c`); batch 2 left it as completed work.
- The 25-symbol batch took 64 minutes, against 86–179 in wave 0, with no sibling landing in its
  homes.
- Batch 0 accounts for most of the wave's smell growth (`+9` `unsafe fn`, `+7` raw pointer):
  most of it is `AVClass`'s callback fields, wrapped as `unsafe fn call` handles over C
  function pointers taking `void *` contexts (4 raw-pointer sites in `log.rs`), and the rest
  `AVBPrint`'s buffer (3 in `bprint.rs`).
- Batch rows sum to `+11` and `+8`, which equals the wave tip's unseeded scan (32 and 28) over the
  reviewed base (21 and 20). No undocumented `unsafe`: the workspace denies
  `clippy::undocumented_unsafe_blocks` and `clippy -D warnings` passes (the scan's own
  `undocumented_unsafe` count reads 0 of 0 for every crustify campaign; see below).
- The Σ `cov` cells now read wave 1's last type and symbol landings: batch 0 (`c530b56cec`) and
  batch 2 (`b2cff27cd6`, the wave tip). The rows' `cov` and `: tests` figures continue the
  wave-0 method.

The wave tip passes `clippy --all-targets -D warnings` and the plain, `asan-ubsan`, `tsan` and
`bsan` suites: 275 unit, 273 equivalence, 252 `ub_safe` and 59 `ub_unsafe` tests under `bsan`.
Its review runs with wave 2's, on Opus 5.5, after wave 2.

### Dependency merges after `core` wave 1

On 2026-10-09, at the user's direction:

- crustify `main` took the `ffimap` driver pin (`4efc0cc`) and the results-template split
  (`2a4a613`). The live `crustify ffimap` then gave the same counts on the wave-1 tip as the
  fix branch had.
- ffibox `main` took `196c5fb` (`CSlice::to_cvec` for `Copy` cells).

Against the merged ffibox, the wave-1 tip passes `clippy` and the plain (918 tests) and
`asan-ubsan` (878) suites.

### `libavutil/core` review wave 1: per-kind reviewers, Astra guardrail

From wave 1 on, at the user's direction, review runs after every wave. Each review wave has
one reviewer per item kind, over the merged worklist of all the wave's batches of that kind,
with the review caps lifted to fit. crustify rejects a batch that mixes the type and symbol
routes, so a wave with both kinds gets two reviewers. In wave 1 that is batch 0 (3 types, 23
fields) and batch 1 (26 symbols). The `batch` column of the review tables indexes these merged
batches, not the translation batches.

The user chose `openrouter/openai/gpt-6-astra` (via `codex`) for this wave:

- **Types reviewer:** ran 13m21s without any provider intervention and landed `fd0282416a` and
  `b877190f99`. Its 5 reports are 2 `misc` (a `BPrint` doctest that did not compile, `AVClass`
  docs that assumed `'static` classes) and 3 `wavefront`. It found no UB or equivalence
  defect, added `BPrint` allocation-failure and `ClassRef` lifetime tests, and brought
  `bprint.rs` to full Rust line coverage.
- **Symbols reviewer:** ran 2m51s before OpenAI's filter stopped it, still in its reading phase
  and having written nothing. The error was twice "This content was flagged for possible
  cybersecurity risk … apply for Daybreak access", and it exited 1. Just before, it had opened
  the wave-0 `memcpy-backptr-block-length-overflow` report (ASan `negative-size-param` and
  UBSan traces) alongside its own instructions to write UB reproducers. That attempt cost
  `$3.46` for 1.16M tokens; its log and usage record are kept under
  `review-wave-1/batch-1/attempt-0-gpt-6-astra/`. It is in no batch table above, but the
  Overview's `libavutil/core` review cost includes it.
- **Symbols re-run:** at the user's direction, on `openrouter/anthropic/claude-fable-5.1` (via
  `claude`), from the review tip after the types landing. That is the symbol row above.

Findings:

- **Equivalence drift (batch 1):** `equiv/nearest-refuses-lists-c-finishes`. `Rational::nearest`
  refused every candidate list holding two `i32::MIN` denominators. C overflows only when it
  compares that pair, which happens only when the first of them is the nearest so far.
  Rerun on `b2cff27cd6`: 3 lists where C returns an index and Rust `None`. On `1c02ec54b0` all
  agree, and the C calls run clean under `asan-ubsan`. Not rejected. The fix walks C's
  loop with `cmp_distance` (C's `av_nearer_q`), so `None` now means exactly that C would
  overflow.
- **Smells and documentation:** neither reviewer moved either smell; the reviewed tip's
  unseeded scan equals the wave tip's (32 and 28); `clippy -D warnings` with the denied
  `undocumented_unsafe_blocks` lint passes, so no `unsafe` block lacks a `// SAFETY:` comment.

The reviewed tip passes `clippy --all-targets -D warnings` and the plain, `asan-ubsan`, `tsan` and
`bsan` suites: 276 unit, 275 equivalence, 254 `ub_safe` and 59 `ub_unsafe` tests under `bsan`. It was
promoted to `crustify/subsystems/libavutil/core`. Both reviewers wrote Conventional
Commit subjects (`fix(avutil): …`) instead of the campaign's `crustify: …`. Review cost for the
wave: `$23.44` landed, `$26.90` with the stopped attempt.

### `ffimap`'s `undocumented_unsafe` reads 0 of 0 here

The scan's lexical `undocumented_unsafe` walker skips every path that has a `crustify`
directory component (`_SKIP_DIRS` in `crustify_audit/unsafe_scan.py`, meant to exclude
the audit tree's reproducers). A crustify campaign keeps its whole Rust workspace under
`<repo>/crustify/rust/`, so the walker skips every file and reports `sites: 0` on every
scan in this campaign, while the driver counts 431 `unsafe` blocks at `aedcb9f1a5`. The
driver's counts and the smells are unaffected. Statements here that no `unsafe` block lacks a
`// SAFETY:` comment rest on `clippy::undocumented_unsafe_blocks`, which the workspace denies
and which every gated tip passes.

### `ffimap` counter redefinition after `core` wave 1

On 2026-10-09, crustify `main` took `f66b2c3`..`a2b5c64`, which change what the scan counts.
All figures above this note were measured before the change and stay as measured.
Later the same day `821ee53`..`df5688f` added `--sites`, split `ref_to_type_wrapper` into
`_sanctioned` (73 on the same tree) and `_smell` (1: `Class::as_c_ptr(&self)`, `log.rs:472`), added
`ref_to_type_wrapper_body_smell` (0), and documented the known limitations in crustify's
`docs/audit/unsafe-output.md`.
Then `b2f9240`..`68601d7` added `deref_impl_on_wrapper` (0) and `raw_ptr_fields`: raw pointers in
the fields of structs and unions the crate declares join the raw-pointer partition (7 on the
same tree, all private smell, 2 of them `c_void`), so the raw-pointer smell reads 16 pub + 23
private and the `c_void` smell 14 pub + 5 private. `PhantomData<*const T>` markers stopped
counting as positions. From `core` wave 2 the per-batch scan is unseeded (`--name` is no
longer used), per the updated orchestrator prompt.

- **Seam:** the unsafe lifecycle trait methods (`c_drop`, `c_drop_len`, `c_dup`, `c_clone_len`,
  `c_up_ref`, `c_is_sole_owner`, `c_lock`, `c_unlock`, `c_dispose`) join the seam name list.
  Their trait fixes their `unsafe fn` signature.
- **Partitions:** `unsafe_fns`, the raw-pointer positions and the `c_void` positions are each
  split into seam + pub smell + private smell. "Pub" is rustc's effective visibility
  (callable from outside the crate). This replaces `unsafe_fns_pub`, which counted declared
  visibility and included seam functions, and `void_ptr_sanctioned` / `void_ptr_smell`.
  `raw_ptr_in_wrapper` is dropped.
- **Positions:** a raw pointer nested in a reference, generic argument, tuple, array or slice
  of a parameter or return type now counts, a pointer to a pointer counts once, and
  fn-pointer types are not entered.
- **Sites:** `raw_ptr_sites` is renamed `raw_ptr_smell_sites` and lists every smell position,
  not only those whose pointee has a wrapper.
- **`undocumented_unsafe`:** its walker now applies the skip list below the workspace root,
  so it measures this campaign's crate: 18 of 1044 sites have no `// SAFETY:` comment in
  its lookback window. The ones checked are long comments and `unsafe impl Send` blocks, not
  gaps; `clippy::undocumented_unsafe_blocks` passes.

Same Rust tree (`core` waves 0–1 reviewed), old against new definitions:

| | old | new |
|---|---|---|
| `unsafe fn` | `75`: `43` seam, `24` pub (declared, seam included) | `75`: `53` seam, `14` pub smell, `8` private smell |
| `unsafe fn` smell | `32` | `22` |
| raw-pointer positions | `131`: `103` seam, smell `28` | `138`: `106` seam, smell `16` pub + `16` private |
| `c_void` positions | `60`: `44` sanctioned, smell `16` | `61`: `44` seam, smell `14` pub + `3` private |

The 14 public `unsafe fn` smells are the 14 explicit `pub unsafe fn`s, 11 of them taking an
untyped `AVClass`-enabled object, and the 14 public `c_void` smells are those objects'
parameters. Nested counting found two public raw pointers into C memory from the safe API:
`OptionRangesRef::ranges` and `get` return `*const ffi::AVOptionRange` until that type is
wrapped in `libavutil/options`.

### `libavutil/core` wave 2

Wave 2 is one `wrap` batch of 12 `bprint.h` symbols, based on `c0589fdcbf` (the reviewed
wave-1 tip `1c02ec54b0` plus the updated task settings). It landed `06a7548c5c` at 00:47Z on
2026-10-10 on its first push, and every scheduled item carries its anchor there. Translation
cost `$7.58` for 20.9M tokens and 31m30s of agent wall time.

- `av_bprint_init`, `av_bprint_init_for_buffer` and `av_bprint_finalize` were already anchored
  by wave 1's `AVBPrint` type batch (`c530b56cec`) as `BPrint`'s constructors and finalizers;
  the batch left them as completed work. It wrapped the other nine as `BPrintMut` and
  `BPrintRef` methods, routed `av_vbprintf` through a new `crustify_av_vbprintf` shim (Rust
  cannot build a `va_list`), and implemented `fmt::Write` on `BPrintMut`.
- `av_bprint_strftime` takes libc's `struct tm`, which no batch schedules, so the batch mirrored
  it as a handwritten, Rust-owned `Tm` in `bprint.rs`. Later `struct tm` users
  (`av_small_strptime`, `av_timegm`) can reuse or move it.
- One documented divergence: `append_with` (`av_bprint_get_buffer`) zeroes the storage it hands
  out, which a caller-supplied buffer shows past the string afterwards.
- Neither smell moved (22 `unsafe fn`, 39 raw-pointer) and the public API gained no `unsafe fn`
  or raw pointer; `unsafe` blocks went from 431 to 474.

The wave tip passes `clippy --all-targets -D warnings` and the plain, `asan-ubsan`, `tsan` and
`bsan` suites: 284 unit, 279 equivalence, 259 `ub_safe` and 59 `ub_unsafe` tests under `bsan`.
From this wave the orchestrator runs `bsan` as one invocation per test binary, in parallel;
`ub_safe` took 30m18s and equivalence 14m22s. The gate passed, so the review got no
`bsan-gate.log`.

### `libavutil/core` review wave 2

From this wave on, per the updated task, every review runs on
`openrouter/anthropic/claude-fable-5.1` (api, `claude`) after every wave, with review caps sized
to the wave. Wave 2 has symbols only, so its review is one batch over the translation batch's
12 items.

- **Landings:** the reviewer ran 20m24s for `$10.25` (13.5M tokens) and landed `cf7e1e63d8`
  (reports) and `91e3426ac9` (fixes).
- **Reports:** 2 `misc`, both documentation. `is-complete-doc-count-only`: `is_complete`'s doc
  implied a fresh count-only buffer is complete, where C's `len < size` is false at `size` 0
  from initialisation. `tm-zone-doc-null-zone`: `TmRef::zone`'s doc said a NULL `tm_zone`
  always names the local zone, where glibc formats nothing when `tm_isdst < 0`. Each fix
  rewords the doc and adds a doctest; behaviour was already equivalent, and both claims match
  C's `av_bprint_is_complete` and the reviewer's glibc harness. Nothing was rejected.
- **Probe:** `probe/` is a differential sweep, not a defect reproducer: random appender
  sequences over every init shape, with `av_max_alloc` switched mid-sequence, compared after
  every step. Rerun on `91e3426ac9`: 20 000 sequences, 0 failures.
- **Observation, not changed:** `BPrint::into_string` and `into_bytes` consume the owner, so when
  `av_bprint_finalize` fails to copy an inline string out (`ENOMEM`, reachable only through
  `av_max_alloc`) the string is lost, where C's caller keeps a usable buffer. Changing it would
  change wave 1's landed API.
- The review moved no smell and no `#[test]` count (its two doctests are outside it), so its
  `: tests` row reads zero. Its `+LoC` reads `+0/-0` because it changed only comments and
  doctests; `rv loc` counts them.

The reviewed tip passes `clippy --all-targets -D warnings` and the plain, `asan-ubsan`, `tsan`
and `bsan` suites, with the same counts as the wave tip. It was promoted to
`crustify/subsystems/libavutil/core`, which completes the sub-campaign, and the
sub-campaign to `crustify/main`.

### `libavutil/core` sub-campaign totals

- The Overview row counts 21 types and 159 symbols (147 in waves 0–1, 12 in wave 2). Its
  `$/type` and `$/sym` divide each kind's translation and review cost by its units: types
  `$149.02` over 21, symbols `$272.75` over 159. `review` excludes the stopped Astra attempt
  (`$3.46`).
- `sub-campaign wall` runs from wave 0's first launch (02:54Z on 2026-10-08) to the reviewed
  wave-2 gate (02:06Z on 2026-10-10), including the interruption before the wave-0 resume
  and the pauses for the user. Agent wall time sums to 25h56m: 16h31m translating and 9h25m
  reviewing.

### `libavutil/formats` wave 0

`formats` is based on `9faf0c31a2`: the campaign tip `20f9b4c307` plus the `channel_layout`,
`csp`, `imgutils`, `pixdesc`, `pixelutils`, `pixfmt` and `samplefmt` header modules. Its rows
follow `core`'s in each `Batches` table; the `subsystem` column, added to every Types and
Symbols table after wave 3, tells them apart.

Wave 0 is 7 type batches (12 enums and `AVComponentDescriptor`) and one batch of 8 symbols and
3 callbacks. All 8 landed between 02:58Z and 03:25Z on 2026-10-10, ending at `17c012afcd`, and
every scheduled item carries its anchor there. Translation cost `$35.09` for 82.9M tokens and
3h02m of agent wall time.

- Every enum is an open `#[repr(transparent)]` newtype that accepts any C value, as `core`'s
  `MediaType` does, because C users write these values into public struct fields. The
  non-ABI `*_NB` enumerators get no constant. `PixelFormat` names all 269 enumerators, the
  in-enum aliases and the 104 native-endian `AV_PIX_FMT_NE` aliases, enumerated by a new
  `crustify_AV_PIX_FMT_NE` shim because bindgen cannot evaluate them.
- `ComponentDescriptor` is an ffibox layout type with getters and setters for its five fields.
- The symbol batch found that on x86 `av_image_copy_plane_uc_from`'s SSE4 path segfaults on
  rows that are not 16-byte aligned, and overruns a last row shorter than the rounded width
  (confirmed with a C harness). For those planes `copy_plane_from_uncacheable` copies the rows
  itself as C's plain path does. The divergence is documented and has regression tests. The
  `TrcFunction`, `EotfFunction` and `SadFn` callback handles get their constructors in wave 1,
  so for now their tests are inline unit tests.
- Smells: the symbol batch added one `unsafe fn` (`check_size_with_context`, whose
  `void *log_ctx` stays a raw seam until logging contexts get a safe handle) and one raw
  pointer, making 23 and 40. No type batch moved either.

The wave tip passes `clippy --all-targets -D warnings` and the plain, `asan-ubsan` and `tsan`
suites, as well as `bsan` for `lib`, `ub_safe` (324) and `ub_unsafe`. The `bsan` equivalence
binary failed: "access an allocation that has been freed" at `av_strtod` (`eval.c:120`). The
orchestrator reproduced it, and found it needs `cpu::` and `imgutils::` to run concurrently.
It passes single-threaded (348/348), for either half alone, and for every other pairing of
`cpu`/`opt`/`log` with the new modules. The log and this triage went to the review as
`review-wave-0/bsan-gate.log`.

### `libavutil/formats` review wave 0

Two `openrouter/anthropic/claude-fable-5.1` reviewers covered the wave: batch 0 the 12 types,
batch 1 the 11 symbols and callbacks. Together they cost `$93.08` for 73.0M tokens and 3h20m.

- **Types reviewer** (44m08s, `$21.19`): landed `5449fae712` (reports) and `71419a9128`
  (fixes). `missing-default-on-none-valued-enums`: `PixelFormat`, `SampleFormat`, `Channel`
  and `MatrixEncoding` lacked the `Default` that the wave's other eight enums carry. Each now
  defaults to `NONE`, the value C gives an unset field, checked against `av_frame_alloc`,
  `av_channel_from_string` and libswresample's option default. A `wavefront` report corrected
  `AVMatrixEncoding`'s recorded encoding (`c_uint`, not `c_int`); as with `core`, the PoC table
  does not count it.
- **Symbols reviewer** (2h35m57s, `$71.89`): landed `6cd65a43ce` (reports) and `ba5d176528`
  (fix). Its one report,
  `misc/bsan-stale-provenance-in-av-strtod-under-rust-log-callback`, triages the gate failure
  as a sanitizer artifact. gdb shows the flagged read is byte 3 of the live 4-byte block that
  `av_expr_parse` owns, through an `endptr` written by uninstrumented glibc `strtod`. The
  report comes and goes with an unrelated libc call in the callback or a debugger stop, and
  every other instrument passes. The trigger is a test that installs a process-wide Rust log
  callback (`equiv/imgutils.rs::check_size_logs_as_c`) while `cpu::parse_caps` logs from
  another thread. The same pattern also showed up in `ub_safe` through `core`'s log tests.
  The fix re-runs the five callback-installing tests in an isolated child process
  (`--exact <name> --test-threads=1`). They still run under BSan. It found no UB or
  equivalence defect in the wrappers, and recorded full Rust coverage of the four homes.
- The orchestrator accepted the artifact verdict on the gdb evidence. There is no runnable
  reproducer, so nothing was rejected or reverted. `equiv/log.rs` and `ub_unsafe/log.rs`
  still install callbacks without that isolation.

The reviewed tip `ba5d176528` passes `clippy --all-targets -D warnings` and the plain,
`asan-ubsan`, `tsan` and `bsan` suites: under `bsan` 395 unit, 351 equivalence, 324 `ub_safe`
and 63 `ub_unsafe` tests. The equivalence run took 13m26s and `ub_safe` 24m26s. It was
promoted to `crustify/subsystems/libavutil/formats`. Wave wall time, from the first
launch (02:47Z) to the reviewed gate (07:07Z), was 4h19m54s.

### `libavutil/formats` wave 1

Wave 1 has 3 type batches (`AVCIExy`, `AVChannelCustom` with `AVLumaCoefficients`, and
`AVPixFmtDescriptor`) and 3 symbol batches (25, 22 and 9 symbols). It is based on `f9fbba11e6`,
the reviewed wave-0 tip plus its records. All 6 batches landed between 07:31Z and 08:16Z,
ending at `0a2fbf9bb7`, and all 60 scheduled items carry their anchors there. Translation cost
`$60.34` for 153.7M tokens and 3h53m of agent wall time.

- `CieXy`, `LumaCoefficients` and `ChannelCustom` are ffibox value types owned through the
  generic `HeapBox`/`HeapVec` policies; no C routine allocates, frees or copies them.
  `PixelFormatDescriptor` has handles but no owner, since C keeps descriptors in a static
  table. Its strings are `&'static CStr`, and `comp[4]` is a `CSlice` run of wave 0's
  `ComponentDescriptor`.
- `imgutils.rs` gained `Planes`/`PlanesMut` (C's `data[4]` and `linesize[4]`), an `Image` owner
  (`av_image_alloc`) and every copy, fill and size query. Inputs on which C would overflow
  `int`, divide by zero (height 0), index past its arrays, or take the faulting SSE4
  uncacheable path are refused or rerouted before C runs. `samplefmt.rs` gained
  `SampleBuffer` owners and plane views. With the default alignment, C's
  `av_samples_get_buffer_size` overflows `FFALIGN` at `nb_samples == INT_MAX - 31`
  (UBSan, `samplefmt.c:137`); the wrappers pass the equivalent alignment 1 for that count.
- Smells went from 23 to 25 `unsafe fn` and from 40 to 54 raw pointers, from
  `check_size_max_pixels_with_context`'s log context, the private C plane-pointer arrays in
  `imgutils.rs` and `samplefmt.rs`, `ChannelCustom`'s `opaque` cookie and
  `PixelFormatDescriptor::from_raw` (per the scan's `--sites`).

The wave tip passes `clippy --all-targets -D warnings` and the plain, `asan-ubsan` and `tsan`
suites, as well as `bsan` for `lib` (501) and `ub_unsafe` (72). The `bsan` equivalence and
`ub_safe` binaries hit the gate's 2 h timeout without printing any report. Timing each
unfinished test alone found four that ran past 25 minutes, peaking at 62 GB RSS
(`samplefmt::buffers::many_planar_channels_spill_the_plane_pointers`), 14 GB, 4.4 GB and
3.8 GB: the `imgutils::images` every-format sweeps and the `samplefmt::buffers` owner tests. A
fifth took 14m27s. The rest of their modules finished in seconds. The triage went to the review
as `review-wave-1/bsan-gate.log`.

### `libavutil/formats` review wave 1

Two `openrouter/anthropic/claude-fable-5.1` reviewers: batch 0 took the 4 types and batch 1
the 56 symbols. Together they cost `$101.41` for 89.5M tokens and 4h21m.

- **Types reviewer** (2h33m50s, `$54.06`): landed `264c66a4b2` (reports) and `d4a373233b`
  (fixes). Three reports. `channel-custom-eq-without-hash`: `ChannelCustom` was the only `Eq`
  value type of the subsystem without `Hash`. `bsan-quadratic-pixdesc-descriptor-equiv-tests`:
  two `pixdesc` equivalence tests walked plane bytes through slice iterators and took 215 s and
  1432 s under BSan; they now take 7 s and 215 s, with the same bytes and assertions.
  `internal/descriptor-unit-tests-assume-literal-address-identity`: two unit tests compared a
  stored pointer with a second occurrence of the same literal, whose address Rust does not
  promise to share; they failed under Miri for seeds 0–5 (before/after logs kept).
- **Symbols reviewer** (1h47m37s, `$47.36`): landed `02894b1296` (reports) and `2610fa70e5`
  (fix). `bsan-gate-per-byte-retags-in-image-alloc-and-sweeps`: `Image::alloc` zeroed its
  `av_malloc` block with `[MaybeUninit<u8>]::fill`, one `&mut` retag per byte, which made
  every image sweep run away under BSan. It now writes the block once with
  `ptr::write_bytes`, and the test helpers write and compare whole slices. Under
  `CRUSTIFY_FFMPEG_VARIANT=bsan` only, the three every-format sweeps keep one format per
  layout class, two sizes and two alignments, and the 4096-channel sample test uses 24
  channels. It added `random_plane_layouts_match_c`, a randomized C-versus-wrapper layout
  sweep. One `conventions` report: a rustfmt-wrapped `unsafe` block that `ffimap`
  counted as undocumented. It found no UB, equivalence or lifecycle defect.
- No report has a runnable reproducer, so nothing was rerun or rejected.

The reviewed tip `d4a373233b` passes `clippy --all-targets -D warnings` and the plain,
`asan-ubsan`, `tsan` and `bsan` suites: under `bsan` 502 unit, 419 equivalence (19m21s), 415
`ub_safe` (29m17s) and 72 `ub_unsafe` tests. It was promoted to
`crustify/subsystems/libavutil/formats`. Wave wall time, from the first launch
(07:10Z) to the reviewed gate (13:49Z), was 6h38m47s, including the 2 h gate timeout and its
triage.

### `libavutil/formats` wave 2

Wave 2 has one type batch (`AVChannelLayout`, `AVPrimaryCoefficients`) and one batch of 13
symbols. It is based on `459b29cb71`, the reviewed wave-1 tip plus its records. Both landed by
14:33Z, ending at `ac34a06476`, and all 15 items carry their anchors there. Translation cost
`$23.18` for 65.7M tokens and 1h17m of agent wall time.

- `ChannelLayout` is owned by value as `ChannelLayoutVal`, released by
  `av_channel_layout_uninit`. Its C initialisers are constructors (`custom`, `from_mask`,
  `parse`, `default_for`), and deep copies go through `av_channel_layout_copy`. It is neither
  `Copy` nor `Clone`, and its setters keep every layout in a state that C's readers and
  `uninit` accept. `PrimaryCoefficients` is a value type shaped like `LumaCoefficients`.
- The symbol batch wrapped the descriptor lookups (`'static` handles into C's table), the
  image line readers and writers, the bit counters, `av_image_fill_max_pixsteps`,
  `av_csp_luma_coeffs_from_avcsp`, and the channel name appenders on `BPrintMut`. The line
  readers and writers recompute the exact bytes C touches and check them against the planes
  before calling C.
- Raw-pointer smells went from 54 to 58, all from the type batch; `unsafe fn` stayed at 25.

The wave tip passes `clippy --all-targets -D warnings` and the plain, `asan-ubsan` and `tsan`
suites, as well as `bsan` `ub_unsafe` (72). The `bsan` `lib` (1 of 539 failed), equivalence and
`ub_safe` runs aborted on "access an allocation that has been freed" inside C's
`av_channel_layout_from_string` (`channel_layout.c:342` and `:417`, the reads through glibc's
`strtol`/`strtoull` end pointers). The failing unit test failed 3/3 when run alone, and a
pure-C program built with the bsan toolchain reproduced the report. Both went to the review as
`review-wave-2/bsan-gate.log`.

### `libavutil/formats` review wave 2

Two `openrouter/anthropic/claude-fable-5.1` reviewers: batch 0 took the 2 types and batch 1
the 13 symbols. Together they cost `$56.63` for 68.9M tokens and 2h08m.

- **Types reviewer** (1h38m46s, `$43.74`): landed `f86b75cbd7` (reports) and `3a92203413`
  (fixes). `bsan-stale-stack-shadow-under-glibc-strtol-end-pointers`: the gate's aborts are a
  sequence-dependent BSan artifact. The end pointers are uninitialised locals that
  uninstrumented glibc writes, and their shadow keeps the freed `av_strdup` block that an
  earlier call left in the same stack slot. Its `probe.c`, rerun by the orchestrator, aborts
  on `"3c"` followed by `"ambisonic 1"` and passes on either alone. The fix skips the affected
  parser inputs under `CRUSTIFY_FFMPEG_VARIANT=bsan` only, through a shared test helper.
  `channel-layout-header-constants-unreachable`: the header's `AV_CH_*`, `AV_CH_LAYOUT_*` and
  `AV_CHANNEL_LAYOUT_*` macros had no Rust spelling, since bindgen cannot evaluate them and
  wavefront schedules no macro. The reviewer added `Channel::mask`, `channel_layout::masks`,
  40 `ChannelLayout` constants and `ChannelLayout::native`, with table shims as the C
  reference.
- **Symbols reviewer** (29m40s, `$12.88`): landed `05e88ed90e` (reports) and `1740c18750`
  (fixes). One `misc` report (two doc omissions in `PixelFormat::descriptor` and the line
  readers), a randomized equivalence test of the line readers and writers against a
  transliteration of C's loops, and a kept differential `probe/` (300k rounds plain and
  300k under ASan/UBSan, no divergence). It found no defect.
- Across `formats` waves 0–2 none of the three BSan gate failures was a wrapper or FFmpeg
  defect: two were BSan false positives on glibc-written end pointers (`av_strtod`,
  `av_channel_layout_from_string`) and one was BSan runaway on per-byte retags. Reviewers
  that investigated them cost `$47`–`$72` each against `$10`–`$21` for the others.
  On 2026-10-10 the user ruled that reviewers no longer run BSan suites, only a candidate
  reproducer; only the orchestrator runs the workspace under BSan.

The reviewed tip `3a92203413` passes `clippy --all-targets -D warnings` and the plain,
`asan-ubsan`, `tsan` and `bsan` suites: under `bsan` 542 unit, 449 equivalence (21m36s), 453
`ub_safe` (35m48s) and 72 `ub_unsafe` tests. It was promoted to
`crustify/subsystems/libavutil/formats`. Wave wall time, from the first launch
(13:50Z) to the reviewed gate (16:53Z), was 3h03m01s.

### `libavutil/formats` wave 3

Wave 3 has one type batch (`AVColorPrimariesDesc`) and two symbol batches (14 and 4
`av_channel_layout_*` symbols). It is based on `cf76bab78e`, the reviewed wave-2 tip plus its
records. It launched at 16:54Z on 2026-10-10. Batches 0 and 2 landed (`0545d10678`,
`3d65561a6e`) before the user stopped the wave. Batch 1 was killed mid-rebase, and its
unlanded commit, worktree, branch and artifact dir were purged. That attempt's cost is not
recorded: it was killed before the harness wrote its usage file. At the user's request batch 1
was re-run from the wave tip and landed `e0a75b5d7f` at 18:13Z. All 19 items carry their
anchors there. The recorded translation cost is `$18.74` for 52.4M tokens and 1h24m of agent
wall time; batch 1's row is the re-run.

- `ColorPrimariesDescriptor` is a value type over `AVColorPrimariesDesc`, with `CieXy` handles
  for the white point and primaries. The symbol batches wrapped the remaining channel-layout
  readers, compare, describe, initialisers, retype, subset and the standard-layout iterator.
- Neither smell moved (25 `unsafe fn`, 58 raw pointers).

From this wave the orchestrator no longer gates waves. At the user's direction the gate moved
to the reviewers (`orchestrator.md` and `translator.md`, uncommitted in crustify): each
reviewer ran the workspace BSan gate on the wave tip and on its landed tree, with only true
positives pursued. Later the same day the user narrowed this to the sub-campaign's last
review wave; other waves run BSan only on individual reproducers.

### `libavutil/formats` review wave 3

Two `openrouter/anthropic/claude-fable-5.1` reviewers: batch 0 took the type and batch 1 the
18 symbols. Together they cost `$73.31` for 48.5M tokens and 3h47m. Neither went into a BSan
investigation: 24 and 21 BSan-related commands, against 46–92 for the investigating reviewers
of waves 0–2.

- **Symbols reviewer** (1h32m19s, `$37.27`): landed `d002d02b8a` (reports and reproducer) and
  `c3ad0048d6` (fixes). `ub_safe/parse-ambisonic-int-max-order-overflow` is the first real
  defect in `formats`. `ChannelLayout::parse("ambisonic 2147483647")` reaches the `order + 1`
  signed `int` overflow in C's `av_channel_layout_from_string` (`channel_layout.c:341`, then
  `:346`), after which C accepts a layout of no channels. Rerun by the orchestrator, the
  reproducer exits 1 on `e0a75b5d7f` with UBSan's `signed integer overflow: 2147483647 + 1`
  and 0 on `c3ad0048d6`, so it is accepted. The fix refuses that order before C, mirroring
  glibc's `strtol` and C's walk of the `+` chain, and returns `EINVAL` as C does for the next
  order down. A `ub_safe` regression covers it. Also one `misc` doc fix
  (`index_by_name`/`channel_by_name`) and one `wavefront` correction recording the hazard.
- **Types reviewer** (2h15m32s, `$36.04`): landed `c03ff99e85` (review record) and
  `aaf8fa4727` (an equivalence test of a descriptor embedded in a C-layout parent). It found
  no defect. Most of its wall time went to its two workspace BSan gates, waited on 18 times,
  which is what moved the gate to sub-campaign end.

The reviewers' BSan gates on the wave tip and on their landed trees passed, as recorded in
their READMEs; the orchestrator ran none. The reviewed tip `aaf8fa4727` was promoted to
`crustify/subsystems/libavutil/formats`. Wave wall time, from the first launch
(16:54Z) to the last review landing (20:29Z), was 3h34m, including the stop and the batch-1
re-run.

### `libavutil/formats` wave 4

Wave 4, the sub-campaign's last, is one batch of the two colorspace primaries lookups. It is
based on `45cde5ae94`, the reviewed wave-3 tip plus its records. It landed `cfa4db1026` at
23:31Z, with both items anchored, for `$4.76` (10.5M tokens, 28m11s).
`ColorPrimaries::descriptor` hands out `'static` handles into C's gamut table.
`ColorPrimariesDescriptorRef::identify` deliberately diverges from C: C reads the unordered
result of a 0/0 coordinate difference as within tolerance, so it identifies, for example, the
all-zero description as BT.709, while the wrapper returns `None`. Neither smell moved.

### `libavutil/formats` review wave 4

One `openrouter/anthropic/claude-fable-5.1` reviewer for the 2 symbols (57m14s, `$12.25`,
9.0M tokens) landed `860917635b` (report) and `804edf7d05` (fix). Its one `misc` report,
`identify-doc-promised-exact-rational-sums`, is about documentation. The doc promised exact
rational sums, but C reduces every term to `int` rationals, so descriptions about 2e-13 inside
the 1/1000 tolerance are judged as C judges them. It also narrowed the divergence to a 0/0
coordinate or two infinite ones. The code already matched C; the fix rewords the doc and adds a
3,000-round C-versus-wrapper regression. Its Python probe is kept with the report. There is no
runnable reproducer to rerun.

As the sub-campaign's last wave, under the updated `translator.md`, this reviewer ran the
workspace BSan gate covering all of `formats`. It was clean on the wave tip before any change
(lib 565, equivalence 468, `ub_safe` 481, `ub_unsafe` 72) and on its landed tree. The wave
forked before the results restructure `81b07a7405` landed on the sub-campaign branch, so the
reviewed tip was promoted with the merge commit `ab73e8a433`, whose Rust tree equals
`804edf7d05`.

### `libavutil/formats` sub-campaign totals

- The Overview row counts 19 types and 100 symbols over 5 waves. `batch` is the translation
  cost of the recorded batches; it excludes wave 3's killed first batch-1 attempt, which wrote
  no usage. `$/type` and `$/sym` divide each kind's translation and review cost by its units:
  types `$214.92` over 19, symbols `$263.86` over 100.
- Review cost 2.4× translation. In waves 0–2 the reviewers that investigated BSan gate
  failures cost `$44`–`$72` each, and none of those failures was a real defect. From wave 3 the
  gate moved to the reviewers, and then to the sub-campaign's last review wave only.
- The one real defect found was UBSan's: the `ChannelLayout::parse` ambisonic `INT_MAX` order
  overflow (review wave 3).
- `sub-campaign wall` runs from the scaffold (02:46Z on 2026-10-10) to the last review landing
  (00:29Z on 2026-10-11), including the stop of wave 3 and the pauses for the user. Agent wall
  time sums to 24h41m: 10h05m translating and 14h35m reviewing.
- At the end the unseeded scan reads 25 `unsafe fn` and 58 raw-pointer smells (22 and 39 at
  `core`'s end), with an unsafe-LoC ratio of 0.134. The union run covers 33.58% of libavutil's
  C and 99.82% of the authored Rust, with 565 unit, 482 `ub_safe`, 72 `ub_unsafe` and 469
  equivalence tests.

### `libavutil/options` wave 0

`options` is based on `33edf63f19`: the campaign tip `9cc23338f5` plus the `dict`, `eval` and
`parseutils` header modules (`opt.rs` already existed). From this sub-campaign on, at the
user's direction (TASK.md, 2026-10-11), every review runs on
`openrouter/anthropic/claude-opus-5.5` instead of Fable 5.1, and orchestrator parallelism is
up to 16; no `options` wave had more than 5 batches.

Wave 0 is 3 type batches (`AVDictionary` with `AVDictionaryEntry`, `AVExpr` with
`AVOptionArrayDef`, `AVOptionRange` with `AVOptionType`) and two symbol batches (24 and 5
symbols). It launched at 01:28Z on 2026-10-11, and all 5 batches landed between 01:52Z and
03:02Z, ending at `cde88fc249`. All 35 scheduled items carry their anchors there. Translation
cost `$70.60` for 201.1M tokens and 4h12m of agent wall time.

- `Dictionary` is owned as `DictionaryBox` (never empty) or `Dict` (C's nullable
  `AVDictionary *` slot, built through `av_dict_set`), released by `av_dict_free` and cloned by
  `av_dict_copy` with `AV_DICT_MULTIKEY`; entries are borrow-only. `Expr` is owned as
  `ExprBox` (`av_expr_free`), built only by `Expr::parse` from `&CStr` names and safe
  `extern "C" fn` callbacks; it is `Send`, not `Sync`, since evaluation writes the root's
  registers. `OptionType` is an open newtype with the `ARRAY` flag; `OptionRange` is only
  borrowed from its list.
- Batch 3 introduced `opt::ObjectRef`/`ObjectMut`, borrowed handles over any
  `AVOptions`-enabled object (adopted through an unsafe `from_ptr` seam until the concrete
  objects are wrapped), and `BareObject`, a safe class-only object for `ub_safe` and log
  contexts. It wrapped 21 `av_opt_*` items on them. The setters and getters refuse, before C
  runs, the values C turns into undefined behaviour: evaluator builtins converting
  out-of-range doubles (`ld(1e300)`), `+`/`-` flag operands outside `int64`, rationals whose
  bounds exceed `int`, scalar stores over array options, and out-of-bounds type-name lookups
  in C's error logs. Beyond C, `free_options` resets binary lengths that C leaves stale and
  later dereferences through NULL.
- Batch 4 wrapped the parsers. `parse_color` refuses a NaN opacity (`red@nan`,
  float-cast-overflow at `parseutils.c:433`) and a prefix ending inside the `#`/`0x` marker
  (an uninitialised stack read). Concurrent local-time `av_parse_time` calls are a TSan false
  positive inside glibc's `tzset`, so the suites serialize them.
- Parse-heavy tests return early under BSan: glibc `strtod`/`strtol` end pointers keep a stale
  shadow slot, so BSan reports a use after free at `eval.c:120`. Batch 1 reproduced this from
  pure C (two parses, the second after an `alloca`).
- Smells went from 25 to 28 `unsafe fn` and from 58 to 60 raw pointers: `set_options_string`
  and `parse_color_with_context` took raw contexts, and `Expr::parse_with_context` a raw
  parent log context.

### `libavutil/options` review wave 0

Two `openrouter/anthropic/claude-opus-5.5` reviewers: batch 0 took the 6 types and batch 1 the
29 symbols. Together they cost `$34.83` for 97.6M tokens and 1h46m.

- **Types reviewer** (32m55s, `$8.34`): landed `6b173399d0` (reports) and `382c43ab9b`
  (fix). `unsafe/expr-parse-with-context-raw-log-context`: batch 3's `ObjectRef` made the raw
  parent log context obsolete, so `Expr::parse_with_context` is now safe over
  `Option<ObjectRef>`. `conventions/expr-send-safety-comment-past-scan-window`: `Expr`'s
  nine-line `SAFETY:` block exceeded `ffimap`'s six-line window. No UB or equivalence
  defect.
- **Symbols reviewer** (73m04s, `$26.49`): landed `dc2706cfb2` (reports and reproducers) and
  `eabd316be0` (fixes). Three UB defects, each in a safe method on an object adopted within
  the seam's contract, each rerun by the orchestrator under `asan-ubsan`:
  - `set-flag-operand-clock-race`: `ObjectMut::set` previews a flag operand to refuse C's
    unchecked conversion, then C evaluates it again, and an operand calling `time()` changes
    value in between. Exits 1 on `cde88fc249` (UBSan `opt.c:508`, `1e+30 is outside the range
    of representable values of type 'long'`) and 0 on `eabd316be0`. Previews now refuse
    expressions that read the clock.
  - `set-array-default-constant-mismatch`: the preview evaluated an array option's `default`
    as 0, but C's `DEFAULT_NUMVAL` reads the definition's address bits. Same UBSan report on
    `cde88fc249`, 0 on `eabd316be0`.
  - `bare-object-foreign-callbacks-read-past-object`: `BareObject::new` accepted classes whose
    `item_name`/`get_category` read their object, so logging read past the 8-byte bare
    object. ASan heap-buffer-overflow on `cde88fc249`, 0 on `eabd316be0`.

  Two `unsafe` reports made `set_options_string` (now `ObjectMut::set_options_string`) and
  `parse_color_with_context` safe, and two `wavefront` reports corrected the log-context and
  option-value records. The rewritten equivalence module found that a failed color parse, like
  a failed image size, copies C's uninitialised stack buffer into the field (seen under TSan
  only), so both are masked after a failing set.
- All three reproducers held, so all are accepted. The review brought the smells back to 25
  `unsafe fn` and 57 raw pointers. The reviewed tip `eabd316be0` was promoted to
  `crustify/subsystems/libavutil/options`. Wave wall time, from the first launch
  (01:28Z) to the last review landing (04:17Z), was 2h48m11s.

### `libavutil/options` wave 1

Wave 1 is two symbol batches (22 and 13 symbols), based on `eabd316be0`. It launched at 04:20Z,
and both batches landed (`a2aace4c3d` at 04:59Z, `c771c3e8bd` at 05:08Z). All 35 items carry
their anchors there. Translation cost `$37.85` for 124.5M tokens and 1h28m of agent wall time.

- `dict.rs` gained the readers and writers (`len`, `iter`, `get`/`get_all`, `serialize`,
  `set`, `remove`, `set_owned` for the `DONT_STRDUP` hand-over, `set_int`, `parse_pairs`,
  `copy_from`). `eval.rs` gained `Expression`, an evaluable parse that keeps the constant
  count C does not (`av_expr_eval` reads `const_values[const_index]` unchecked) and refuses
  the evaluator's unchecked conversions.
- `opt.rs` gained the typed getters and setters, array edits over two sealed traits
  (`ArrayElement` for owned outputs, `ArrayInput` for borrowed inputs) and the dictionary
  appliers. Refused before C runs: any element of a binary array (C writes each length past
  the element's pointer, ASan `opt.c:373`), numbers C converts unchecked into rational or
  video-rate elements, and layouts the option's describers cannot read. `get_video_rate`
  reads the stored rate, where C's `av_opt_get_video_rate` fails with `EINVAL` on every video
  rate option (documented divergence). `parseutils::timegm` mirrors `av_timegm`'s `int` sums
  with checked operations and returns `None` where C overflows.
- Smells went to 37 `unsafe fn` and 67 raw pointers, from the private `ArrayInput`/element C
  sides and `parse_ratio_with_context`'s log context.

### `libavutil/options` review wave 1

One `openrouter/anthropic/claude-opus-5.5` reviewer for the 35 symbols (26m24s, `$13.35`,
45.1M tokens) landed `c84c90c508` (reports and reproducer) and `4ea36e8629` (fix).
`ub_unsafe/remove-array-shrink-failure-double-free`: `av_opt_set_array`'s removal frees and
shifts the elements before its shrinking `av_realloc_array`. Under an `av_max_alloc` limit,
which safe code can set, that call fails with `ENOMEM` and C keeps the old count, so
`av_opt_free` frees a shifted string twice. Rerun by the orchestrator, the reproducer exits 1
on `c771c3e8bd` (ASan `attempting double-free`) and 0 on `4ea36e8629`, so it is accepted.
`remove_array` now writes the new count back after that failure; this is a correction beyond C.
A `misc` report fixed `insert_array`'s doc (C stores a rational into an integer-like element
as `llrint(1/den)*num`), and a `wavefront` report corrected three records. Neither smell
moved. The reviewed tip was promoted; wave wall time (04:20Z to 05:37Z) was 1h16m54s.

### `libavutil/options` wave 2

Wave 2, the sub-campaign's last, is one batch of 14 `opt.c` symbols, based on `3019565384`
(the reviewed wave-1 tip plus its records). It launched at 05:40Z and landed `abf526dc98` at
06:20Z, with all 14 items anchored, for `$13.57` (41.4M tokens, 40m30s). It wrapped the option
lookups (`find`, `find_with_target`, and `ClassRef::find` through C's fake object), the option
walk, the per-option default check (which refuses an option outside the object's own table,
whose offset C would read out of bounds) and the `av_opt_eval_*` evaluators in an
`EvalContext`. The safe `option_ranges` queries on `ObjectRef` and `ClassRef` replaced wave 0's
unsafe raw-object `OptionRanges::query`, so the smells fell to 35 `unsafe fn` and 65 raw
pointers.

### `libavutil/options` review wave 2

One `openrouter/anthropic/claude-opus-5.5` reviewer for the 14 symbols (81m25s, `$15.34`,
39.1M tokens) landed `ad02fe43af` (reports and reproducer) and `2a848962bc` (fixes).
`ub_safe/eval-flags-token-split-hides-evaluator-call` is reachable from safe code: C cuts a
flags value into tokens of at most 255 bytes, and the refusal checked the whole value, so
`0*` + 253 zeros + `ld(1e300)` reached `ld`'s unchecked `(int)1e300` through `eval_flags` on
C's `cpuflags` class. Rerun by the orchestrator, the reproducer exits 1 on `abf526dc98` (UBSan
`eval.c:206`) and 0 on `2a848962bc`, so it is accepted. The fix checks every token, for
`eval_flags` and for the setters that share the refusal. A `wavefront` report corrected
`av_opt_eval_flags`' record.

As the sub-campaign's last wave, this reviewer ran the workspace BSan gate covering all of
`options`. On the wave tip every binary stopped at its first report within 2–91 s. Each test
was then run alone (1882 processes): 44 reported, all reads through glibc `strtod`/`strtol`/
`strtoul` end pointers (`eval.c:120`, `:429`, `:581`, `parseutils.c:166`, `:391`), the known
false-positive class. The two sites not listed before reproduce from pure C
(`misc/bsan-gate-strto-end-pointer-false-positives`). On the landed tree, with those tests
skipped, equivalence (525), `ub_safe` (551), `ub_unsafe` (95) and the lib binary
single-threaded passed. One libc-only unit sweep that timed out at 3000 s now sweeps three
bytes instead of four under BSan. The reviewed tip was promoted; wave wall time (05:40Z to
07:43Z) was 2h02m50s.

### `libavutil/options` sub-campaign totals

- The Overview row counts 6 types and 78 symbols over 3 waves. `$/type` and `$/sym` divide
  each kind's translation and review cost by its units: types `$36.91` over 6, symbols
  `$148.62` over 78.
- Review cost 0.52× translation (`$63.52` against `$122.01`), against 2.4× in `formats`. The
  workspace BSan gate ran once, in the last review wave.
- Five reproducers held: four in the option object API (`set` previews and the clock,
  array defaults, foreign class callbacks on `BareObject`, the array shrink failure) and one
  reachable from safe code (the flags token cut). All are UB or memory errors in C reached
  through wrappers whose refusals did not match what C evaluates.
- `sub-campaign wall` runs from the scaffold (01:27Z on 2026-10-11) to the last review landing
  (07:43Z), with no pauses. Agent wall time sums to 9h53m: 6h20m translating and 3h33m
  reviewing.
- At the end the unseeded scan reads 35 `unsafe fn` and 65 raw-pointer smells (25 and 58 at
  `formats`' end), with an unsafe-LoC ratio of 0.129. Most of the new ones are the documented
  `ObjectRef`/`ObjectMut::from_ptr` adoption seams and private C sides of the array traits.
  The union run covers 47.99% of libavutil's C and 99.84% of the authored Rust, with 678
  unit, 559 `ub_safe`, 107 `ub_unsafe` and 544 equivalence tests.

### `libavutil/crypto` wave 0

`crypto` is based on `26c563ecc2`: the campaign tip `6daed536cb` plus 22 header modules. At the
user's direction (2026-10-11) its translators ran on `openrouter/openai/gpt-6.1-sol` via
`codex` (api billing); reviews stayed on `openrouter/anthropic/claude-opus-5.5`.

Wave 0 is 10 type batches (19 types: the cipher, hash and checksum contexts, `AVCRCId` and
`AVHMACType`) and one batch of 24 symbols (checksums, base64, CRC tables, LZO, MD5 sum, UUIDs,
hash names and the nine context-size globals). It launched at 07:53Z on 2026-10-11, and all 11
batches landed between 08:03Z and 08:48Z, ending at `c45fd5b3d1`. All 43 scheduled items carry
their anchors there. Translation cost `$37.44` for 135.4M tokens and 5h41m of agent wall time:
`$3.40` per batch, against `$14.12` per batch in `options` wave 0 on Opus 5.5.

- Most contexts are opaque C layouts owned through ffibox owners released by their `av_free`
  family (`DesBox`, `Md5Box`, ...), and `AVCRCId`/`AVHMACType` are open newtypes. The symbol
  batch kept base64, LZO and UUID buffers in checked slices.
- The translators committed their own notes (26 files: `validation.md`, coverage and scan
  dumps, `findings/*.json`) into the orchestrator's batch record dirs, two of them at its
  `ffimap.json` path. They were moved to `batch-<k>/translator-notes/` when the wave was
  recorded. Wave 1's translators then followed that layout, plus one stray
  `wave-1/batch-2-validation.md`, moved the same way.
- Codex echoes its shell commands into `spawn.out`, where the orchestrator's watcher took any
  line starting with `exit ` as the agent's exit. It handled batches 1, 3 and 8 early and
  scanned batch 8 at a pre-landing commit; nothing was purged before landing, and those
  batches were purged and rescanned at their landing commits.
- Smells went from 35 to 36 `unsafe fn` and from 65 to 66 raw pointers.

### `libavutil/crypto` review wave 0

Two `openrouter/anthropic/claude-opus-5.5` reviewers: batch 0 took the 19 types and batch 1
the 24 symbols. Together they cost `$19.83` for 58.2M tokens and 1h09m.

- **Types reviewer** (42m30s, `$14.00`): landed `82f93237f0` (reports) and `99811e0ab3`
  (fixes). `lifecycle/avdes`: `AVDES` had no clone strategy; `Des` is now `Clone`/`Copy` and
  `DesBox` clones through `av_memdup`. `unsafe/opaque-contexts-offer-rust-sized-allocation`:
  the SHA, SHA-512, Camellia, TEA and Twofish owners used the `Heap` policy over a zero-sized
  forward-declared layout, which exposed a safe Rust-sized `new_uninit()`; they now use
  storage-only release policies, pinned by `compile_fail` doctests.
  `unsafe/hash-context-free-raw-adapter`: `HashContextFree` implements `CDrop` directly.
  Two `conventions` reports (a wrapped `SAFETY` block, `clippy --all-targets` failures in the
  SHA equivalence tests), one `wavefront` correction (`av_hash_alloc` as the constructor) and
  three `misc` reports (two drifted trybuild snapshots, the translator notes above).
- **Symbols reviewer** (26m34s, `$5.82`): landed `c3a3e29251` (reports and reproducer) and
  `a4a081f276` (fix). `equiv/base64-rejects-outputs-beyond-int-max`: `base64::encode` and
  `decode` refused any output buffer longer than `INT_MAX` bytes with `EINVAL`, where C, given
  `INT_MAX`, succeeds. Rerun by the orchestrator, the reproducer exits 1 on `c45fd5b3d1`
  (`Rust Err(-22): DIFFER` for both) and 0 on `99811e0ab3`, so it is accepted. Three
  `conventions` reports (wrappers placed after the `unit_tests` module, which fails
  `clippy::items_after_test_module`; UUID `copy`/`equal`/`nil` anchored `Wraps` though they
  never call C; equivalence-test `SAFETY` comments) and one `wavefront` correction.
- No UB was found. The reviewed tip `99811e0ab3` was promoted. Wave wall time, from the first
  launch (07:53Z) to the last review landing (09:33Z), was 1h40m33s.

### `libavutil/crypto` wave 1

Wave 1, the sub-campaign's last, is 4 symbol batches (25, 25, 25 and 4 symbols: block
processing, CTR IVs, streaming hashes, HMAC and the cipher `crypt` routines), based on
`10c0222a0d`. It launched at 09:39Z, and all 4 batches landed between 09:47Z and 10:02Z, ending
at `0896e41211`. All 79 items carry their anchors there. Translation cost `$8.70` for 35.9M
tokens and 1h11m of agent wall time (7–26 minutes per agent). Neither smell moved, and no
`ub_unsafe` test was added: the wave adds no public unsafe API.

### `libavutil/crypto` review wave 1

One `openrouter/anthropic/claude-opus-5.5` reviewer for the 79 symbols (2h11m00s, `$18.39`,
50.0M tokens) landed `f60d669d34` (reports and reproducer) and `75d235c777` (fixes).

- `equiv/hash-and-hmac-reject-outputs-beyond-c-ranges`: the hash `finalize_bin`/`hex`/`b64`
  wrappers refused outputs over `INT_MAX` and HMAC `finalize`/`calculate` over `UINT_MAX`,
  where C succeeds, the same shape as wave 0's base64 finding. Rerun by the orchestrator, the
  reproducer exits 1 on `0896e41211` (5 cases `DIFFER`) and 0 on `75d235c777` (all agree), so
  it is accepted.
- `internal/xtea-in-place-source-pointer-taken-before-exclusive-retag`: XTEA's in-place path
  derived C's source pointer from a shared reborrow before the exclusive one. That is UB under
  Stacked Borrows (shown on a standalone Miri model), accepted under Tree Borrows, and clean
  under BSan. The path now derives one pointer and passes it twice, as the wave's other
  in-place wrappers do. There is no reproducer against the crate to rerun.
- Two `conventions` reports: DES, TEA, Twofish, XTEA and RC4 now share `aes::Direction` and
  `aes::CryptError` instead of `bool`/`-22` and duplicate types, and CAST5's ECB `SAFETY`
  comments described CBC. One `wavefront` correction (six output-size records).

As the sub-campaign's last wave, this reviewer ran the workspace BSan gate covering all of
`crypto`, skipping the 48 known `strto*` false positives from `options`. On the wave tip `lib`
(752), `ub_safe` (642), `ub_unsafe` (95) and the seven `-sys` binaries passed; `equiv` stopped
at a true positive in test code: the raw C references reused one IV pointer across a
reborrow (`misc/bsan-gate-equiv-iv-pointer-reused-across-reborrow`). Fixed in the AES,
Blowfish, Camellia and CAST5 equivalence tests, after which every binary passed on the landed
tree (`equiv` 591, `ub_safe` 642, `lib` 753, `ub_unsafe` 95). The reviewed tip was promoted;
wave wall time (09:39Z to 12:18Z) was 2h38m56s.

### `libavutil/crypto` sub-campaign totals

- The Overview row counts 19 types and 103 symbols over 2 waves. `$/type` and `$/sym` divide
  each kind's translation and review cost by its units: types `$47.05` over 19, symbols
  `$37.30` over 103.
- Translation on `gpt-6.1-sol` cost `$46.14` for 122 items (`$0.38` per item); `options` on
  Opus 5.5 cost `$122.01` for 84 (`$1.45`). Review cost 0.83× translation (`$38.22`).
- The reviewers found no UB in the translated wrappers. They found two behavioural
  divergences of one shape (base64 and hash/HMAC refusing output buffers C accepts), one
  lifecycle gap (`AVDES` clone), one Rust-sized allocator over zero-sized layouts, one
  aliasing-model hazard (XTEA), and more convention defects than earlier waves:
  `clippy --all-targets` failures, wrappers after the test module, divergent error and
  direction vocabularies, and notes committed into the orchestrator's record dirs.
- `sub-campaign wall` runs from the scaffold (07:52Z on 2026-10-11) to the last review landing
  (12:18Z), with no pauses. Agent wall time sums to 10h12m: 6h52m translating and 3h20m
  reviewing.
- At the end the unseeded scan reads 35 `unsafe fn` and 65 raw-pointer smells, the same as at
  `options`' end, with an unsafe-LoC ratio of 0.150 (0.129 at `options`' end). The union run
  covers 65.09% of libavutil's C and 99.74% of the authored Rust, with 763 unit, 650
  `ub_safe`, 107 `ub_unsafe` and 610 equivalence tests.

### `libavutil/buffer-frame` wave 0

`buffer-frame` is based on `200821da7c`: the campaign tip `d6b6eab6e2` plus the `buffer`, `frame` and
`refstruct` header modules. Translators and reviewers ran on `openrouter/anthropic/claude-opus-5.5`
(api billing). The campaign branch was merged into the sub-campaign twice (`ba0b233ccf`, `98495065e5`)
to carry the campaign-wide scan and results structure commits, so its promotion stays a fast-forward.

Wave 0 is 5 type batches (`AVBuffer`, `AVBufferPool`, `AVRefStructOpaque`, `AVRefStructPool`,
`AVSideDataDescriptor`, `AVSideDataProps`, `AVFrameSideDataType`, `AVActiveFormatDescription`) and one
batch of 7 symbols (the `av_refstruct_*` reference routines and `av_buffer_default_free`). It launched
at 16:19Z on 2026-10-11, and all 6 batches landed by 17:13Z, ending at `adfecd6c81`, with all 15
items anchored. Translation cost `$38.93` for 106.5M tokens and 3h38m of agent wall time. Smells went
from 35 to 36 `unsafe fn` and from 65 to 71 raw pointers.

### `libavutil/buffer-frame` review wave 0

Two `openrouter/anthropic/claude-opus-5.5` reviewers (8 types, 7 symbols) cost `$13.10` for 35.9M tokens.

- **Symbols reviewer** (18m00s, `$5.52`): landed `867ce88ed7` and `edd9025228`.
  `unsafe/refstruct-count-wrap-by-leaked-references`: C increments RefStruct's `uintptr_t` count
  unchecked, and safe code can leak references with `mem::forget`, so on targets narrower than 64 bits
  a loop of leaked clones wraps it and the next release frees a live object. Every counted up-ref now
  gates on `COUNT_IS_WIDE`. `ub_unsafe/ffibox-crefclone-contract-allows-count-wrap`: ffibox's
  `CRefClone` contract never forbade a wrapping count. Its pure-Rust reproducer, rerun by the
  orchestrator, fails under Miri with a use after free after 255 leaked clones of an 8-bit count; the
  contract fix was merged into ffibox `main` as `630f4a5`.
- **Types reviewer** (30m49s, `$7.57`): landed `d171e9db69` and `13a178b7db`.
  `unsafe/buffer-pool-allocator-free-callback-under-pool-lock`: `BufferPool::with_allocator`'s
  `# Safety` let a buffer's free callback call into its own pool, which `av_buffer_pool_uninit` runs
  under the pool mutex, so it self-deadlocked. The reproducer, rerun by the orchestrator, exits 1
  through its watchdog on `adfecd6c81`; it is a contract fix, so it still deadlocks afterwards by
  design. Also one `wavefront` correction and one `conventions` fix.
- Wave wall time (16:19Z to 17:46Z) was 1h26m51s.

### `libavutil/buffer-frame` wave 1

One type batch (`AVBufferRef`, `AVRegionOfInterest`) and one batch of 10 symbols (RefStruct allocators
and pools, `av_buffer_pool_uninit`, side data name and descriptor lookups), based on `f4f372d373`.
Both landed between 18:29Z and 18:45Z, all 12 items anchored, for `$23.55` (66.2M tokens, 1h25m).
`AVBufferRef` is `BufferReferenceBox`; its `try_clone` refuses once `av_buffer_get_ref_count` reaches
a limit, so `AVBuffer`'s unchecked `atomic_uint` cannot wrap either. Allocations that hand out
uninitialised bytes return an `UninitBuffer`. `unsafe fn` went to 37.

### `libavutil/buffer-frame` review wave 1

Two reviewers (2 types, 10 symbols) cost `$15.35` for 41.3M tokens and found no code defect. The types
reviewer documented `RegionOfInterest`'s `self_size` hazards: every C reader checks only that it is
non-zero and divides the payload, so an undersized one reads past it (ASan in a C probe) and
`vf_addroi` aborts on 0. No safe path hands a region payload to C yet. The symbols reviewer corrected
the typed pools' zeroing argument and a `SAFETY` comment past the scanner's look-back. Wave wall time
(17:55Z to 19:25Z) was 1h29m25s.

### `libavutil/buffer-frame` wave 2

One type batch (`AVFrameSideData`) and one batch of 15 `av_buffer_*` symbols, based on `98495065e5`.
Both landed by 20:11Z, all 16 items anchored, for `$19.64` (53.1M tokens, 1h11m). Raw pointers went
from 71 to 78.

### `libavutil/buffer-frame` review wave 2

Two reviewers (1 type, 15 symbols) cost `$17.10` for 51.1M tokens.
`ub_unsafe/pool-buffer-resized-in-place-under-its-entry`: a pool built with
`BufferPool::with_allocator(n, alloc_resizable)` hands out buffers C keeps
`BUFFER_FLAG_REALLOCATABLE`, so the safe resize let `av_buffer_realloc` move one under its pool entry
and the pool recycled the freed block. Rerun by the orchestrator, the reproducer exits 1 on
`83bd5f2e64` (ASan heap-use-after-free) and 0 on `b3cfd5dbd2`, so it is accepted; buffers with an
opaque now resize by copy. `misc/uninit-view-over-initialised-rust-owner`: `UninitBuffer::try_from`
exposed an owner's initialised bytes as uninitialised; it now refuses buffers with an opaque, and
`UninitBuffer::from_owner` covers pool allocators. The types reviewer submitted `AVFrameSideData`'s
missing lifecycle roles. Wave wall time (19:33Z to 20:43Z) was 1h09m52s.

### `libavutil/buffer-frame` wave 3

One type batch (`AVFrame`, 40 fields) and one batch of 8 `av_frame_side_data_*` symbols, based on
`6224fd5a23`. Both landed by 21:53Z, all 9 items anchored, for `$25.35` (75.0M tokens, 1h32m). The
largest smell move of the sub-campaign: 37 to 42 `unsafe fn` and 78 to 95 raw pointers, mostly the
`AVFrame` batch's public `set_data_ptr` / `set_extended_data_ptr` / `replace_hw_frames_context` and
private layout projections.

### `libavutil/buffer-frame` review wave 3

Two reviewers (1 type, 8 symbols) cost `$17.08` for 50.4M tokens. The types reviewer judged the three
new public `unsafe fn` legitimate (lent planes on a `'static` owner cannot carry a lifetime; the third
waits on hwcontext) and found two of their contracts breakable. All three reproducers were rerun by
the orchestrator and accepted:

- `ub_unsafe/clone-reads-planar-channels-past-data`: planar audio with more than 8 lent channels and
  `extended_data` still `data` made `try_clone`'s `av_frame_copy` read past `data[8]` (ASan SEGV at
  `0x40` on `4b205d72ec`, exit 0 on `82371ac8e1`). `try_clone` now refuses such frames.
- `ub_unsafe/extended-data-keeps-a-replaced-external-plane`: `alloc_extended_data` copied lent planes,
  so a replaced and freed plane stayed and a clone read it (heap-use-after-free on `4b205d72ec`, exit 0
  on `82371ac8e1`).
- `ub_safe/side-data-clone-replace-leaks-partial-metadata`: `add_clone` with REPLACE leaked C's partial
  metadata copy when `av_dict_copy` failed after its first entry (LSan, 36 bytes, on `4f959b26a4`; exit
  0 on `e0117458b2`). Rust now runs that path itself and frees the partial copy.

`replace_hw_frames_context`'s contract was also tightened, and a private raw-pointer return removed
(94 raw pointers after review). Wave wall time (20:52Z to 22:31Z) was 1h39m00s.

### `libavutil/buffer-frame` wave 4

Wave 4, the sub-campaign's last, is one batch of 18 `av_frame_*` routines, based on `6bc1e235fc`. It
landed `2453c27798` at 23:43Z, all 18 items anchored, for `$22.57` (72.0M tokens, 1h03m). Neither
smell moved.

### `libavutil/buffer-frame` review wave 4

One reviewer for the 18 symbols (3h31m34s, `$26.77`, 48.3M tokens; most of the wall time was the BSan
gate) landed `e9fad85f29` and `686a30f88a`.
`ub_safe/audio-copy-sample-count-rounding-overflow`: `alloc_buffers` guarded C's
`FFALIGN(nb_samples, 32)` overflow at `INT_MAX - 31` samples, but `make_writable`, `try_clone` and
`try_clone_from` reached it through the `av_frame_get_buffer` call inside `av_frame_ref` and the
wrapper's copy helper. Rerun by the orchestrator, the reproducer exits 1 on `2453c27798` (UBSan
`samplefmt.c:137`, `2147483616 + 32 cannot be represented in type 'int'`) and 0 on `686a30f88a`, so it
is accepted. One `misc` doc fix and four `wavefront` corrections.

As the sub-campaign's last wave, this reviewer ran the workspace BSan gate covering all of
`buffer-frame`, with the 48 known `strto*` skips. No BSan report on either tree. On the wave tip
`equiv` reached its two-hour limit with 655 of 656 tests passed: the one still running was wave 2's
`buffer::mutation::sequences_mutate_count_and_release_as_c`, about 13 minutes per seed under BSan over
16 seeds (`misc/bsan-gate-equiv-buffer-mutation-sequences-outrun-the-limit`); it now runs one seed
under BSan. On the landed tree every binary passed (`equiv` 656 in 4127 s, `ub_safe` in 3426 s).
Wave wall time (22:40Z to 03:16Z on 2026-10-12) was 4h36m16s.

### `libavutil/buffer-frame` sub-campaign totals

- The Overview row counts 12 types and 58 symbols over 5 waves. `$/type` and `$/sym` divide each
  kind's translation and review cost by its units: types `$103.02` over 12, symbols `$116.42` over 58.
- Review cost 0.69× translation (`$89.40` against `$130.04`).
- Eight reproducers held, every one in the ownership and lifetime layer this subsystem is: a
  refcount wrap through leaked clones (and the same gap in ffibox's contract), a pool callback
  deadlock, a pool buffer resized under its entry, two plane-array reads past or after their memory,
  a metadata leak, and a sample-count `int` overflow. Two are reachable from safe code (the metadata
  leak and the overflow).
- `sub-campaign wall` runs from the scaffold (16:18Z on 2026-10-11) to the last review landing
  (03:16Z on 2026-10-12), 10h58m03s, with no pauses. Agent wall time sums to 16h10m: 8h51m
  translating and 7h19m reviewing.
- At the end the unseeded scan reads 42 `unsafe fn` and 94 raw-pointer smells (35 and 65 at
  `crypto`'s end), with an unsafe-LoC ratio of 0.157. The union run covers 72.39% of libavutil's C and
  99.69% of the authored Rust, with 884 unit, 790 `ub_safe`, 141 `ub_unsafe` and 675 equivalence
  tests.
- After the review, at the user's request, the orchestrator moved `refstruct.rs`'s four typed-pool and
  release trampolines off their one-element-slice workaround (`CSliceMut::from_raw_parts(p, 1)`) onto
  `T::mut_from_void_ptr` / `T::mut_from_ptr`, the generic handle constructors ffibox `5a98173` gave
  `CCell` (`b36c196b9b`). It was validated with clippy, the whole plain suite, and the refstruct tests
  under ASan/UBSan, TSan and BSan; no smell moved. The sub-campaign and campaign-wide scans were taken
  on that tree.

### `libavutil/hwcontext` wave 0

`hwcontext` is based on `23002c771a`: the campaign tip `e7a860fd68` plus the `hwcontext` header module.
Translators and reviewers ran on `openrouter/anthropic/claude-opus-5.5`. Every campaign build is
configured with `--disable-autodetect` and no hardware backend, so FFmpeg's backend table is empty:
`av_hwdevice_ctx_alloc` and `create` fail for every device type and no real device or frames context
exists. The translators therefore built three fake backends in `tests/common/` that mirror the C
backend structs, so the lifecycle, map, derive, transfer and constraints paths run through C.

Wave 0 is one type batch (`AVHWDeviceType`, `AVHWFrameTransferDirection`). It landed `b681e8e946` at
03:49Z on 2026-10-12, both items anchored, for `$2.84` (6.6M tokens, 12m09s). Its reviewer (`$2.28`,
9m50s) corrected `DeviceType`'s docs: `AVHWDeviceContext.type` is read-only after allocation (the
backends cast `hwctx` by it), so the context gets no type setter. No code defect. Wave wall time was
24m00s.

### `libavutil/hwcontext` wave 1

One type batch (`AVHWDeviceContext`, `AVHWFramesConstraints`) and two symbol batches (14 device and
frames-context routines; the transfer pair), based on `6974541d1e`. All 3 landed between 04:44Z and
05:29Z, all 18 items anchored, for `$50.94` (156.2M tokens, 3h05m); the type batch alone took
`$25.46`. The type batch and a symbol batch both designed owners for the same C object, and the
later landing kept one. `unsafe fn` went from 42 to 47.

### `libavutil/hwcontext` review wave 1

Two reviewers (2 types, 16 symbols) cost `$19.86` for 61.1M tokens and found no UB or equivalence
defect. The symbols reviewer made the hardware frame transfers and the transfer format listing safe,
since they had stayed `unsafe` waiting for a frames context wrapper a sibling batch landed in the same
wave (`unsafe fn` 46). It also completed `UninitFrames::as_mut_ptr`'s contract. The types reviewer
fixed the device context's free-callback refusal: several backends set `free` without `user_opaque`.
Wave wall time (04:05Z to 06:06Z) was 2h01m31s.

### `libavutil/hwcontext` wave 2

Wave 2, the sub-campaign's last, is one type batch (`AVHWFramesContext`) and one batch of 2 symbols,
based on `ec0ec293a8`. The symbol batch found both symbols already bound by wave 1's type batch as
`FramesConstraints`' constructor and release, and recorded that (`45c1736871`, `$1.88`). The type batch
landed `ad5932bb53` at 06:52Z for `$11.06` (33.3M tokens, 41m54s). `unsafe fn` went to 50.

### `libavutil/hwcontext` review wave 2

Two reviewers cost `$28.95` for 66.9M tokens. With two last-wave reviewers, only the types reviewer
ran the workspace BSan gate; the symbols reviewer ran BSan only on its own test (the orchestrator's
reading of the gate policy, to avoid two concurrent gates).

- **Types reviewer** (1h56m00s, `$24.69`): landed `47c3eea0f4` and `2a1f7d6d5a`.
  `unsafe/frame-hw-frames-context-seam-left-untyped`: `frame.rs` had kept `AVFrame.hw_frames_ctx` as a
  byte-view getter and an untyped unsafe setter until this type existed, and that setter's contract let
  the safe `FrameRef::try_clone` of a frame without `buf[0]` reach C's unchecked `transfer_data_from`
  call through the context's backend. Rerun by the orchestrator, the reproducer exits 1 on `ad5932bb53`
  (ASan SEGV in `libavutil.so`); on the fixed tree it no longer compiles, because the setter now takes a
  `FramesReferenceBox` and is safe, and the same scenario through the safe API is a unit test that dies
  with SIGSEGV without the new refusal. Accepted. Copies of frames holding a frames context are refused
  (`ENOSYS`). `unsafe fn` 49.
- **Symbols reviewer** (15m42s, `$4.26`): no defect; added a 12-step constraints query equivalence test.

The BSan gate passed on both trees after thinning wave 1's `constraints_keep_what_they_have_on_enomem`
(a 64 Ki-element list ran away to 40 GB; 512 elements exercise the same `ENOMEM` path) and skipping one
more instance of the documented `strtol` end-pointer false positive
(`misc/bsan-gate-lib-constraints-enomem-runaway`). Wave wall time (06:12Z to 08:52Z) was 2h39m56s.

### `libavutil/hwcontext` sub-campaign totals

- The Overview row counts 5 types and 18 symbols over 3 waves. `$/type` and `$/sym` divide each kind's
  translation and review cost by its units: types `$73.21` over 5, symbols `$44.58` over 18. The high
  `$/type` is `AVHWDeviceContext` and `AVHWFramesContext`, whose tests had to build fake backends.
- Review cost 0.77× translation (`$51.09` against `$66.73`).
- One reproducer held, reachable from safe code: the frame copy through an untyped hardware frames
  context, which also closed the last seam `buffer-frame` left open.
- `sub-campaign wall` runs from the scaffold (03:36Z on 2026-10-12) to the last review landing (08:52Z),
  5h15m53s, with no pauses. Agent wall time sums to 7h30m: 4h11m translating and 3h18m reviewing.
- At the end the unseeded scan reads 49 `unsafe fn` and 94 raw-pointer smells (42 and 94 at
  `buffer-frame`'s end), with an unsafe-LoC ratio of 0.162. The union run covers 75.12% of libavutil's C
  and 99.69% of the authored Rust, with 968 unit, 809 `ub_safe`, 181 `ub_unsafe` and 718 equivalence
  tests.

### Rust coverage recomputed without test helpers

The coverage measurement filtered test files by a literal `tests/` path prefix. The library's unit
tests include four `tests/common` helpers through `#[cfg(test)] #[path = "../tests/common/.."]`:
`opt.rs` since `ab1d2518bf` (`core`, 2026-10-08), and the fake hardware backends `hwdevice.rs`,
`hwframes.rs` and `hwbackend.rs` since `hwcontext` wave 1. They reached the profile as
`libavutil/../tests/common/..`, so their lines counted as library code: 110 lines at first, 293 at
`buffer-frame`'s end, 1049 at `hwcontext`'s end. Every recorded Rust figure was therefore
understated, by 1.3–2 points late in `libavutil` and by up to 12.9 points in `core`'s first waves,
when the library was 650 lines.

The measurement now normalises paths first. On 2026-10-12 the 183 affected landing and base
commits were measured again with it (C coverage and test counts reproduced to within 0.024 points),
and every Rust figure in the `Batches` tables was rewritten from those runs: the Rust half of each
`cov (C/Rust)` cell, the Rust deltas in the `: tests` tables and their Σ cells, which moved by the
sum of their rows' changes, and the sub-campaign totals above. Each row was first mapped to its
landing commit and reproduced exactly from the old measurements, and no C figure, count or other
cell changed. `core`'s first 9 measured commits predate the include and were not affected.

### Orchestrator cost

The Overview's orchestrator row sums the 10 `orchestrator-*.usage.json` records in
`crustify/artifacts/orchestrator/`: `$500.92` for `772.74M` tokens over `181.9h` of session wall, all
`openrouter/anthropic/claude-opus-5.5`. Three of them were recovered from the session transcripts
under the container's `/work/.claude/projects/-target/`, each marked with `recovered_from`:

- `orchestrator-20261011-011120`, session `7997f6c9`: the running orchestrator, `$197.87` and
  `320.33M` through its last request at `2026-10-12T21:45:51Z`. The container exited on
  `2026-10-13` before the session ended, so it never wrote its record.
- `orchestrator-20261008-170823` (`550bb3c6`, `$0.75`) and `orchestrator-20261010-000824`
  (`003e77eb`, `$0.27`): launches that left a transcript but no log, each minutes before the
  relaunch that has one.

Every orchestrator log in the campaign dir now has a record, so the row carries no `+`. It
excludes the CLI's auxiliary requests, which the transcript does not record.

