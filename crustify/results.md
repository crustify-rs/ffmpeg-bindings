# crustify — `ffmpeg / public API`

Copy this template into the campaign checkout, then fill it as work lands.

## Campaign

- **target repo** — `ffmpeg` @ `1019f8f036602a8464185baa4857654337eeca14`
- **target** — `whole public API of libavutil, libswresample, libswscale, libavcodec, libavformat, libavfilter, libavdevice, excluding deprecated items`
- **campaign objective** — `wrap`
- **`impl_files`** — `3181 files: the built TUs and reachable private headers of the 7 libraries (crustify/wavefront/configs/wavefront-config.json)`
- **`api_headers`** — `122 installed headers, excluding the platform/vendor hwcontext_*.h backends, libavcodec/{d3d11va,dxva2,jni,mediacodec,qsv,vdpau,videotoolbox}.h and the generated avconfig.h/ffversion.h`
- **agent backend** — `claude`
- **model** — `openrouter/anthropic/claude-opus-5.5`
- **`--billing`** — `api`
- **`--max-types`** — `2`
- **`--max-syms`** — `25`
- **`--max-loc`** — `500`
- **`--min-fields`** — `20`
- **orchestrator parallelism** — `10` concurrent batch processes
- **branch** — `crustify/campaigns/20261005`, tip `<sha>`
- **deps** — crustify `2a4a613` (`main`, plus uncommitted prompt edits), ffibox `196c5fb` (`main`)
- **build version** — `3`
- **C test baseline** — `5437/5437, 0 skipped, and no disabled tests`

## Review pass

Thin batches with `"objective": "review"`, LLM-as-a-Judge over each landed wave.

- **agent backend** — `claude`
- **model** — `openrouter/anthropic/claude-opus-5.5`
- **`--billing`** — `api`
- **`--max-types`** — `2` (wave 0); the wave's types from wave 1
- **`--max-syms`** — `25` (wave 0); the wave's symbols from wave 1
- **`--max-loc`** — `500` (wave 0); unbounded from wave 1
- **`--min-fields`** — `20`
- **orchestrator parallelism** — `10` concurrent batch processes
- **branch** — `crustify/reviews/20261005/libavutil/core/wave-1`, tip `532c537e67`
- **agents** — `20` (one stopped by the provider), over `4` wave(s)

`rv`-prefixed columns below carry the review pass; the unprefixed ones remain
the campaign's.

## Legend

- `objective` — what the batch's agents were told to do: `wrap`, `port`, or
  `raw lifetime`. The Types tables are split by it, so it appears as a column
  only in the Symbols `Batches` table, which mixes the two
- `wave` / `batch` — the batch's position in its sub-campaign's plan: the index
  of its wave in `waves` and of the batch within that wave's `batches`, so a
  batch row joins to `schedule.json`, to the agent that landed it, and to its
  Overview sub-campaign row. Schema-v2 plans carry `steps` and `plan_items`
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
- `sub-campaign` — the Overview row's `<link-unit>/<subsystem>`, or
  `raw-lifetime-void` / `raw-lifetime-string`
- `sub-campaign wall` — from its first batch launch through its final review
  and regression gate
- `batch` / `review` — in the Overview, the computed cost of the sub-campaign's
  translation batches and of its review batches; `—` where it ran none
- `total` — `batch` + `review`; for the orchestrator, its own cost
- `$/type` / `$/sym` — in the Overview, a sub-campaign's cost over the types or
  symbols it was scheduled for; `—` where it was scheduled for none
- `+LoC` — the landed batch's Rust-source insertions/deletions relative to its
  wave's base
- `cov (C/Rust)` — in a `Batches` table, the absolute line coverage right after
  that batch landed, from ONE run of all four test workloads together (their
  union): C over the sub-campaign link unit's TUs in the `coverage` prebuild,
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
  batch filed which the orchestrator rejected after rerunning them: they did not
  reproduce, or did not show a real bug, and their fixes were reverted. They
  stay counted in their categories; `total` minus this column is the confirmed
  defects
- `+unsafe fn smell` / `+raw-ptr smell` — the change the landed batch made to the
  `unsafe fn` smell and the raw-pointer smell (total − seam) of the static scan
  over its names, against its wave's base; a review batch's fixes show as
  negative values
- `rv $` / `rv wall` / `rv loc` — the REVIEW agent's cost, elapsed time, and net
  `.rs` line delta (`+ins/-del`) of its landing commit. Under subscription
  billing `rv $` is an API-equivalent comparison value, not a charged amount
- `UB tests` / `Equiv tests` / `Unit tests` — counts of `#[test]`
  functions under `mod ub_tests` / `mod equiv_tests` / `mod units_tests`. Each
  coverage pair comes from running only that workload, against the
  coverage-instrumented target C sources and authored Rust sources; state any
  excluded files or generated code in Notes. The three workloads bound different
  obligations, so report them apart and never sum them. Report the UB
  workload once, from a single coverage run on the plain build: it executes once
  per prepared sanitizer, but those runs share one test set and re-counting them
  inflates the figure. Name the instruments it ran under in Notes
- `Total tests` — the three counts added. Its coverage pair is NOT the three
  pairs added: it comes from one run of all three workloads together and is the
  union of the lines any of them reached, which is smaller than the sum wherever
  two workloads cover the same line. Deriving it arithmetically overstates it.
  It is a reach figure only — it mixes three verdict sources and so bounds no
  single obligation, which is why the three stay reported apart above it

Every table below is a heading, a model line and the table. All prose belongs
in Notes.

## Overview

