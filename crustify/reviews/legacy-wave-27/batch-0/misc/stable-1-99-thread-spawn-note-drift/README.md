# Misc: two older trybuild snapshots predate rustc 1.99's `thread::spawn` note

- Files: `crustify/rust/ffmpeg/tests/ub_safe/compile_fail/{intfloat_mut_handle_sent_past_value,intreadwrite_view_sent_past_bytes}.stderr`
- Affected commit: present at the wave base `6daed536cb` and the tip
  `c45fd5b3d1` under the installed stable `rustc 1.99.0 (b940084d7 2026-09-28)`
- Items: none of this workset (core wave `intfloat` / `intreadwrite` cases);
  fixed because the review's `cargo test --workspace` gate cannot pass otherwise

## Finding

Both E0597 cases still fail to compile as intended, but rustc 1.99 appends
the source excerpt of the `'static` bound to its note
(`--> $RUST/std/src/thread/functions.rs` followed by `F: Send + 'static,`
and a caret line), so the snapshots mismatch and the `ub_safe` binary fails.
Every crypto wave-0 translator recorded the two mismatches as
"pre-existing" and skipped `compile_fail` instead of re-blessing.

## Fix

The three added diagnostic lines are appended to each `.stderr`; nothing
else changes (checked against trybuild's ACTUAL OUTPUT, which is identical
to the old snapshot up to those lines). The `compile_fail` test passes on
stable 1.99 again.
