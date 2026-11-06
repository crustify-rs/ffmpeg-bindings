# `ObjectMut::set` previews an array option's `default` as 0; C reads the definition's address

- File: `crustify/rust/ffmpeg/libavutil/opt.rs`
- Affected commit: `cde88fc249` (options wave-0 head at review time;
  translation batch 3)
- Items: `av_opt_set` (`ObjectMut::set`), and through the same check
  `av_opt_set_from_string` and `av_set_options_string`
- Sanitizer: UBSan, `runtime error: 1e+30 is outside the range of
  representable values of type 'long'` at `opt.c:508:30` (the same preview
  guards `(int) num` of a wide rational array at `opt.c:343`)
- Suite: `tests/ub_unsafe/opt.rs`,
  `previewed_values::array_defaults_preview_as_c_evaluates_them`; unit test
  `opt::unit_tests::objects::array_defaults_read_the_definition_address`

## Finding

C evaluates a numeric option value with the constants `default`, `max`,
`min`, `none` and `all`; `default` is

```c
#define DEFAULT_NUMVAL(opt) ((opt->type == AV_OPT_TYPE_INT64 || \
                              opt->type == AV_OPT_TYPE_UINT64 || \
                              opt->type == AV_OPT_TYPE_CONST || \
                              opt->type == AV_OPT_TYPE_FLAGS || \
                              opt->type == AV_OPT_TYPE_UINT  || \
                              opt->type == AV_OPT_TYPE_INT)     \
                             ? opt->default_val.i64             \
                             : opt->default_val.dbl)
```

For an element of an array option (`opt_set_array` passes the array's own
`AVOption`), `opt->type` is `FLAGS | AV_OPT_TYPE_FLAG_ARRAY`, which equals
none of the integer types, so C reads `default_val.dbl`, whose bytes are
the `AVOptionArrayDef *` C keeps in the union: a non-zero subnormal
whenever the option has a definition.

The wrapper previews each `+`/`-` flag operand (and the value of a rational
with wide bounds) to refuse the ones C converts with undefined behaviour
(`flags_refusal`, `number_value`). Its `default_number` mapped the array
option's `OptionDefault::Array` to 0. So the operand
`+1e30*not(not(default))` previews as `1e30 * not(not(0)) = 0`, in range,
and C evaluates it to `1e30 * not(not(<subnormal>)) = 1e30` and converts it
(`opt.c:508`). The reproducer's class has one flags array option with a
definition (no default, `,` separator), a valid table under `ObjectRef`'s
contract; the object is adopted with `ObjectMut::from_ptr` and the safe
`set` called once:

```console
src/libavutil/opt.c:508:30: runtime error: 1e+30 is outside the range of representable values of type 'long'
    #0 0x70ea6b600d0e in set_string_number /target/crustify/builds/asan-ubsan/src/libavutil/opt.c:508:30
    #1 0x70ea6b5fd2f8 in opt_set_elem /target/crustify/builds/asan-ubsan/src/libavutil/opt.c:756:16
    #2 0x70ea6b5fc9fc in opt_set_array /target/crustify/builds/asan-ubsan/src/libavutil/opt.c:858:15
    #3 0x70ea6b5fb2f7 in av_opt_set /target/crustify/builds/asan-ubsan/src/libavutil/opt.c:899:12
    #4 0x6271feb09b77 in <ffmpeg::libavutil::opt::ObjectMut>::set crustify/rust/ffmpeg/libavutil/opt.rs:1799:28
    #5 0x6271feb07ebc in set_array_default_constant_mismatch::main src/main.rs
SUMMARY: UndefinedBehaviorSanitizer: undefined-behavior src/libavutil/opt.c:508:30
$ echo $?
1
```

The fixture's arrays (`array`, `strings`, `empty`) are of `int` and string
elements, which need no preview, so no landed test reached it.

## Fix

`default_number` gives an array option C's value: the bits of its
definition's address as a double (`f64::from_bits(addr)`), 0 for none.
The preview then evaluates `default` as C does; the operand above is
refused with `AVERROR(ERANGE)`, and in-range operands that use `default`
store what the preview saw (`+not(not(default))+4,8` is `5,8`).

## Reproduce

As `../set-flag-operand-clock-race/README.md`, with the binary
`set-array-default-constant-mismatch`. On the fixed tree it prints
`set: Err(-34)` and exits 0.
