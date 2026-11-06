# Finalize can clear str without transferring it

Affected base: `b2cff27cd691af1482bd36e8c2edab9d5e25fcf8`.

The AVBPrint.str finding said NULL occurs only when finalize hands the
allocation out. `libavutil/bprint.c:av_bprint_finalize` also calls
`av_freep(&buf->str)` for an allocated string when `ret_str == NULL`, freeing
and clearing the field. The structured nullable/ownership properties were
correct; fix the note via the accompanying submission. Inline storage is
not cleared by either finalize path. The existing Rust drop policy correctly
selects the NULL-output disposal path.
