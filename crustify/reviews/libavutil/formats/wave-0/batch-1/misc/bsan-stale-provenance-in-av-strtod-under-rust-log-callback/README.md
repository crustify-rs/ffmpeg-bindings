# BSan gate: `av_strtod` "freed allocation" report while `equiv/imgutils.rs` holds a Rust log callback

- Files: `crustify/rust/ffmpeg/tests/equiv/imgutils.rs`;
  `crustify/rust/ffmpeg/tests/ub_safe/log.rs` for the same pattern (below)
- Affected commit: `17c012afcd` (formats wave-0 head; the gate log is
  `crustify/artifacts/libavutil/formats/review-wave-0/bsan-gate.log`)
- Items: `check_size`, `check_size_with_context` (`Wraps: av_image_check_size`);
  the test `check_size_logs_as_c` compares the lines C logs
- Instrument: BSan only. ASan/UBSan, TSan and the plain build pass the same
  tests; `--test-threads=1` passes under BSan.
- Verdict: a sanitizer artifact, not undefined behaviour in the wrapped code.
  The flagged read is one byte inside a live heap block, through a pointer
  that uninstrumented libc produced. Fixed by running the callback-capturing
  test alone in a child process, so no other suite logs through its callback.

## Finding

The wave's BSan gate failed in the `equiv` binary with

```
error: Undefined Behavior: trying to access an allocation that has been freed.
   --> /target/crustify/builds/bsan/src/libavutil/eval.c:120:13
120 | if (next[0] == 'd' && next[1] == 'B') {
stack backtrace:
0: av_strtod
```

Reproduced in ~1 s on the two-test pair; it is deterministic for the two
`cpu::parse_caps` tests that walk the whole `STRINGS` table and for no other
`cpu` test, and for `imgutils::check_size_logs_as_c` and no other `imgutils`
test (`cpu::` alone passes three times out of three):

```console
$ BSAN_OPTIONS=gc_hang_threshold_ms=600000 CRUSTIFY_FFMPEG_VARIANT=bsan \
  cargo bsan test --test equiv -- --exact \
  cpu::parse_caps::single_strings_match_c_from_several_starting_words \
  imgutils::check_size_logs_as_c
```

`gdb` at the runtime's `exit_group` (BSan exits rather than aborts) gives the
full stack: the read is on the `cpu` test's thread, under
`ffmpeg::libavutil::cpu::parse_cpu_caps` → `token_flags` →
`av_parse_cpu_caps` → `av_opt_eval_flags` → `set_string_number` →
`av_expr_parse_and_eval` → `av_expr_parse` → `parse_expr` → … →
`parse_primary` → `av_strtod("1e3")`. `imgutils` code is not on that stack;
the only link between the two tests is the process-wide log callback that
`check_size_logs_as_c` installs through `log::set_callback::<Capture>()`
while it runs (`cpu`'s "Undefined constant or missing '('" messages then go
through the Rust trampoline on `cpu`'s thread, where `Capture::log` finds no
capture buffer and returns).

The access itself is valid:

```
#3  av_strtod (numstr=0x704000002000 "1e3", tail=0x7bfff41f9920) at eval.c:120
$1 = numstr = 0x704000002000 "1e3"
$2 = next   = 0x704000002003 ""          # strtod consumed the whole token
x/4xb numstr: 0x31 0x65 0x33 0x00
#11 av_expr_parse (...) at eval.c:766
$4 = w   = 0x704000002000 "1e3"          # av_malloc(strlen(s) + 1) == 4 bytes
$5 = p.s = 0x704000002000 "1e3"
```

`next[0]` is byte 3 of the 4-byte block `w` that `av_expr_parse` allocated
and frees only after `parse_expr` returns (`eval.c:794`); the frame is still
inside `parse_expr`. The pointer `next` is written by glibc's `strtod`
through `char **endptr`, which BSan does not instrument, so the provenance
BSan reads back for it is whatever its metadata holds for that stack slot or
that address, not the block's. The reported "freed allocation" therefore
describes metadata, not memory.

Experiments on the same binary (each rebuilt, each run three times,
deterministic):

| variant of `equiv/imgutils.rs` | BSan |
|---|---|
| as landed | reports |
| `captured` runs `f()` without installing any callback | passes |
| callback installed, `Capture::log` returns at once (env gate) | passes |
| callback installed, full capture, plus an `env::var_os` call at the top of `Capture::log` and one assertion dropped | passes |
| callback kept installed for the whole test, body looped 20× | reports |
| as landed, under a gdb hardware watchpoint (slows the `cpu` thread) | passes |

So the report comes and goes with changes that neither touch the flagged
memory nor the `cpu` wrapper: an extra libc call in the callback or a
debugger stop is enough. A standalone program that installs the same
`Capture` from a second thread while the main thread replays the `cpu`
test's exact loop (`av_parse_cpu_caps` and `parse_cpu_caps` over `STRINGS`
from the same five starting words) runs clean, as do the variants with a
no-op Rust callback, a C-ABI no-op callback, and plain heap churn on the
other thread. The trigger needs the libtest thread layout and timing.