- **Rust LoC, non-test** — `<n>`
- **Rust LoC, tests** — `<n>`
- **UB safe tests** — `<count of #[test]>` (`<n>`% C LoC coverage, `<n>`% Rust LoC coverage)
- **UB unsafe tests** — `<count of #[test]>` (`<n>`% C LoC coverage, `<n>`% Rust LoC coverage)
- **Equiv tests** — `<count of #[test]>` (`<n>`% C LoC coverage, `<n>`% Rust LoC coverage)
- **Unit tests** — `<count of #[test]>` (`<n>`% C LoC coverage, `<n>`% Rust LoC coverage)
- **Total tests** — `<sum of the three counts>` (`<n>`% C LoC coverage, `<n>`% Rust LoC coverage)
- **C LoC** — `<n>`
- **ported types** — `<n>`
- **ported symbols** — `<n>`
- **wrapped types** — `<n>`
- **wrapped symbols** — `<n>`

Implementation `openrouter/anthropic/claude-opus-5.5` via `claude`; review
`openrouter/anthropic/claude-opus-5.5` via `claude`.

| sub-campaign | nr types | nr symbols | sub-campaign wall | batch | review | total | $/type | $/sym |
|---|---:|---:|---|---:|---:|---:|---:|---:|
| `raw-lifetime-void` | `0` | `6` | `78m48s` | `$8.46` | `$4.69` | `$13.15` | — | `$2.19` |
| `raw-lifetime-string` | `0` | `3` | `48m03s` | `$3.77` | `$5.25` | `$9.02` | — | `$3.01` |
| `<link-unit>/<subsystem>` | `<n>` | `<n>` | `<n>h<n>m<n>s` | `$<n>` | `$<n>` | `$<n>` | `$<n>` | `$<n>` |
| orchestrator | `<n>` | `<n>` | — | — | — | `$<n>`+ | — | — |
| **Σ recorded agents** | **`<n>`** | **`<n>`** | **`<n>h<n>m`** | **`$<n>`** | **`$<n>`** | **`$<n>`** | **`$<n>`** | **`$<n>`** |

## Raw lifetime discovery

`openrouter/anthropic/claude-opus-5.5` via `claude`.

| tier | strategies emitted | $ | wall |
|---|---|---|---|
| void | `3` | `$8.46` | `57m23s` |
| string | `2` | `$3.77` | `19m11s` |
| **Σ** | **`5`** | **`$12.23`** | **`76m34s`** |

### Review

`openrouter/anthropic/claude-opus-5.5` via `claude`.

| tier | rv loc | rv $ | rv wall |
|---|---|---|---|
| void | `+241/-4` | `$4.69` | `17m36s` |
| string | `+86/-17` | `$5.25` | `26m33s` |
| **Σ** | **`+327/-21`** | **`$9.94`** | **`44m09s`** |

## Types

### Batches — wrap

`openrouter/anthropic/claude-opus-5.5` via `claude`.

| wave | batch | types | fields | +LoC | cov (C/Rust) | +unsafe fn smell | +raw-ptr smell | $ | wall | $/type | $/field |
|---|---|---|---|---|---|---|---|---|---|---|---|
|`0`|`0`| `2` | `0` |`+649/-0`| `2.18`% / `99.41`% | `0` | `0` | `$2.13` | `10m25s` | `$1.06` | `—` |
|`0`|`1`| `2` | `0` |`+569/-0`| `2.92`% / `99.63`% | `0` | `0` | `$2.77` | `16m06s` | `$1.39` | `—` |
|`0`|`2`| `2` | `0` |`+1154/-0`| `9.10`% / `86.96`% | `+4` | `+3` | `$7.87` | `32m39s` | `$3.94` | `—` |
|`0`|`3`| `2` | `2` |`+848/-2`| `9.22`% / `88.07`% | `0` | `0` | `$4.39` | `28m48s` | `$2.19` | `$2.19` |
|`0`|`4`| `2` | `0` |`+504/-0`| `3.29`% / `99.75`% | `0` | `0` | `$4.53` | `23m25s` | `$2.27` | `—` |
|`0`|`5`| `2` | `6` |`+650/-2`| `2.92`% / `99.72`% | `0` | `0` | `$3.35` | `20m31s` | `$1.68` | `$0.56` |
|`0`|`6`| `2` | `8` |`+773/-6`| `3.72`% / `99.82`% | `0` | `0` | `$6.15` | `29m17s` | `$3.08` | `$0.77` |
|`0`|`7`| `1` | `2` |`+494/-0`| `2.36`% / `99.56`% | `0` | `0` | `$2.61` | `14m49s` | `$2.61` | `$1.30` |
|`0`|`8`| `2` | `2` |`+741/-2`| `3.72`% / `99.78`% | `0` | `0` | `$5.48` | `25m56s` | `$2.74` | `$2.74` |
|`0`|`9`| `1` | `1` |`+396/-0`| `2.36`% / `99.48`% | `0` | `0` | `$2.95` | `12m24s` | `$2.95` | `$2.95` |
|`1`|`0`| `2` | `18` |`+1555/-8`| `19.58`% / `95.62`% | `+9` | `+7` | `$13.38` | `43m37s` | `$6.69` | `$0.74` |
|`1`|`1`| `1` | `5` |`+500/-2`| `19.23`% / `95.13`% | `+1` | `0` | `$3.10` | `20m22s` | `$3.10` | `$0.62` |
| **Σ** |  | **`21`** | **`44`** | **`+8833/-22`** | **`19.58`% / `95.62`%** | **`+14`** | **`+10`** | **`$58.71`** | — | **`$2.80`** | **`$1.33`** |

### Batches — wrap: tests

`openrouter/anthropic/claude-opus-5.5` via `claude`.

