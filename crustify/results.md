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
- **deps** — crustify `26365e8` (`main`)
- **build version** — `3`
- **C test baseline** — `5437/5437, 0 skipped, and no disabled tests`

## Review pass

Thin batches with `"objective": "review"`, LLM-as-a-Judge over each landed wave.

- **agent backend** — `codex`
- **model** — `openrouter/openai/gpt-5.6-sol`
- **`--billing`** — `api`
- **`--max-types`** — `2`
- **`--max-syms`** — `25`
- **`--max-loc`** — `500`
- **`--min-fields`** — `20`
- **orchestrator parallelism** — `10` concurrent batch processes
- **branch** — `<branch>`, tip `<sha>`
- **agents** — `<n>`, over `<n>` wave(s)

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
- `+UB safe tests` / `+UB unsafe tests` / `+equiv tests` / `+unit tests` — tests
  added by that landed batch; each parenthesized pair is its C/Rust
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

Implementation `<provider>/<model>` via `<backend>`; review
`<provider>/<model>` via `<backend>`.

| sub-campaign | nr types | nr symbols | sub-campaign wall | batch | review | total | $/type | $/sym |
|---|---:|---:|---|---:|---:|---:|---:|---:|
| `raw-lifetime-void` | `0` | `<n>` | `<n>m<n>s` | `$<n>` | `$<n>` | `$<n>` | — | `$<n>` |
| `raw-lifetime-string` | `0` | `<n>` | `<n>m<n>s` | `$<n>` | `$<n>` | `$<n>` | — | `$<n>` |
| `<link-unit>/<subsystem>` | `<n>` | `<n>` | `<n>h<n>m<n>s` | `$<n>` | `$<n>` | `$<n>` | `$<n>` | `$<n>` |
| orchestrator | `<n>` | `<n>` | — | — | — | `$<n>`+ | — | — |
| **Σ recorded agents** | **`<n>`** | **`<n>`** | **`<n>h<n>m`** | **`$<n>`** | **`$<n>`** | **`$<n>`** | **`$<n>`** | **`$<n>`** |

## Raw lifetime discovery

`<provider>/<model>` via `<backend>`.

| tier | strategies emitted | $ | wall |
|---|---|---|---|
| void | `<n>` | `$<n>` | `<n>m<n>s` |
| string | `<n>` | `$<n>` | `<n>m<n>s` |
| **Σ** | **`<n>`** | **`$<n>`** | **`<n>m<n>s`** |

### Review

`<provider>/<model>` via `<backend>`.

| tier | rv loc | rv $ | rv wall |
|---|---|---|---|
| void | `+<n>/-<n>` | `$<n>` | `<n>m<n>s` |
| string | `+<n>/-<n>` | `$<n>` | `<n>m<n>s` |
| **Σ** | **`+<n>/-<n>`** | **`$<n>`** | **`<n>m<n>s`** |

## Types

### Batches — wrap

`<provider>/<model>` via `<backend>`.

| wave | batch | types | fields | +LoC | +UB safe tests (+C/+Rust pp) | +UB unsafe tests (+C/+Rust pp) | +equiv tests (+C/+Rust pp) | +unit tests (+C/+Rust pp) | +unsafe fn smell | +raw-ptr smell | $ | wall | $/type | $/field |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
|`<n>`|`<n>`| `<n>` | `<n>` |`+<n>/-<n>`|`+<n>` (`+<n>`/`+<n>` pp)|`+<n>` (`+<n>`/`+<n>` pp)|`+<n>` (`+<n>`/`+<n>` pp)|`+<n>` (`+<n>`/`+<n>` pp)| `±<n>` | `±<n>` | `$<n>` | `<n>m<n>s` | `$<n>` | `$<n>` |
| **Σ** |  | **`<n>`** | **`<n>`** | **`+<n>/-<n>`** | **`+<n>` (`+<n>`/`+<n>` pp)** | **`+<n>` (`+<n>`/`+<n>` pp)** | **`+<n>` (`+<n>`/`+<n>` pp)** | **`+<n>` (`+<n>`/`+<n>` pp)** | **`±<n>`** | **`±<n>`** | **`$<n>`** | — | **`$<n>`** | **`$<n>`** |

### Batches — port

`<provider>/<model>` via `<backend>`.

| wave | batch | types | fields | +LoC | +UB safe tests (+C/+Rust pp) | +UB unsafe tests (+C/+Rust pp) | +equiv tests (+C/+Rust pp) | +unit tests (+C/+Rust pp) | +unsafe fn smell | +raw-ptr smell | $ | wall | $/type | $/field |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
|`<n>`|`<n>`| `<n>` | `<n>` |`+<n>/-<n>`|`+<n>` (`+<n>`/`+<n>` pp)|`+<n>` (`+<n>`/`+<n>` pp)|`+<n>` (`+<n>`/`+<n>` pp)|`+<n>` (`+<n>`/`+<n>` pp)| `±<n>` | `±<n>` | `$<n>` | `<n>m<n>s` | `$<n>` | `$<n>` |
| **Σ** |  | **`<n>`** | **`<n>`** | **`+<n>/-<n>`** | **`+<n>` (`+<n>`/`+<n>` pp)** | **`+<n>` (`+<n>`/`+<n>` pp)** | **`+<n>` (`+<n>`/`+<n>` pp)** | **`+<n>` (`+<n>`/`+<n>` pp)** | **`±<n>`** | **`±<n>`** | **`$<n>`** | — | **`$<n>`** | **`$<n>`** |

### Batches — review

`<provider>/<model>` via `<backend>`.