This matches the runtime's known limits recorded in the campaign notes
(dead-frame provenance for pointers that pass through uninstrumented code).
It is the `log` module's trampoline that runs on the other threads, used
exactly as `tests/ub_safe/log.rs` and `tests/equiv/log.rs` use it; those
suites happen not to collide with `cpu` in the gate.

## Fix

`tests/equiv/imgutils.rs::check_size_logs_as_c` now re-executes the test
binary with `--exact imgutils::check_size_logs_as_c --test-threads=1` (the
`isolated` pattern the inline `imgutils` and `pixelutils` unit tests already
use for `av_force_cpu_flags`) and runs its body only in that child. No other
test can log through the capturing callback, so no sibling thread runs the
Rust trampoline, under any instrument. The test still runs under BSan, in
the child, and still compares every line and return value against C.

With the fix, the pair passes under BSan three times out of three, the
`cpu:: imgutils::` 14-test run passes, and the whole crate passes under the
plain build, ASan/UBSan, TSan and BSan (see the review commit message).

## The same pattern in `ub_safe`

The pattern is not specific to `equiv/imgutils.rs`. On the rebased tree
(`71419a9128`, the wave head after the types review, plus this batch's
commits) the `ub_safe` binary reported the same line three times out of
three, within its first two minutes, and `ub_safe cpu:: log::` alone on the
second run. `ub_safe`'s `imgutils` tests install no callback: there, core's
`tests/ub_safe/log.rs` installs Rust callbacks under its own `LogState`
lock (`records_format_into_exact_size_buffers`,
`callbacks_reenter_logging_and_swap_themselves`,
`logging_races_with_callback_level_and_flag_changes`,
`class::the_cpuflags_class_reaches_a_callback`) while `ub_safe/cpu.rs`'s
`parse_caps` tests reach `av_strtod` through `av_parse_cpu_caps`. The gate
had passed that binary at `17c012afcd`; the types review's `Default`
impls shifted the timing. Those four tests now run alone in a child through
the same `isolated` helper, beside the binary's existing `ISOLATED` marker
that `default_callback_takes_every_level_flag_and_colour_mode` already used
for its colour modes. With that, `ub_safe cpu:: log::` passed every run of
a loop under BSan and the whole binary passed (see the commit message).

`tests/equiv/log.rs` (a C-ABI capture plus `tests/common/class.rs::with_class`)
and `tests/ub_unsafe/log.rs` also install callbacks while `cpu` and `opt`
tests run; they passed the gate and every run of this review and are left
as they are. Should the report recur in one of them, the same isolation
applies; running the BSan gate with `--test-threads=1` (which the
orchestrator found passes) is the other option.

## Reproduce on the affected commit

```console
$ git checkout 17c012afcd -- crustify/rust/ffmpeg/tests/equiv/imgutils.rs
$ cd crustify/rust
$ BSAN_OPTIONS=gc_hang_threshold_ms=600000 CRUSTIFY_FFMPEG_VARIANT=bsan \
  cargo bsan test --test equiv -- --exact \
  cpu::parse_caps::single_strings_match_c_from_several_starting_words \
  imgutils::check_size_logs_as_c
error: Undefined Behavior: trying to access an allocation that has been freed.
   --> /target/crustify/builds/bsan/src/libavutil/eval.c:120:13
$ echo $?
1
```

For the stack, catch the runtime's exit under gdb:

```console
$ BSAN_SYMBOLIZER=/root/.rustup/toolchains/bsan/bin/llvm-symbolizer gdb -batch \
  -ex 'catch syscall exit_group' -ex run -ex 'bt 20' --args \
  <target>/bsan/x86_64-unknown-linux-gnu/debug/build/ffmpeg/*/out/equiv-* \
  --exact cpu::parse_caps::single_strings_match_c_from_several_starting_words \
  imgutils::check_size_logs_as_c --test-threads=2
```
