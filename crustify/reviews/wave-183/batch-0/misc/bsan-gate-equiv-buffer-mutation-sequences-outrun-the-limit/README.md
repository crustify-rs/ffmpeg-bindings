# Misc: one buffer equivalence test keeps the BSan gate's `equiv` binary past two hours

- Test: `tests/equiv/buffer.rs::mutation::sequences_mutate_count_and_release_as_c`
  (buffer-frame wave 2, batch 1).
- Kind: test cost under BorrowSanitizer. This is not a code defect, and BSan
  reported nothing.
- Status: thinned under `CRUSTIFY_FFMPEG_VARIANT=bsan`. Plain, ASan/UBSan and
  TSan still run all 16 seeds.

## What the gate saw

This review ran the sub-campaign's BSan gate: one invocation per test
binary, in parallel, each under `timeout 7200`, with the 48 known `strto*`
false positives skipped (`../../../../options/wave-2/batch-0/misc/bsan-gate-strto-end-pointer-false-positives/skip-list.txt`).

On the wave tip `2453c27798`, `lib`, `ub_safe`, `ub_unsafe` and the seven
`-sys` binaries passed. `equiv` was still running when the two-hour limit
stopped it. Every one of its 656 selected tests had reported `ok` except
this one. It was the binary's only thread left, and it had used about 2600
CPU seconds at that point. The first run of the landed tree showed the same.

Run alone in the BSan build with one seed instead of 16, the test takes 781
s. All 16 seeds would take about 3.5 hours. Each of the 200 steps per seed
compares a snapshot of every held reference on both sides (each window's
bytes copied out, its count, its writability, its opaque), and the number of
references and buffers grows through the sequence. That is the per-retag
cost class the campaign's BSan notes describe. Plain and ASan runs take
0.13 s.

## Change

Under BSan the test now runs seed 2 only (`seeds()`). The mutation floor
scales with the seed count (more than 50 mutating steps per seed, 800 for
16 as before), and so does the path floor (more than 10 each for 16 seeds,
more than 0 for one). Seed 2 alone reaches all three C paths the test
counts: 41 copies, 5 in-place resizes and 1 same-buffer assignment. Seed 1
reaches no same-buffer assignment, so it would fail the floor. That run
failed in 781 s, which is how the floor was found.

Gate figures with the change are in the batch README.
