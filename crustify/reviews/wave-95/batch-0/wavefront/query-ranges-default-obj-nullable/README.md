# Wavefront: `av_opt_query_ranges_default.obj` was recorded as non-nullable

- Store: `crustify/wavefront/ownership-store.json`
- Affected commit: `ab1d2518bf`
- Entry: symbol `av_opt_query_ranges_default` (`libavutil/opt.c`),
  `ptr_args` position 1 (`obj`)

## Finding

The previous run submitted `obj` with `nullable: false` and the note "only
read for the call (av_opt_find)", copied from `av_opt_query_ranges`, whose
`obj` really is dereferenced unconditionally:

```c
int av_opt_query_ranges(AVOptionRanges **ranges_arg, void *obj, const char *key, int flags)
{
    const AVClass *c = *(AVClass**)obj;
    int (*callback)(...) = c->query_ranges;
```

`av_opt_query_ranges_default` only passes `obj` to `av_opt_find`, and
`av_opt_find2` returns NULL for a NULL object before reading it:

```c
int av_opt_query_ranges_default(AVOptionRanges **ranges_arg, void *obj, const char *key, int flags)
{
    ...
    const AVOption *field = av_opt_find(obj, key, NULL, 0, flags);
    ...
    *ranges_arg = NULL;
    if (!ranges || !range || !range_array || !field) {
        ret = AVERROR(ENOMEM);
        goto fail;

const AVOption *av_opt_find2(void *obj, const char *name, const char *unit,
                             int opt_flags, int search_flags, void **target_obj)
{
    if(!obj)
        return NULL;
    c= *(AVClass**)obj;
    if (!c)
        return NULL;
```

So NULL is an accepted value of `obj` that fails the query with
`AVERROR(ENOMEM)`, the same way a NULL class pointer does.
`tests/equiv/opt.rs::null_object_agrees_with_the_default_query` executes the
raw C routine with a NULL `obj` for every flag combination and observes
`AVERROR(ENOMEM)` with a NULL out slot, which the finding said could not
happen. The over-strict finding propagated into the wrapper's `# Safety`
contract (see `../../unsafe/query-default-contract-rejects-null-object`).

## Fix

Resubmitted through `wavefront query symbols --name av_opt_query_ranges_default --update`
with `obj.nullable: true` and a note that states the path (`av_opt_find` ->
`av_opt_find2` returns NULL for a NULL `obj` or class pointer, so the query
fails with `AVERROR(ENOMEM)` instead of dereferencing it), contrasting it
with `av_opt_query_ranges`. The `ranges_arg` note now also says NULL is
stored on *every* failure, before the lookup. `av_opt_query_ranges`'s entry
is unchanged. The submission JSON is reproduced below so it can be reapplied
verbatim after a store merge.

```json
{
  "ptr_args": {
    "0": {"ptr": {"string": false, "scalar": {"by_ref": {"owned": true, "borrowed": null}}, "array": null,
                  "owned": false, "borrowed": {"lifetime": "other"}, "mutable": true, "nullable": false,
                  "note": "Out-parameter: receives a fresh owned AVOptionRanges on success (av_opt_freep_ranges releases it); NULL is stored on every failure, before the lookup."}},
    "1": {"ptr": {"string": false, "scalar": {"by_val": true}, "array": null,
                  "owned": false, "borrowed": {"lifetime": "other"}, "mutable": false, "nullable": true,
                  "note": "AVClass-enabled object (first member a const AVClass *), or with AV_OPT_SEARCH_FAKE_OBJ a const AVClass **; only read for the call, through av_opt_find -> av_opt_find2, which returns NULL for a NULL obj or a NULL class pointer, so the query then fails with AVERROR(ENOMEM) instead of dereferencing it (unlike av_opt_query_ranges, which reads *(AVClass **)obj unconditionally). The result borrows nothing from it."}},
    "2": {"ptr": {"string": true, "scalar": null, "array": null,
                  "owned": false, "borrowed": {"lifetime": "other"}, "mutable": false, "nullable": false,
                  "note": "Option name, read for the call only (strcmp in av_opt_find2)."}}
  },
  "ptr_ret": null,
  "lifetime": {"for": "ranges_arg", "is_dropper": false, "is_disposer": false, "is_cloner": null,
               "is_constructor": {"alloc": true, "init": false}}
}
```

## Findings re-checked and left as they are

- `AVOption`: every field's string / `arr` pointer is a borrow of program
  lifetime into an immutable table; no C path allocates, writes or frees an
  option (no `sizeof(AVOption)` allocation in the libraries; `fftools` and
  `doc/print_options.c` are out of scope). The `default_val` member selection
  by `type` matches every reader in `opt.c` (`DEFAULT_NUMVAL`, `read_number`,
  `av_opt_set_defaults2`, `av_opt_is_set_to_default`, `log_default`), and
  `default_val.q` has no reader.
- `AVOptionRanges`: dropped by `av_opt_freep_ranges`, constructed through the
  out-parameter by `av_opt_query_ranges` / `av_opt_query_ranges_default`, no
  cloner or field disposer, no in-tree `AVClass.query_ranges` callback; the
  `range` array is owned, component-major, with NULL elements possible.
