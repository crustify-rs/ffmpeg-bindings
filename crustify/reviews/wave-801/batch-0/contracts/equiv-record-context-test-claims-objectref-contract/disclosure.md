Subject: ffmpeg-bindings: equiv test adopts fixtures as `ObjectRef` against its contract

Hi Marius,

The equivalence test `record_context::contexts_read_as_c_reads_them`
(`tests/equiv/log.rs`, added by wave 800, commit `c0f53d14a81`) has a `// SAFETY:` comment
that does not hold.

It adopts four fixture objects with `ObjectRef::from_ptr` before logging in them:

```rust
// SAFETY: a live object whose class's callbacks accept it, with
// no options or offsets; nothing writes it while it logs.
let o = unsafe { ObjectRef::from_ptr((&raw mut *obj).cast()) }.unwrap();
```

`ObjectRef`'s contract asks for a class that has a name, and for a `get_category` that
returns a category below `AV_CLASS_CATEGORY_NB`. One fixture class has a NULL
`class_name`. Another object's `get_category` returns 46, which is `AV_CLASS_CATEGORY_NB` itself and C's
default callback uses to index a static colour table. The test passes because it installs
a Rust callback, and that class's version 0 makes C skip the callback. But it still claims
a contract it breaks.

The patch logs in each fixture through the raw `ffi::av_log`, which only asks for an
`AVClass`-enabled context, so the callback still sees each object as its record's context.
It also moves the out-of-range `get_category` value into range. The out-of-range class
`category` (4242) stays covered, since neither `av_default_get_category` nor the Rust
getters bound it. There is no sanitizer trace to show: nothing misbehaves at run time. I
can share the review report on request.

Found by [Crustify](https://github.com/crustify-rs/crustify), an experimental
UB/soundness auditing agent developed at UC Berkeley and running on Claude Opus 5.5, then
manually reviewed and independently reproduced.
