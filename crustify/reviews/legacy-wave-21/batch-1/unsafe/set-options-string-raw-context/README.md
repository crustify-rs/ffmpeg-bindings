# `set_options_string` kept a raw `*mut c_void` context after `ObjectMut` landed

- File: `crustify/rust/ffmpeg/libavutil/opt.rs`
- Affected commit: `cde88fc249`
- Item: `av_set_options_string`
- Smell: `crustify scan-unsafe` `unsafe_fns_pub_smell`, `raw_ptr_pub_smell`,
  `void_ptr_pub_smell` at `opt.rs:1175`

## Finding

`av_set_options_string` landed with translation batch 2 (`ae14a85553`),
before the object API existed, as

```rust
pub unsafe fn set_options_string(ctx: *mut c_void, opts: &CStr,
    key_val_sep: &CStr, pairs_sep: &CStr) -> Result<usize, i32>
```

documenting `ctx` as "a raw seam while `AVClass`-enabled objects have no
safe handle". Batch 3 (`cde88fc249`, same wave) added that handle,
`ObjectMut`, whose safe setters (`set`, `set_from_string`) refuse the values
C parses with undefined behaviour. `set_options_string` was not moved onto
it: every caller still had to adopt a raw object and discharge a
four-clause `# Safety` contract, of which the third ("each value must be
one `av_opt_set` stores without undefined behaviour") asked callers to know
C's unchecked conversions, a check `ObjectMut::set` already performs. The
contract was also incomplete for exactly that reason: it named the NaN
opacity and out-of-range bounds, but not a `+`/`-` flag operand or a wide
rational outside `int`, an evaluator index such as `ld(1e300)`, or an
ambisonic order of `INT_MAX`, all of which `ObjectMut::set` refuses. The
seam was not a cut SCC or a higher-layer dependency: the dependency was in
the same module.

## Fix

`set_options_string` is now the safe method
`ObjectMut::set_options_string(&mut self, opts, key_val_sep, pairs_sep)`.
Before C runs it reads the pairs as `parse_key_value_pair` does
(`av_get_token` for the key and value, one separator byte skipped after
each) and refuses the list with `set`'s error when any pair's value is one
`set` refuses for that key with `AV_OPT_SEARCH_CHILDREN`
(`options_refusal`), so no pair is set; otherwise C sets the pairs as
before. The NULL / classless object cases of the old contract fall away (a
`BareObject` is the object with no options; every key is not found, as
before). The `ub_unsafe` suite now adopts the fixture's well-formed
`OBJ_CLASS` objects (the old one used the test class whose undefined option
type `ObjectRef`'s contract excludes), and the `equiv` module compares the
method with the raw call on pairs of `OBJ_CLASS` objects with children.