| wave | batch | types | +LoC | +UB safe tests (+C/+Rust pp) | +UB unsafe tests (+C/+Rust pp) | +equiv tests (+C/+Rust pp) | +unit tests (+C/+Rust pp) | +unsafe fn smell | +raw-ptr smell | rv loc | rv $ | rv wall | rv $/type |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
|`<n>`|`<n>`| `<n>` |`+<n>/-<n>`|`+<n>` (`+<n>`/`+<n>` pp)|`+<n>` (`+<n>`/`+<n>` pp)|`+<n>` (`+<n>`/`+<n>` pp)|`+<n>` (`+<n>`/`+<n>` pp)| `±<n>` | `±<n>` | `+<n>/-<n>` | `$<n>` | `<n>m<n>s` | `$<n>` |
| **Σ** |  | **`<n>`** | **`+<n>/-<n>`** | **`+<n>` (`+<n>`/`+<n>` pp)** | **`+<n>` (`+<n>`/`+<n>` pp)** | **`+<n>` (`+<n>`/`+<n>` pp)** | **`+<n>` (`+<n>`/`+<n>` pp)** | **`±<n>`** | **`±<n>`** | **`+<n>/-<n>`** | **`$<n>`** | — | **`$<n>`** |

### Batches — review: PoC + reports

`<provider>/<model>` via `<backend>`.

| wave | batch | +lifecycle | +unsafe | +UB safe | +UB unsafe | +equiv | +internal | +conventions | +misc | total | +rejected PoCs |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `<n>` | `<n>` | `+<n>` | `+<n>` | `+<n>` | `+<n>` | `+<n>` | `+<n>` | `+<n>` | `+<n>` | `<n>` | `<n>` |
| **Σ** |  | **`+<n>`** | **`+<n>`** | **`+<n>`** | **`+<n>`** | **`+<n>`** | **`+<n>`** | **`+<n>`** | **`+<n>`** | **`<n>`** | **`<n>`** |

## Symbols

### Batches

`<provider>/<model>` via `<backend>`.

| wave | batch | objective | symbols | loc | +LoC | +UB safe tests (+C/+Rust pp) | +UB unsafe tests (+C/+Rust pp) | +equiv tests (+C/+Rust pp) | +unit tests (+C/+Rust pp) | +unsafe fn smell | +raw-ptr smell | $ | wall | $/symbol | $/loc |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
|`<n>`|`<n>`| wrap | `<n>` | `<n>` |`+<n>/-<n>`|`+<n>` (`+<n>`/`+<n>` pp)|`+<n>` (`+<n>`/`+<n>` pp)|`+<n>` (`+<n>`/`+<n>` pp)|`+<n>` (`+<n>`/`+<n>` pp)| `±<n>` | `±<n>` | `$<n>` | `<n>m<n>s` | `$<n>` | `$<n>` |
|`<n>`|`<n>`| port | `<n>` | `<n>` |`+<n>/-<n>`|`+<n>` (`+<n>`/`+<n>` pp)|`+<n>` (`+<n>`/`+<n>` pp)|`+<n>` (`+<n>`/`+<n>` pp)|`+<n>` (`+<n>`/`+<n>` pp)| `±<n>` | `±<n>` | `$<n>` | `<n>m<n>s` | `$<n>` | `$<n>` |
| **Σ** |  |  | **`<n>`** | **`<n>`** | **`+<n>/-<n>`** | **`+<n>` (`+<n>`/`+<n>` pp)** | **`+<n>` (`+<n>`/`+<n>` pp)** | **`+<n>` (`+<n>`/`+<n>` pp)** | **`+<n>` (`+<n>`/`+<n>` pp)** | **`±<n>`** | **`±<n>`** | **`$<n>`** | — | **`$<n>`** | **`$<n>`** |

### Batches — review

`<provider>/<model>` via `<backend>`.

| wave | batch | symbols | +LoC | +UB safe tests (+C/+Rust pp) | +UB unsafe tests (+C/+Rust pp) | +equiv tests (+C/+Rust pp) | +unit tests (+C/+Rust pp) | +unsafe fn smell | +raw-ptr smell | rv loc | rv $ | rv wall | rv $/symbol |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
|`<n>`|`<n>`| `<n>` |`+<n>/-<n>`|`+<n>` (`+<n>`/`+<n>` pp)|`+<n>` (`+<n>`/`+<n>` pp)|`+<n>` (`+<n>`/`+<n>` pp)|`+<n>` (`+<n>`/`+<n>` pp)| `±<n>` | `±<n>` | `+<n>/-<n>` | `$<n>` | `<n>m<n>s` | `$<n>` |
| **Σ** |  | **`<n>`** | **`+<n>/-<n>`** | **`+<n>` (`+<n>`/`+<n>` pp)** | **`+<n>` (`+<n>`/`+<n>` pp)** | **`+<n>` (`+<n>`/`+<n>` pp)** | **`+<n>` (`+<n>`/`+<n>` pp)** | **`±<n>`** | **`±<n>`** | **`+<n>/-<n>`** | **`$<n>`** | — | **`$<n>`** |

### Batches — review: PoC + reports

`<provider>/<model>` via `<backend>`.

| wave | batch | +lifecycle | +unsafe | +UB safe | +UB unsafe | +equiv | +internal | +conventions | +misc | total | +rejected PoCs |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `<n>` | `<n>` | — | `+<n>` | `+<n>` | `+<n>` | `+<n>` | `+<n>` | `+<n>` | `+<n>` | `<n>` | `<n>` |
| **Σ** |  | — | **`+<n>`** | **`+<n>`** | **`+<n>`** | **`+<n>`** | **`+<n>`** | **`+<n>`** | **`+<n>`** | **`<n>`** | **`<n>`** |

## Safety audit

Deterministic `crustify <workdir> audit unsafe`; no model.

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
