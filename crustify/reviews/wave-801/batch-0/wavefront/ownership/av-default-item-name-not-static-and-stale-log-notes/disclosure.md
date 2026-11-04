Subject: ffmpeg-bindings: ownership store calls av_default_item_name's result static

Hi Marius,

While reviewing wave 800 of `ffmpeg-bindings`, I checked the wavefront ownership findings
for the eight `libavutil/log.c` items it rebound. The wave's commit says they all agree
with the wrappers. Two entries do not.

1. `av_default_item_name`'s return, and the `AVClass` its argument points to, are recorded
   as `static` borrows ("a string literal in every in-tree class"). FFmpeg itself has a
   counterexample in `libavfilter/vidstabutils.c`:

   ```c
   AVClass class = {
       .class_name = tag,                 /* the caller's string */
       .item_name  = av_default_item_name,
       ...
   };
   ctx.class = &class;
   av_vlog(&ctx, type, format, ap);
   ```

   There, `av_default_item_name` returns `tag`, and the class lives on the stack. The safe
   wrapper already ties the name to the context's borrow, so the Rust is correct, but a
   later batch reading the store would be told `'static`.

2. The context notes on `av_log`, `av_log_once`, `av_log_format_line`,
   `av_log_format_line2` and `av_log_default_callback` still say the Rust context is "a raw
   seam `*mut c_void` until AVClass-carrying types are wrapped". Since wave 800 it is a
   `LogContext`.

The review resubmitted all six records with `query symbols --update`. The return now
borrows from `ptr` and the inner class is `other`, and the notes describe `LogContext`.
The submitted documents and the evidence are in the review's report. There is no sanitizer
trace: nothing misbehaves at run time.

Found by [Crustify](https://github.com/crustify-rs/crustify), an experimental
UB/soundness auditing agent developed at UC Berkeley and running on Claude Opus 5.5, then
manually reviewed and independently reproduced.