| wave | batch | +UB safe tests (+C/+Rust pp) | +UB unsafe tests (+C/+Rust pp) | +equiv tests (+C/+Rust pp) | +unit tests (+C/+Rust pp) |
|---|---|---|---|---|---|
|`0`|`0`|`+3` (`+0.00`/`+12.03` pp)|`+0` (`+0.00`/`+0.00` pp)|`+3` (`+1.69`/`-29.59` pp)|`+9` (`+0.00`/`-0.59` pp)|
|`0`|`1`|`+8` (`+0.00`/`-0.93` pp)|`+0` (`+0.00`/`+0.00` pp)|`+11` (`+0.57`/`-2.30` pp)|`+9` (`+0.00`/`+0.07` pp)|
|`0`|`2`|`+0` (`+0.00`/`-20.74` pp)|`+10` (`+1.10`/`+6.13` pp)|`+5` (`+0.74`/`-13.82` pp)|`+12` (`+5.51`/`-15.47` pp)|
|`0`|`3`|`+14` (`+0.00`/`+3.17` pp)|`+0` (`+0.00`/`-0.52` pp)|`+10` (`+0.37`/`-0.37` pp)|`+15` (`+0.00`/`+0.97` pp)|
|`0`|`4`|`+4` (`+0.00`/`+1.78` pp)|`+0` (`+0.00`/`+0.00` pp)|`+9` (`+0.37`/`-1.69` pp)|`+8` (`+0.00`/`+0.03` pp)|
|`0`|`5`|`+9` (`+0.00`/`-1.08` pp)|`+0` (`+0.00`/`+0.00` pp)|`+6` (`+0.00`/`+0.85` pp)|`+7` (`+0.00`/`+0.10` pp)|
|`0`|`6`|`+9` (`+0.00`/`-4.18` pp)|`+0` (`+0.00`/`+0.00` pp)|`+7` (`+0.00`/`+2.75` pp)|`+12` (`+0.00`/`+0.04` pp)|
|`0`|`7`|`+9` (`+0.00`/`-3.59` pp)|`+0` (`+0.00`/`+0.00` pp)|`+8` (`+0.00`/`+0.19` pp)|`+9` (`+0.00`/`+0.08` pp)|
|`0`|`8`|`+10` (`+0.00`/`-1.32` pp)|`+0` (`+0.00`/`+0.00` pp)|`+8` (`+0.43`/`+1.09` pp)|`+7` (`+0.00`/`+0.03` pp)|
|`0`|`9`|`+10` (`+0.00`/`+9.78` pp)|`+0` (`+0.00`/`+0.00` pp)|`+5` (`+0.18`/`+1.84` pp)|`+4` (`+0.00`/`+0.07` pp)|
|`1`|`0`|`+8` (`+0.08`/`+0.87` pp)|`+4` (`+0.00`/`+1.22` pp)|`+8` (`+0.37`/`+1.14` pp)|`+11` (`+0.34`/`-0.22` pp)|
|`1`|`1`|`+8` (`+0.00`/`+0.09` pp)|`+5` (`+0.00`/`+0.68` pp)|`+3` (`+0.00`/`-0.24` pp)|`+5` (`+0.00`/`+0.44` pp)|
| **Σ** |  | **`+92` (`+0.08`/`-4.13` pp)** | **`+19` (`+1.10`/`+7.51` pp)** | **`+83` (`+4.71`/`-40.15` pp)** | **`+108` (`+5.85`/`-14.45` pp)** |

### Batches — port

`<provider>/<model>` via `<backend>`.

| wave | batch | types | fields | +LoC | cov (C/Rust) | +unsafe fn smell | +raw-ptr smell | $ | wall | $/type | $/field |
|---|---|---|---|---|---|---|---|---|---|---|---|
|`<n>`|`<n>`| `<n>` | `<n>` |`+<n>/-<n>`| `<n>`% / `<n>`% | `±<n>` | `±<n>` | `$<n>` | `<n>m<n>s` | `$<n>` | `$<n>` |
| **Σ** |  | **`<n>`** | **`<n>`** | **`+<n>/-<n>`** | **`<n>`% / `<n>`%** | **`±<n>`** | **`±<n>`** | **`$<n>`** | — | **`$<n>`** | **`$<n>`** |

### Batches — port: tests

`<provider>/<model>` via `<backend>`.

| wave | batch | +UB safe tests (+C/+Rust pp) | +UB unsafe tests (+C/+Rust pp) | +equiv tests (+C/+Rust pp) | +unit tests (+C/+Rust pp) |
|---|---|---|---|---|---|
|`<n>`|`<n>`|`+<n>` (`+<n>`/`+<n>` pp)|`+<n>` (`+<n>`/`+<n>` pp)|`+<n>` (`+<n>`/`+<n>` pp)|`+<n>` (`+<n>`/`+<n>` pp)|
| **Σ** |  | **`+<n>` (`+<n>`/`+<n>` pp)** | **`+<n>` (`+<n>`/`+<n>` pp)** | **`+<n>` (`+<n>`/`+<n>` pp)** | **`+<n>` (`+<n>`/`+<n>` pp)** |

### Batches — review

`openrouter/anthropic/claude-fable-5.1` via `claude` (wave 0); `openrouter/openai/gpt-6-astra` via `codex` (wave 1).

