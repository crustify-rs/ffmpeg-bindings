# Misc (not fixed): wave-0 translators committed notes into the orchestrator's record dirs

- Paths: `crustify/campaigns/20261005/libavutil/crypto/wave-0/batch-{0..10}/`
  (26 files: `validation.md`, `README.md`, `analysis.md`, `coverage.json`,
  `safety-scan.json`, `scan-unsafe.json`, `findings/*.json`,
  `trybuild-mismatches.txt`)
- Affected commits: every crypto wave-0 translation commit
  (`b76d1bb41d` .. `7935c403e0`)

## Finding

Earlier sub-campaigns' translators kept their notes out of the tree; the
`crustify/campaigns/<id>/<lu>/<sub>/wave-<n>/batch-<k>/` directories hold
only the orchestrator's records (`batch.json`, `spawn.out`,
`scan-unsafe.json`), which it commits once a wave's review is promoted. The
crypto wave-0 translators (Codex) committed their own validation notes into
those directories, and two of them (`batch-2/scan-unsafe.json`,
`batch-8/scan-unsafe.json`) sit at the exact path where the orchestrator
writes its per-batch scan, so recording the wave will overwrite them or
conflict with them. The notes duplicate what the commits and this review
record, and `batch-8/trybuild-mismatches.txt` documents the snapshot
failure fixed in `misc/trybuild-e0277-snapshot-lists-calloc-implementors`.

## Not fixed

The directories belong to the orchestrator, which decides what its wave
record keeps; this review neither deletes nor moves the files. Suggested
handling: drop them (or move them under a translator-owned path) before
committing the wave's records, and tell the Codex translators not to write
under `crustify/campaigns/`.
