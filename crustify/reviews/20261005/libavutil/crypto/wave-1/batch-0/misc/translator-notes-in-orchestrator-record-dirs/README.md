# Misc (not fixed): wave-1 translators again committed notes under `crustify/campaigns/`

- Paths, all under `crustify/campaigns/20261005/libavutil/crypto/wave-1/`:
  - `batch-0/translator-notes/validation.md` (`5f76ee7dca`)
  - `batch-1/translator-notes/validation.md` (`e71508dc3c`)
  - `batch-2-validation.md` (`71c9f8836e`), at the wave level, outside any
    batch directory
  - `batch-3/translator-notes/validation.md` (`828f6bdbf7`)
- Previous report: `../../../../wave-0/batch-0/misc/translator-notes-in-orchestrator-record-dirs`

## Finding

The wave-0 review reported that the crypto translators (Codex) committed
their validation notes into the orchestrator's record directories, where the
orchestrator later writes `batch.json`, `spawn.out` and `scan-unsafe.json`;
the orchestrator moved those notes to `wave-0/batch-<k>/translator-notes/`
when it recorded the wave. In wave 1 all four translators did it again: three
wrote straight into the `translator-notes/` name the orchestrator had used,
and batch 2 wrote `wave-1/batch-2-validation.md` beside the batch
directories, a name no orchestrator record uses. None of them collides with
an orchestrator file this time, but they still land in the orchestrator's
tree before it records the wave, and the notes repeat what the commit
messages and this review record.

## Not fixed

The directories belong to the orchestrator, which decides what the wave's
record keeps; this review neither deletes nor moves the files. When
recording the wave, `batch-2-validation.md` would need moving to
`batch-2/translator-notes/validation.md` to match the others.
