# `ObjectMut::set` previews a `+`/`-` flag operand that C then evaluates to another value

- File: `crustify/rust/ffmpeg/libavutil/opt.rs`
- Affected commit: `cde88fc249` (options wave-0 head at review time; the
  object API landed with it, translation batch 3)
- Items: `av_opt_set` (`ObjectMut::set`), and through the same check
  `av_opt_set_from_string` (`ObjectMut::set_from_string`) and
  `av_set_options_string` (`ObjectMut::set_options_string`)
- Sanitizer: UBSan, `runtime error: 1e+30 is outside the range of
  representable values of type 'long'` at `opt.c:508:30`
- Suite: `tests/ub_unsafe/opt.rs`,
  `previewed_values::clock_reading_operands_are_refused` (and
  `set_options_string::refused_values_set_nothing`); unit tests
  `opt::unit_tests::objects::quiet_evaluation_refuses_the_clock`,
  `eval::unit_tests::clock_readers_are_found_by_name`

## Finding

For a flags option C's `set_string_number` evaluates each `+`/`-` operand
with the expression evaluator and then converts it unchecked:

```c
if (cmd == '+')
    d = intnum | (int64_t)d;          /* opt.c:508 */
else if (cmd == '-')
    d = intnum &~(int64_t)d;
```

A NaN or out-of-range `d` is undefined behaviour. The wrapper refuses those
before calling C (`flags_refusal`): it evaluates the operand itself, the
same way, through `av_expr_parse_and_eval` (`number_value`), and refuses
with `AVERROR(ERANGE)` when the value does not fit `int64_t`. The same
preview guards C's `(int) num == num` conversion of a rational whose bounds
exceed `int` (`opt.c:343`).

The preview assumes C's own evaluation, a moment later, yields the same
value. The evaluator has one function whose value changes from call to
call: `time()` (`etime`, `av_gettime() * 1e-6`). An operand such as

```
+if(gte(time(0),T),1e30,1)
```

with `T` between the two evaluations previews as 1 (in range, passed to C)
and evaluates to `1e30` in C, which converts it. The reproducer adopts a
well-formed object (one flags option) with `ObjectMut::from_ptr` within its
contract and calls the safe `set` with `T` up to half a millisecond ahead of
the clock; the first attempt that straddles `T` trips UBSan (the first
attempt already did in every run here):

```console
src/libavutil/opt.c:508:30: runtime error: 1e+30 is outside the range of representable values of type 'long'
    #0 0x7e61437d7d0e in set_string_number /target/crustify/builds/asan-ubsan/src/libavutil/opt.c:508:30
    #1 0x7e61437d42f8 in opt_set_elem /target/crustify/builds/asan-ubsan/src/libavutil/opt.c:756:16
    #2 0x7e61437d22f7 in av_opt_set /target/crustify/builds/asan-ubsan/src/libavutil/opt.c:899:12
    #3 0x583627464eb7 in <ffmpeg::libavutil::opt::ObjectMut>::set crustify/rust/ffmpeg/libavutil/opt.rs:1799:28
    #4 0x5836274612d4 in set_flag_operand_clock_race::main src/main.rs:111:17
SUMMARY: UndefinedBehaviorSanitizer: undefined-behavior src/libavutil/opt.c:508:30
$ echo $?
1
```

On x86-64 the conversion yields `INT64_MIN`, so C then rejects the flag
value with `AVERROR(ERANGE)`; the landed suites' corpora hold no operand
that calls `time()`.

## Fix

`eval::reads_clock` finds an identifier `time` in an expression once C's
white space is dropped (`t i m e(0)` included); `number_value`, the preview
both refusals use, now refuses such a token with `AVERROR(EINVAL)` as it
refuses the tokens its own quiet evaluation could reach UB on. The other
values are unaffected: an operand without `time()` previews exactly as C
evaluates it, and options that need no preview (numbers C range-checks
before converting, `int`-bounded rationals) still accept `time()`. Documented
on `number_value`.

## Reproduce

The sanitized libraries are not on the binary's rpath, so put the
build's `lib*` directories on `LD_LIBRARY_PATH`. `Cargo.lock` is seeded
from `crustify/rust/Cargo.lock`, so `--offline` resolves. UBSan recovers by
default; `main` re-runs itself with `UBSAN_OPTIONS=halt_on_error=1` and
exits with the child's status.

```console
$ B=$(git rev-parse --show-toplevel)/crustify/builds/asan-ubsan
$ CRUSTIFY_FFMPEG_VARIANT=asan-ubsan RUSTFLAGS=-Zsanitizer=address \
  cargo +nightly build --offline -Zbuild-std --target x86_64-unknown-linux-gnu
$ LD_LIBRARY_PATH=$B/libavutil:$B/libavcodec:$B/libavformat:$B/libavfilter:$B/libavdevice:$B/libswscale:$B/libswresample \
  target/x86_64-unknown-linux-gnu/debug/set-flag-operand-clock-race
```

(Frames symbolized with `UBSAN_OPTIONS=...:external_symbolizer_path=` set
to `llvm-symbolizer-18`.) On the fixed tree every attempt is refused and the
program prints `no straddle: 0 operands set, 200000 refused` and exits 0.
