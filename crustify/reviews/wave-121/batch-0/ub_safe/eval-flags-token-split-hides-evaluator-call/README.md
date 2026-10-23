# `OptionDefRef::eval_flags` misses an evaluator call hidden by C's 255-byte token cut: UB

- File: `crustify/rust/ffmpeg/libavutil/opt.rs` (`flags_refusal`, reached
  through `elem_refusal`)
- Affected commit: `abf526dc98` (options wave-2 tip at review time, its one
  translation batch); the setter half of the path is options wave 0's
- Items: `av_opt_eval_flags` (`OptionDefRef::eval_flags`, a safe API); the
  same refusal guards `av_opt_set` and the key-value appliers
  (`ObjectMut::set`, `set_options_string`, `set_from_string`,
  `set_from_dict`) for flags options
- Sanitizer: UBSan `float-cast-overflow`, `eval.c:206:57: runtime error:
  1e+300 is outside the range of representable values of type 'int'`
  (`eval.c:205` in the source tree; the sanitized build's copy has one more
  line)
- Suites: `tests/ub_safe/opt.rs::flag_token_split::cut_tokens_are_refused_on_their_own`
  (this reproducer, through the safe API),
  `tests/ub_unsafe/opt.rs::lookups::cut_flag_tokens_are_refused_by_evaluators_and_setters`
  (the evaluators in both contexts and the setters that share the refusal),
  `tests/equiv/opt.rs::lookups::long_flag_values_are_cut_and_refused_as_tokens`
  (long values C evaluates safely agree with C after every call; the cut
  ones are refused)

## Finding

C's `set_string_number` evaluates an unsigned flags value token by token
(`opt.c:451-458`):

```c
        if (type == AV_OPT_TYPE_FLAGS) {
            if (*val == '+' || *val == '-')
                cmd = *(val++);
            for (; i < sizeof(buf) - 1 && val[i] && val[i] != '+' && val[i] != '-'; i++)
                buf[i] = val[i];
            buf[i] = 0;
        }
```

A token ends at the next `+` / `-` *or after 255 bytes*, and each token is
looked up as a named constant or parsed by `av_expr_parse_and_eval` as an
expression of its own. The wrapper's refusal of expressions C evaluates with
undefined behaviour (`eval::refuses`, which finds `ld(`, `st(`, `taylor(`,
`print(`, ... calls and checks their arguments) ran once over the whole
value, and per token only after a `+` or `-` (through `number_value`). A
token that starts after a cut is neither: it can begin inside what the whole
value reads as one name or number. With `0*` and 253 zeros (one 255-byte
token, value 0) followed by `ld(1e300)`, the whole value scans as the
identifier `000…0ld`, which starts with a digit and so is a number, and
`(1e300)`: nothing to refuse. C evaluates the first token, writes 0, then
evaluates `ld(1e300)` alone, and `ld` converts its argument to an `int` for
`av_clip` (`eval.c:205`, `p->var[av_clip(eval_expr(p, e->param[0]), 0,
VARS-1)]`) unchecked: undefined behaviour for `1e300`.

`eval_flags` is safe and takes a class context: `ClassRef` handles of C's
own classes reach safe code (here the `cpuflags` class from a log record),
so no unsafe code is needed. `ObjectMut::set` and the appliers refuse
through the same `elem_refusal` / `flags_refusal` and were equally exposed
on any flags option of an adopted object.

## Reproducer

`src/main.rs` (`#![forbid(unsafe_code)]`) installs a log callback, makes
`cpu::parse_cpu_caps` fail so that C logs in the `cpuflags` class's name,
finds that class's `flags` option in the callback and calls
`eval_flags(class, value, &mut flags)` with the value above. UBSan recovers
by default, so `main` re-runs itself with `UBSAN_OPTIONS=halt_on_error=1`.

```console
$ CARGO_TARGET_DIR=/tmp/t CRUSTIFY_FFMPEG_VARIANT=asan-ubsan RUSTFLAGS=-Zsanitizer=address \
    cargo +nightly build -Zbuild-std --target x86_64-unknown-linux-gnu --offline
$ B=/target/crustify/builds/asan-ubsan
$ LD_LIBRARY_PATH=$(printf '%s:' $B/lib*) \
    /tmp/t/x86_64-unknown-linux-gnu/debug/eval-flags-token-split-hides-evaluator-call; echo "exit=$?"
```

On the affected commit (exit 1):

```text
src/libavutil/eval.c:206:57: runtime error: 1e+300 is outside the range of representable values of type 'int'
    #0 in eval_expr /target/crustify/builds/asan-ubsan/src/libavutil/eval.c:206:57
    #1 in av_expr_eval /target/crustify/builds/asan-ubsan/src/libavutil/eval.c:836:12
    #2 in av_expr_parse_and_eval /target/crustify/builds/asan-ubsan/src/libavutil/eval.c:852:10
    #3 in set_string_number /target/crustify/builds/asan-ubsan/src/libavutil/opt.c:497:23
    #4 in <ffmpeg::libavutil::opt::OptionDefRef>::eval::<i32> crustify/rust/ffmpeg/libavutil/opt.rs:4223:28
    #5 in <ffmpeg::libavutil::opt::OptionDefRef>::eval_flags::<ffmpeg::libavutil::log::ClassRef> crustify/rust/ffmpeg/libavutil/opt.rs:4079:14
    #6 in <eval_flags_token_split_hides_evaluator_call::Probe as ffmpeg::libavutil::log::Callback>::log src/main.rs:58:26
    #7 in ffmpeg::libavutil::log::trampoline::<eval_flags_token_split_hides_evaluator_call::Probe> crustify/rust/ffmpeg/libavutil/log.rs:1408:5
    #8 in av_log /target/crustify/builds/asan-ubsan/src/libavutil/log.c:446:5
    #9 in set_string_number /target/crustify/builds/asan-ubsan/src/libavutil/opt.c:500:21
    #10 in av_parse_cpu_caps /target/crustify/builds/asan-ubsan/src/libavutil/cpu.c:225:12
    #11 in ffmpeg::libavutil::cpu::token_flags crustify/rust/ffmpeg/libavutil/cpu.rs:182:24
    #12 in ffmpeg::libavutil::cpu::parse_cpu_caps crustify/rust/ffmpeg/libavutil/cpu.rs:156:21
    #13 in eval_flags_token_split_hides_evaluator_call::main src/main.rs:75:13
    ...
SUMMARY: UndefinedBehaviorSanitizer: undefined-behavior src/libavutil/eval.c:206:57
```

On the fixed tree it prints `eval_flags: Err(-22), flags 0x0` and exits 0.
BSan was not run on it: the defect is a float-to-int conversion, which BSan
does not instrument.

## Fix

`flags_refusal` now checks every token it splits, exactly as C splits them,
with `eval::refuses(token, 0)` (C evaluates each token at level offset 0)
and refuses with `AVERROR(EINVAL)`, before the `+` / `-` operand check.
Refusing a later token that C would never reach (it stops at the first
failing token) is conservative: the value is refused as a whole, as the
other refusals already do. The `ObjectMut::set` and evaluator docs now say
that for flags every token C evaluates on its own is checked.