| wave | batch | types | +LoC | cov (C/Rust) | +unsafe fn smell | +raw-ptr smell | rv loc | rv $ | rv wall | rv $/type |
|---|---|---|---|---|---|---|---|---|---|---|
|`0`|`0`| `2` |`+265/-0`| `19.00`% / `94.73`% | `0` | `0` | `+350/-0` | `$8.17` | `30m15s` | `$4.08` |
|`0`|`1`| `2` |`+0/-0`| — | `0` | `0` | `+0/-0` | `$4.58` | `17m23s` | `$2.29` |
|`0`|`2`| `2` |`+147/-3`| `18.96`% / `94.71`% | `0` | `0` | `+239/-15` | `$10.68` | `28m27s` | `$5.34` |
|`0`|`3`| `2` |`+138/-2`| `19.00`% / `94.73`% | `0` | `0` | `+168/-2` | `$10.67` | `33m26s` | `$5.33` |
|`0`|`4`| `2` |`+3/-0`| `18.96`% / `94.69`% | `0` | `0` | `+14/-0` | `$4.92` | `18m50s` | `$2.46` |
|`0`|`5`| `2` |`+399/-7`| `19.00`% / `94.83`% | `0` | `0` | `+524/-8` | `$10.30` | `37m29s` | `$5.15` |
|`0`|`6`| `2` |`+228/-0`| `18.96`% / `94.71`% | `0` | `0` | `+303/-6` | `$8.43` | `25m31s` | `$4.21` |
|`0`|`7`| `1` |`+97/-1`| `18.96`% / `94.69`% | `0` | `0` | `+127/-1` | `$5.99` | `20m21s` | `$5.99` |
|`0`|`8`| `2` |`+41/-6`| `19.00`% / `94.74`% | `0` | `0` | `+62/-10` | `$8.73` | `34m59s` | `$4.37` |
|`0`|`9`| `1` |`+24/-7`| `18.96`% / `94.69`% | `0` | `0` | `+31/-10` | `$4.68` | `16m27s` | `$4.68` |
|`1`|`0`| `3` |`+98/-0`| `20.40`% / `96.05`% | `0` | `0` | `+132/-8` | `$13.17` | `13m21s` | `$4.39` |
| **Σ** |  | **`21`** | **`+1440/-26`** | **`20.40`% / `96.05`%** | **`0`** | **`0`** | **`+1950/-60`** | **`$90.32`** | — | **`$4.30`** |

### Batches — review: tests

`openrouter/anthropic/claude-fable-5.1` via `claude` (wave 0); `openrouter/openai/gpt-6-astra` via `codex` (wave 1).

| wave | batch | +UB safe tests (+C/+Rust pp) | +UB unsafe tests (+C/+Rust pp) | +equiv tests (+C/+Rust pp) | +unit tests (+C/+Rust pp) |
|---|---|---|---|---|---|
|`0`|`0`|`+1` (`+0.00`/`+0.08` pp)|`+0` (`+0.00`/`-0.03` pp)|`+4` (`+0.04`/`+0.08` pp)|`+1` (`+0.00`/`+0.06` pp)|
|`0`|`1`|`+0` (`+0.00`/`+0.00` pp)|`+0` (`+0.00`/`+0.00` pp)|`+0` (`+0.00`/`+0.00` pp)|`+0` (`+0.00`/`+0.00` pp)|
|`0`|`2`|`+0` (`+0.00`/`+0.00` pp)|`+2` (`+0.00`/`+0.00` pp)|`+2` (`+0.00`/`+0.00` pp)|`+1` (`+0.00`/`+0.00` pp)|
|`0`|`3`|`+2` (`+0.00`/`+0.00` pp)|`+0` (`+0.00`/`+0.00` pp)|`+1` (`+0.00`/`+0.00` pp)|`+1` (`+0.00`/`+0.00` pp)|
|`0`|`4`|`+0` (`+0.00`/`+0.00` pp)|`+0` (`+0.00`/`+0.00` pp)|`+0` (`+0.00`/`+0.00` pp)|`+0` (`+0.00`/`+0.00` pp)|
|`0`|`5`|`+5` (`+0.00`/`+0.14` pp)|`+0` (`+0.00`/`-0.19` pp)|`+2` (`+0.00`/`+0.01` pp)|`+3` (`+0.00`/`+0.31` pp)|
|`0`|`6`|`+0` (`+0.00`/`-0.35` pp)|`+0` (`+0.00`/`-0.05` pp)|`+3` (`+0.00`/`-0.35` pp)|`+2` (`+0.00`/`+0.08` pp)|
|`0`|`7`|`+2` (`+0.00`/`+0.00` pp)|`+0` (`+0.00`/`+0.00` pp)|`+1` (`+0.00`/`+0.00` pp)|`+0` (`+0.00`/`+0.00` pp)|
|`0`|`8`|`+0` (`+0.00`/`-0.12` pp)|`+0` (`+0.00`/`-0.02` pp)|`+0` (`+0.00`/`-0.12` pp)|`+1` (`+0.00`/`+0.03` pp)|
|`0`|`9`|`+0` (`+0.00`/`+0.12` pp)|`+0` (`+0.00`/`+0.02` pp)|`+0` (`+0.00`/`+0.12` pp)|`+1` (`+0.00`/`-0.03` pp)|
|`1`|`0`|`+2` (`+0.00`/`+0.00` pp)|`+0` (`+0.00`/`+0.00` pp)|`+1` (`+0.00`/`+0.00` pp)|`+1` (`+0.00`/`+0.00` pp)|
| **Σ** |  | **`+12` (`+0.00`/`-0.13` pp)** | **`+2` (`+0.00`/`-0.27` pp)** | **`+14` (`+0.04`/`-0.26` pp)** | **`+11` (`+0.00`/`+0.45` pp)** |

### Batches — review: PoC + reports

`openrouter/anthropic/claude-fable-5.1` via `claude` (wave 0); `openrouter/openai/gpt-6-astra` via `codex` (wave 1).

| wave | batch | +lifecycle | +unsafe | +UB safe | +UB unsafe | +equiv | +internal | +conventions | +misc | total | +rejected PoCs |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `0` | `0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+1` | `1` | `0` |
| `0` | `1` | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `0` | `0` |
| `0` | `2` | `+0` | `+1` | `+0` | `+0` | `+0` | `+0` | `+0` | `+3` | `4` | `0` |
| `0` | `3` | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+1` | `1` | `0` |
| `0` | `4` | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+1` | `1` | `0` |
| `0` | `5` | `+2` | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+1` | `3` | `0` |
| `0` | `6` | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+1` | `1` | `0` |
| `0` | `7` | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `0` | `0` |
| `0` | `8` | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+1` | `1` | `0` |
| `0` | `9` | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+1` | `1` | `0` |
| `1` | `0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+2` | `2` | `0` |
| **Σ** |  | **`+2`** | **`+1`** | **`+0`** | **`+0`** | **`+0`** | **`+0`** | **`+0`** | **`+12`** | **`15`** | **`0`** |

