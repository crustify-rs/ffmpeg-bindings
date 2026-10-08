# Wavefront: every C `union` is recorded with `kind: struct`

- Item: `av_intfloat64` (`libavutil/intfloat.h`), reviewed on branch
  `crustify/review-batches/20261005/libavutil/core/wave-0/batch-7`
- Affected commit: `3770f8f483` (the wave-0 head; the record has been wrong
  since the T1 extraction, independent of any commit in this repository)
- Tool: wavefront `ac2f85c`, `src/entities/types.ql`

## Finding

`query types --name av_intfloat64` returns `"kind": "struct"`. The header
defines a union:

```c
union av_intfloat64 {
    uint64_t i;
    double   f;
};
```

The type schema says `kind` is composer-emitted, one of `struct`, `union` or
`enum`, so this is not a finding an agent can correct through `--update`
(the update shape accepts only `fields` and `_comment_agent`). The
translator who wrapped the type noticed the mismatch and recorded it in
`_comment_agent` ("the record's kind says struct; the definition is a
union"); the ownership findings it submitted (two scalar members, no
refcount, no lock, no lifecycle routine) are correct and were kept.

The defect is systematic, not specific to this type. In the T1 table
`crustify/wavefront/codeql/t1/types.csv`:

- there is no row with `"union"` or `"union_anonymous"` at all, while the
  seven libraries define 16 named unions (`grep -rhoE "^(typedef )?union
  [A-Za-z_0-9]+ *\{"`), six of them in `libavutil` headers alone
  (`av_intfloat32`, `av_intfloat64`, `unaligned_16`, `unaligned_32`,
  `unaligned_64`, plus `mmi_intfloat64`);
- `"av_intfloat32","struct",…`, `"av_intfloat64","struct",…`,
  `"unaligned_64","struct",…` are all unions;
- `"av_alias64","typedef",…,"struct_anonymous"` is
  `typedef union { … } av_alias64` (`libavutil/intreadwrite.h:43`).

The cause is in `src/entities/types.ql`:

```ql
string kindOf(UserType t) {
  if t instanceof Struct then result = "struct"
  else if t instanceof Union then result = "union"
  ...
```

In CodeQL's C/C++ library `Union` is a subclass of `Struct` (a union is a
struct whose members share storage), so every union satisfies the first
test and the `union` branch is unreachable. `unaliasedKindColumnOf` orders
the anonymous case the same way, which is why `av_alias64` reads
`struct_anonymous`.

## Impact

A union's `kind` is what tells a type-route translator to look for a
discriminator (a tagged Rust enum) or, as here, for a punning view with no
active member. With every union labelled `struct`, that signal has to be
recovered by reading the C definition, which this batch's translator did.
The batch's wrapper, findings and field anchors are unaffected; nothing in
this repository needs changing.

## Proposed fix

Test `Union` before `Struct` in both places; `types-kind.patch` next to
this report carries the two-line reorder. It needs a re-run of `extract-ql`
to refresh `types.csv`. The wavefront checkout under
`/opt/venv/share/wavefront` has uncommitted edits of the user's, so the
patch is filed here rather than applied.
