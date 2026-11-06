# Option value UB conditions: stale for `av_set_options_string`, missing for `av_opt_set`

- Entities: `av_set_options_string` (arg 0 `ctx`), `av_opt_set` (arg 2
  `val`)
- Submissions: `av_set_options_string.json`, `av_opt_set.json` (merged with
  `query symbols --update`)

## Previous finding

- `av_set_options_string.ctx`: "... Reaches C UB through av_opt_set for a
  color option whose alpha is NaN (av_parse_color, parseutils.c:433), and
  for numeric options whose min/max lie outside the field type's range
  (write_number converts after the min/max check only). ... The Rust
  wrapper is unsafe with these conditions in its contract."
- `av_opt_set.val`: "Value string parsed for the option's type and copied
  (av_strdup for strings); read for the call. NULL is accepted for some
  types." (no UB condition recorded)

## Evidence

- The UB list was incomplete. `set_string_number` converts a `+`/`-` flag
  operand with `(int64_t)d` (`opt.c:508`) and a rational with `(int) num`
  (`opt.c:343`) after evaluating them; the evaluator converts the index,
  level and operand arguments of `ld`, `st`, `random`, `randomi`, `taylor`,
  `print`, `gcd`, `bitand`, `bitor` unchecked (`eval.c:206-359`), and
  `random` seeds itself from variables `st`/`root` may set to anything;
  `av_channel_layout_from_string` overflows on an ambisonic order of
  `INT_MAX` (`channel_layout.c:341`). `ObjectMut::set` refuses all of these
  (the batch-3 translator's guards); the record names two.
- One recorded condition is not UB: a min/max outside an *integer* field's
  range makes C store `llrint(d)`, an unspecified value (C11 7.12.9.5),
  not undefined behaviour; a double stored in a `float` field is IEEE
  defined.
- Two conditions a caller must know to check the value-dependent
  conversions were unrecorded, and both let UB through the wrapper in this
  review: C's `default` constant for an array option is the bits of its
  `AVOptionArrayDef` address (`DEFAULT_NUMVAL` compares the whole type), and
  an expression calling `time()` changes value between a caller's preview
  and C's evaluation
  (`../../ub_unsafe/set-array-default-constant-mismatch`,
  `../../ub_unsafe/set-flag-operand-clock-race`, UBSan at `opt.c:508`).
- "The Rust wrapper is unsafe" is stale: this review made it the safe
  `ObjectMut::set_options_string`
  (`../../unsafe/set-options-string-raw-context`).

## Corrected finding

`av_opt_set.val` lists the UB values and the two evaluation conditions, the
non-UB integer and float stores, the uninitialised image size, and that
`ObjectMut::set` refuses the UB values. `av_set_options_string.ctx` keeps its
ownership description, refers to `av_opt_set`'s `val` for the value
conditions, and names the safe wrapper. Pointer facets are unchanged.