## Symbols

### Batches

`openrouter/anthropic/claude-opus-5.5` via `claude`.

| wave | batch | objective | symbols | loc | +LoC | cov (C/Rust) | +unsafe fn smell | +raw-ptr smell | $ | wall | $/symbol | $/loc |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
|`0`|`10`| wrap | `25` | `1283` |`+1283/-3`| `11.21`% / `90.81`% | `+1` | `+1` | `$17.79` | `86m15s` | `$0.71` | `$0.014` |
|`0`|`11`| wrap | `25` | `1877` |`+1877/-14`| `12.54`% / `92.30`% | `+4` | `+7` | `$28.38` | `124m29s` | `$1.14` | `$0.015` |
|`0`|`12`| wrap | `25` | `2896` |`+2896/-176`| `18.96`% / `94.70`% | `+2` | `+3` | `$45.49` | `178m41s` | `$1.82` | `$0.016` |
|`0`|`13`| wrap | `25` | `2611` |`+2611/-3`| `18.00`% / `93.86`% | `+1` | `0` | `$27.60` | `155m09s` | `$1.10` | `$0.011` |
|`0`|`14`| wrap | `21` | `1561` |`+1561/-1`| `10.37`% / `89.92`% | `+1` | `+1` | `$11.38` | `62m02s` | `$0.54` | `$0.007` |
|`1`|`2`| wrap | `25` | `2218` |`+2218/-1`| `20.40`% / `96.05`% | `+1` | `+1` | `$15.41` | `63m58s` | `$0.62` | `$0.007` |
|`1`|`3`| wrap | `1` | `264` |`+264/-0`| `19.23`% / `95.00`% | `0` | `0` | `$1.71` | `11m10s` | `$1.71` | `$0.006` |
| **Σ** |  |  | **`147`** | **`12710`** | **`+12710/-198`** | **`20.40`% / `96.05`%** | **`+10`** | **`+13`** | **`$147.76`** | — | **`$1.01`** | **`$0.012`** |

### Batches: tests

`openrouter/anthropic/claude-opus-5.5` via `claude`.

| wave | batch | +UB safe tests (+C/+Rust pp) | +UB unsafe tests (+C/+Rust pp) | +equiv tests (+C/+Rust pp) | +unit tests (+C/+Rust pp) |
|---|---|---|---|---|---|
|`0`|`10`|`+16` (`+0.82`/`+2.63` pp)|`+6` (`+0.02`/`-0.08` pp)|`+21` (`+0.85`/`+2.96` pp)|`+18` (`+0.79`/`+1.65` pp)|
|`0`|`11`|`+20` (`+1.27`/`+2.84` pp)|`+7` (`+0.53`/`+1.20` pp)|`+29` (`+1.49`/`+3.35` pp)|`+13` (`+0.74`/`-1.55` pp)|
|`0`|`12`|`+20` (`+1.66`/`-0.46` pp)|`+13` (`+0.45`/`+6.11` pp)|`+21` (`+0.90`/`+0.47` pp)|`+22` (`+1.07`/`-0.12` pp)|
|`0`|`13`|`+27` (`+6.37`/`+3.54` pp)|`+0` (`+0.00`/`-1.35` pp)|`+27` (`+6.86`/`+4.73` pp)|`+37` (`+1.83`/`+0.77` pp)|
|`0`|`14`|`+20` (`+2.05`/`+4.45` pp)|`+5` (`+1.19`/`-0.57` pp)|`+26` (`+1.05`/`+5.47` pp)|`+12` (`+0.46`/`-4.07` pp)|
|`1`|`2`|`+21` (`+1.48`/`+0.86` pp)|`+6` (`+0.02`/`-0.89` pp)|`+28` (`+0.91`/`+0.49` pp)|`+30` (`+1.54`/`+1.70` pp)|
|`1`|`3`|`+4` (`+0.10`/`+0.10` pp)|`+0` (`+0.00`/`-0.04` pp)|`+4` (`+0.10`/`+0.09` pp)|`+2` (`+0.10`/`+0.21` pp)|
| **Σ** |  | **`+128` (`+13.75`/`+13.96` pp)** | **`+37` (`+2.20`/`+4.38` pp)** | **`+156` (`+12.15`/`+17.55` pp)** | **`+134` (`+6.53`/`-1.40` pp)** |

### Batches — review

`openrouter/anthropic/claude-fable-5.1` via `claude`.

| wave | batch | symbols | +LoC | cov (C/Rust) | +unsafe fn smell | +raw-ptr smell | rv loc | rv $ | rv wall | rv $/symbol |
|---|---|---|---|---|---|---|---|---|---|---|
|`0`|`10`| `25` |`+45/-1`| `19.02`% / `94.83`% | `0` | `0` | `+79/-14` | `$18.09` | `52m22s` | `$0.72` |
|`0`|`11`| `25` |`+75/-0`| `19.02`% / `94.83`% | `0` | `0` | `+97/-0` | `$16.16` | `43m21s` | `$0.65` |
|`0`|`12`| `25` |`+145/-6`| `19.14`% / `94.98`% | `0` | `0` | `+208/-18` | `$26.29` | `57m24s` | `$1.05` |
|`0`|`13`| `25` |`+163/-9`| `19.14`% / `94.92`% | `0` | `0` | `+230/-19` | `$25.37` | `53m00s` | `$1.01` |
|`0`|`14`| `21` |`+65/-12`| `19.00`% / `94.83`% | `0` | `0` | `+93/-23` | `$10.99` | `32m35s` | `$0.52` |
|`1`|`1`| `26` |`+56/-5`| `20.40`% / `96.06`% | `0` | `0` | `+89/-13` | `$10.27` | `29m53s` | `$0.39` |
| **Σ** |  | **`147`** | **`+549/-33`** | **`20.40`% / `96.06`%** | **`0`** | **`0`** | **`+796/-87`** | **`$107.17`** | — | **`$0.73`** |

