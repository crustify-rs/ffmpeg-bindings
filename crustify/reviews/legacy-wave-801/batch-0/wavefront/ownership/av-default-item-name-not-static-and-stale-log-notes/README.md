# `av_default_item_name`'s class name is not `static`, and five logging notes are stale

- Kind: wrong pointer-related finding in wavefront's ownership store
  (`crustify/wavefront/ownership-store.json`), reviewed as the wave-800 items' submitted
  findings
- Items: `av_default_item_name`, `av_log`, `av_log_once`, `av_log_format_line`,
  `av_log_format_line2`, `av_log_default_callback` (all `libavutil/log.c`)
- Fixed: yes, resubmitted with `query symbols --update`

## What the store said

Wave 800's commit message says: "The ownership store's findings for all eight items agree
with the wrappers, so no --update was needed." Two kinds of entries do not agree.

1. **`av_default_item_name`.** Its return was recorded as
   `borrowed: {"lifetime": "static"}`, with the note "it lives as long as the AVClass (a
   string literal in every in-tree class)". The class behind `ptr` (the `by_ref` inner
   pointer) was also recorded as `borrowed: {"lifetime": "static"}`, "static in every
   in-tree class".
2. **The logging contexts.** The `avcl` / `ptr` notes of `av_log`, `av_log_once`,
   `av_log_format_line`, `av_log_format_line2` and `av_log_default_callback` end with
   "Rust: raw seam *mut c_void until AVClass-carrying types are wrapped."

## Evidence

1. Not every in-tree class is static, and not every class name is a literal.
   `vs2av_log` (`c/libavfilter/vidstabutils.c:59-75`) builds an `AVClass` on its stack,
   with `.class_name = tag` (its caller's string) and `.item_name = av_default_item_name`,
   and logs in a stack `VS2AVLogCtx` pointing to it. `av_default_item_name`
   (`c/libavutil/log.c:241-244`) returns `(*(AVClass **) ptr)->class_name`. Through
   `format_line`, for that context it returns `tag`, which is valid only while
   `vs2av_log`'s caller holds it, and the class itself dies when `vs2av_log` returns.
   `fftools/ffmpeg_demux.c:1622` and `fftools/ffmpeg_mux_init.c:2538` build stack classes
   too. The Rust class wrapper already documents this ("a logging adapter may build a
   temporary descriptor (for example `vs2av_log` borrows its caller's tag as the name)"),
   and `default_item_name` ties the name to the context's borrow (`Option<&'a CStr>`), not
   to `'static`.
2. Since wave 800, every one of those wrappers takes a `LogContext` (`log_with_context`,
   `log_fmt_with_context`, `log_once_with_context`), or hands one out from the callback's
   context (`Record::context`). No raw `*mut c_void` context remains on the public API: an
   `ffimap` scan of the wave tip with `--sites raw_ptr.fn_sig_arg.safe.pub
   raw_ptr.fn_sig_ret.safe.pub` lists no site in `log.rs` but `Class::as_c_ptr` (line 504),
   a class pointer, not a context.

## Correction

Submitted with `wavefront c --config ../crustify/wavefront/configs/wavefront-config.json
query symbols --name <item> --file libavutil/log.c --update <file>`. The submitted
documents are kept beside this report (`*.update.json`).

- `av_default_item_name`: the return becomes `borrowed: {"lifetime": "arg:ptr"}`, valid
  while the class `ptr` points to is, with a note naming the stack class of `vs2av_log`.
  The inner class pointer becomes `borrowed: {"lifetime": "other"}`: it is borrowed from
  whoever built the class, which outlives the object but need not be static. Ownership,
  nullability, mutability and shape are unchanged.
- The five notes end with "Rust: a LogContext (an ObjectRef converts into one;
  Record::context hands a callback the one it receives)." instead. The blocks are
  otherwise unchanged.

The Rust needed no change for either: its lifetimes were already the corrected ones.

## Rerun

```bash
sed -n 59,75p c/libavfilter/vidstabutils.c; sed -n 241,244p c/libavutil/log.c
git show 3df5deb2cbe:crustify/wavefront/ownership-store.json | python3 -c '
import json,sys
for r in json.load(sys.stdin)["symbols"]:
    if r["name"]=="av_default_item_name": print(json.dumps(r["ptr_ret"]), json.dumps(r["ptr_args"]))'
wavefront c --config ../crustify/wavefront/configs/wavefront-config.json \
    query symbols --name av_default_item_name --file libavutil/log.c
```