### Batches — review: tests

`openrouter/anthropic/claude-fable-5.1` via `claude`.

| wave | batch | +UB safe tests (+C/+Rust pp) | +UB unsafe tests (+C/+Rust pp) | +equiv tests (+C/+Rust pp) | +unit tests (+C/+Rust pp) |
|---|---|---|---|---|---|
|`0`|`10`|`+0` (`+0.00`/`-0.08` pp)|`+1` (`+0.00`/`+0.09` pp)|`+0` (`+0.00`/`+0.02` pp)|`+1` (`+0.00`/`+0.02` pp)|
|`0`|`11`|`+0` (`+0.00`/`+0.00` pp)|`+0` (`+0.00`/`+0.00` pp)|`+1` (`+0.02`/`+0.00` pp)|`+0` (`+0.00`/`+0.00` pp)|
|`0`|`12`|`+1` (`+0.00`/`+0.33` pp)|`+0` (`+0.00`/`-0.14` pp)|`+0` (`+0.00`/`+0.26` pp)|`+3` (`+0.00`/`+0.22` pp)|
|`0`|`13`|`+0` (`+0.00`/`+0.02` pp)|`+0` (`+0.00`/`-0.18` pp)|`+2` (`+0.10`/`+0.30` pp)|`+2` (`+0.20`/`+0.29` pp)|
|`0`|`14`|`+0` (`+0.00`/`+0.00` pp)|`+0` (`+0.00`/`+0.00` pp)|`+1` (`+0.00`/`+0.00` pp)|`+0` (`+0.00`/`+0.00` pp)|
|`1`|`1`|`+0` (`+0.00`/`+0.05` pp)|`+0` (`+0.00`/`-0.03` pp)|`+1` (`+0.00`/`+0.05` pp)|`+0` (`+0.00`/`+0.03` pp)|
| **Σ** |  | **`+1` (`+0.00`/`+0.33` pp)** | **`+1` (`+0.00`/`-0.25` pp)** | **`+5` (`+0.12`/`+0.63` pp)** | **`+6` (`+0.20`/`+0.56` pp)** |

### Batches — review: PoC + reports

`openrouter/anthropic/claude-fable-5.1` via `claude`.

| wave | batch | +lifecycle | +unsafe | +UB safe | +UB unsafe | +equiv | +internal | +conventions | +misc | total | +rejected PoCs |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `0` | `10` | — | `+1` | `+0` | `+0` | `+0` | `+0` | `+0` | `+1` | `2` | `0` |
| `0` | `11` | — | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `+0` | `0` | `0` |
| `0` | `12` | — | `+0` | `+1` | `+0` | `+0` | `+0` | `+1` | `+1` | `3` | `0` |
| `0` | `13` | — | `+0` | `+0` | `+0` | `+1` | `+0` | `+0` | `+0` | `1` | `0` |
| `0` | `14` | — | `+0` | `+0` | `+0` | `+0` | `+0` | `+2` | `+1` | `3` | `0` |
| `1` | `1` | — | `+0` | `+0` | `+0` | `+1` | `+0` | `+0` | `+0` | `1` | `0` |
| **Σ** |  | — | **`+1`** | **`+1`** | **`+0`** | **`+2`** | **`+0`** | **`+3`** | **`+3`** | **`10`** | **`0`** |

## Safety audit

Deterministic `crustify scan-unsafe <workdir>`; no model.

### Final tally overview

| | final (`<sha>`) |
|---|---|
| unsafe loc | `<n>` |
| % of loc | `<n>`% |
| blocks | `<n>` |
| % in `impl T` | `<n>`% |
| `unsafe fn` | `<n>` |
| ...of which not sanctioned | `<n>` |
| raw-ptr smell | `<n>` |
| void-ptr smell | `<n>` |
| FFI calls | `<n>` |
| `&`/`&mut` on a wrapper | `<n>` |
| field proj outside an accessor | `<n>` |

### All metrics

| metric | final (`<sha>`) | `<link-unit>/<subsystem>` (`<sha>`) | … |
|---|---|---|---|
| `code_lines` | `<n>` | `<n>` | … |
| `total_stmts` | `<n>` | `<n>` | … |
| `unsafe_blocks` | `<n>` | `<n>` | … |
| `unsafe_block_stmts` | `<n>` | `<n>` | … |
| `unsafe_block_lines` | `<n>` | `<n>` | … |
| `unsafe_block_code_lines` | `<n>` | `<n>` | … |
| `unsafe_blocks_wrapper_impl` | `<n>` | `<n>` | … |
| `unsafe_blocks_ffi_export` | `<n>` | `<n>` | … |
| `unsafe_fns` | `<n>` | `<n>` | … |
| `unsafe_fns_seam` | `<n>` | `<n>` | … |
| **`unsafe fn` smell** | **`<n>`** | `<n>` | … |
| `unsafe_fns_pub` | `<n>` | `<n>` | … |
| `unsafe_impls` / `unsafe_traits` | `<n>` / `<n>` | `<n>` / `<n>` | … |
| `ffi_calls` | `<n>` | `<n>` | … |
| `wrapper_newtypes` | `<n>` | `<n>` | … |
| `wrapper_newtypes_declared` | `<n>` | `<n>` | … |
| `wrapper_declared_nonconformant` | `<n>` | `<n>` | … |
| `wrapper_newtypes_undeclared` | `<n>` | `<n>` | … |
| `raw_ptr_args` | `<n>` | `<n>` | … |
| `raw_ptr_rets` | `<n>` | `<n>` | … |
| **total positions** | **`<n>`** | `<n>` | … |
| `raw_ptr_seam` | `<n>` | `<n>` | … |
| **smell (total − seam)** | **`<n>`** | `<n>` | … |
| `raw_ptr_wrapped` | `<n>` | `<n>` | … |
| `raw_ptr_in_wrapper` | `<n>` | `<n>` | … |
| `raw_ptr_derefs` | `<n>` | `<n>` | … |
| `ref_to_type_wrapper` | `<n>` | `<n>` | … |
| `field_proj_wrapped` | `<n>` | `<n>` | … |
| `field_proj_outside_impl` | `<n>` | `<n>` | … |
| `field_ref_wrapped` | `<n>` | `<n>` | … |
| `void_ptr_sanctioned` | `<n>` | `<n>` | … |
| `void_ptr_smell` | `<n>` | `<n>` | … |

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

`crustify/build.json` (version 3) carries `plain`, `coverage`, `asan-ubsan`, `tsan` and `bsan`.
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

The translation batch (`77473b8574`, `+775/-0` `.rs`) bound `av_free` as the release policy
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

The scheduled workset name `void` names no C entity, so `scan-unsafe --name void` matches no
site; both batch dirs keep that output (`scan-unsafe.void.txt`) and a scan seeded with the 6
anchored primitives (`scan-unsafe.json`). Against the empty base, the translation batch moved
the `unsafe fn` smell by `+5` and the raw-pointer smell by `+5`, and the review by `+1` and `0`.
The scan's `unsafe_fns_pub` (3, then 4) counts the `unsafe fn` methods of ffibox's unsafe
policy traits implemented on `Heap`; neither batch added public unsafe API. The two raw-pointer
positions are the private `free_len`/`memdup` adapters whose signatures ffibox's macros fix.

Under `bsan`, freeing an `av_calloc` block after a Rust byte view of 32 KiB or more was taken
over it grew the process without bound (about 180 GB in 12 minutes), while 4 KiB views ran
clean; the translator attributes it to the bsan runtime, and the equivalence tests keep
successful allocations at 4 KiB or less.

The review (`72abd2c241`, `9cdb9f2a73`) filed three defects, all confirmed on rerun:

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

The translation batch (`a95b9e3220`, `+429/-4` `.rs`) added `HeapString`, ffibox's `CStrBox`
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

The review (`f337203e0d`, `8bdf2c7138`) filed three reports and no reproducer, so none was
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
rerun at `050c2adfa0` against it: `clippy` clean, and the plain, `asan-ubsan`, `tsan` and `bsan`
suites pass. The void review's reproducer no longer compiles (`E0080`).

### `libavutil/core` wave 0

Wave 0 is 15 batches: 10 type batches (18 types, 21 scheduled field anchors) and 5 symbol
batches (121 symbols), all `wrap`. The interrupted session launched batches 0–2 and 4–9 at
02:54Z on 2026-10-08; all nine landed by 03:27Z. It never prepared batch 3 or batches 10–14 and
left no record of why. The resumed session launched those six at 18:46Z from the wave tip
`6ae1112aa5`; they landed between 19:15Z and 21:45Z. All 139 scheduled items carry their anchor
on the wave tip `3770f8f483`. Translation cost `$172.86` for 424.5M tokens and 13h41m of agent
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
  modules. All four test binaries serve as coverage objects for every workload, so the four
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
- Smell deltas come from unseeded scans (`scan-unsafe.tree.json`) at the landing commit and
  its parent. They sum to `+13` (`unsafe fn`) and `+15` (raw pointer) over the base's 8 and 5,
  which equals the wave tip's 21 and 20. The seeded scans of batches 0, 1 and 4 matched no site
  (`scan-unsafe.txt`).

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
  `Copy` type in batch 7's tests. Commit `6ae1112aa5` re-blessed the `.stderr` and allowed
  `clone_on_copy` on that statement. Both are test-only.
- **BSan prebuild.** It was rebuilt with the unchanged `build.json` commands (still version 3)
  after reinstalling `zlib1g-dev`; its `CONFIG_*` set is identical. The new runtime no longer
  reports the `realloc(NULL, n)` false use-after-free. Instead it aborts multi-threaded suites
  when its garbage collector waits more than 10 s for a thread to reach a safepoint;
  `BSAN_OPTIONS=gc_hang_threshold_ms=600000` lets every suite pass.
- **`crustify scan-unsafe`.** Its `rustc_private` driver no longer built on the new nightly, so
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
  `crustify/reviews/20261005/libavutil/core/wave-0` (tip `0604718bba`). The batch-1 reviewer
  checked `AVEscapeMode` and `AVMediaType` under every sanitizer, found nothing and landed no
  commit; its row reads `+0/-0` and `—` for `cov`.
- **Reports:** 29 defect reports plus 4 batch summaries. The PoC tables count 22. They have no
  `wavefront` column, so the 7 `wavefront/` reports (record corrections, submitted to the
  ownership store) are counted nowhere above. Three of those correct union types recorded as
  `kind: struct` (`av_alias64`, `av_intfloat32`, `unaligned_64`).
- **Reproducers:** all five runnable ones were rerun on their affected commit and on the
  reviewed tip, and all five hold, so none is rejected:
  - `ub_safe/memcpy-backptr-block-length-overflow` (batch 12). On `3770f8f483`, safe
    `copy_back_reference` with a distance of 2^30 and a run one byte longer makes C's `int`
    block length overflow (UBSan, `mem.c:466`). `memcpy` then runs with a negative size
    (ASan). On `0604718bba` it fills the run and exits 0: the wrapper now splits such runs
    into calls within the bound.
  - `equiv/parse-cpu-caps-prefixed-operand-rounds` (batch 13). `parse_cpu_caps` rounded `+`/`-`
    operands where C truncates them, and rejected some that C accepts: 10 inputs diverged on
    `6978702856`, and all agree on the reviewed tip.
  - `unsafe/default-item-name-alignment-unstated` (batch 10). A misaligned context that
    satisfied the old `# Safety` text triggers UBSan's misaligned load at `log.c:243` on
    `35f30a7185`. The contract now requires alignment.
  - `misc/option-ranges-box-not-send-sync` (batch 2): 15 `E0277` errors on `ad7acaeaca`; builds
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
- **Promotion:** the reviewed tip was promoted to `crustify/subcampaigns/20261005/libavutil/core`.

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

Wave 1 is 4 `wrap` batches based on the reviewed wave-0 tip `0604718bba`:

- batch 0: `AVBPrint` and `AVClass`, 18 fields;
- batch 1: `ff_pad_helper_AVBPrint`, 5 fields;
- batch 2: 25 symbols;
- batch 3: `av_ts_make_time_string2`.

All four landed between 00:42Z and 01:36Z on 2026-10-09, and every scheduled item and field
carries its anchor on the wave tip `23dc07587c`. Translation cost `$33.60` for 91.7M tokens and
2h19m of agent wall time.

Notes on the wave:

- `av_make_q`, scheduled in batch 2, was already anchored by wave 0's `AVRational` batch
  (`7f5c7edf52`); batch 2 left it as completed work.
- The 25-symbol batch took 64 minutes, against 86–179 in wave 0, with no sibling landing in its
  homes.
- Batch 0 accounts for most of the wave's smell growth (`+9` `unsafe fn`, `+7` raw pointer):
  most of it is `AVClass`'s callback fields, wrapped as `unsafe fn call` handles over C
  function pointers taking `void *` contexts (4 raw-pointer sites in `log.rs`), and the rest
  `AVBPrint`'s buffer (3 in `bprint.rs`).
- Batch rows sum to `+11` and `+8`, which equals the wave tip's unseeded scan (32 and 28) over the
  reviewed base (21 and 20). No undocumented `unsafe`.
- The Σ `cov` cells now read wave 1's last type and symbol landings: batch 0 (`364c59df9b`) and
  batch 2 (`23dc07587c`, the wave tip). The rows' `cov` and `: tests` figures continue the
  wave-0 method.

The wave tip passes `clippy --all-targets -D warnings` and the plain, `asan-ubsan`, `tsan` and
`bsan` suites: 275 unit, 273 equivalence, 252 `ub_safe` and 59 `ub_unsafe` tests under `bsan`.
Its review runs with wave 2's, on Opus 5.5, after wave 2.

### Dependency merges after `core` wave 1

On 2026-10-09, at the user's direction:

- crustify `main` took the `scan-unsafe` driver pin (`4efc0cc`) and the results-template split
  (`2a4a613`). The live `crustify scan-unsafe` then gave the same counts on the wave-1 tip as the
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

- **Types reviewer:** ran 13m21s without any provider intervention and landed `3624aef099` and
  `13ae407387`. Its 5 reports are 2 `misc` (a `BPrint` doctest that did not compile, `AVClass`
  docs that assumed `'static` classes) and 3 `wavefront`. It found no UB or equivalence
  defect, added `BPrint` allocation-failure and `ClassRef` lifetime tests, and brought
  `bprint.rs` to full Rust line coverage.
- **Symbols reviewer:** ran 2m51s before OpenAI's filter stopped it, still in its reading phase
  and having written nothing. The error was twice "This content was flagged for possible
  cybersecurity risk … apply for Daybreak access", and it exited 1. Just before, it had opened
  the wave-0 `memcpy-backptr-block-length-overflow` report (ASan `negative-size-param` and
  UBSan traces) alongside its own instructions to write UB reproducers. That attempt cost
  `$3.46` for 1.16M tokens; its log and usage record are kept under
  `review-wave-1/batch-1/attempt-0-gpt-6-astra/`, and it is in no table above.
- **Symbols re-run:** at the user's direction, on `openrouter/anthropic/claude-fable-5.1` (via
  `claude`), from the review tip after the types landing. That is the symbol row above.

Findings:

- **Equivalence drift (batch 1):** `equiv/nearest-refuses-lists-c-finishes`. `Rational::nearest`
  refused every candidate list holding two `i32::MIN` denominators. C overflows only when it
  compares that pair, which happens only when the first of them is the nearest so far.
  Rerun on `23dc07587c`: 3 lists where C returns an index and Rust `None`. On `532c537e67` all
  agree, and the C calls run clean under `asan-ubsan`. Not rejected. The fix walks C's
  loop with `cmp_distance` (C's `av_nearer_q`), so `None` now means exactly that C would
  overflow.
- **Smells and documentation:** neither reviewer moved either smell; the reviewed tip's
  unseeded scan equals the wave tip's (32 and 28), with no undocumented `unsafe`.

The reviewed tip passes `clippy --all-targets -D warnings` and the plain, `asan-ubsan`, `tsan` and
`bsan` suites: 276 unit, 275 equivalence, 254 `ub_safe` and 59 `ub_unsafe` tests under `bsan`. It was
promoted to `crustify/subcampaigns/20261005/libavutil/core`. Both reviewers wrote Conventional
Commit subjects (`fix(avutil): …`) instead of the campaign's `crustify: …`. Review cost for the
wave: `$23.44` landed, `$26.90` with the stopped attempt.
