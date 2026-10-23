# `BareObject::new` accepts classes whose logging callbacks read a full object

- File: `crustify/rust/ffmpeg/libavutil/opt.rs`
- Affected commit: `cde88fc249` (options wave-0 head at review time;
  translation batch 3)
- Items: `av_opt_show2` (`ObjectRef::show`) and `av_expr_parse_and_eval`
  (`eval::parse_and_eval`), whose log contexts `BareObject` provides;
  `av_parse_color` (`parse_color_with_context`) takes one since this review
- Sanitizer: ASan, `heap-buffer-overflow ... READ of size 8` in the class's
  `item_name`, called from `format_line` (`log.c:343`)
- Suite: `tests/ub_unsafe/opt.rs`,
  `log_contexts::bare_objects_refuse_classes_whose_callbacks_read_the_object`;
  unit test `opt::unit_tests::objects::bare_objects_need_a_class_without_options`

## Finding

`BareObject` is a Rust object that is only a class pointer (8 bytes), a
log context for the safe API. `BareObject::new(class)` refused a class with
options, `child_next`, a log level, parent context or state flags offset,
"which C would read in an object larger than a class pointer". It did not
look at the two callbacks logging calls *on the object*: `av_log`'s default
callback prints the prefix with `item_name(avcl, avc)` and, for a class of
a current version, `get_category(avcl)` (`log.c:343-344`). A class's
callbacks are entitled to read its objects (`ItemNameFn::call`'s contract:
"a live object of the class the callback came from"), and FFmpeg has such
classes without options or offsets: `hwdevice_ctx_class` (`item_name`
returns `ctx->hw_type->name`). A bare object of one is 8 bytes the callback
reads past.

The reproducer's class has that shape (no options, children or offsets; an
`item_name` reading its device's name field). Its `ClassRef` comes from a
live device adopted with `ObjectRef::from_ptr` within that function's
contract; `BareObject::new` accepts it, and logging through the bare object
with the safe `ObjectRef::show` reads past it:

```console
BareObject::new accepted the class; logging through it
==1169816==ERROR: AddressSanitizer: heap-buffer-overflow on address 0x7001435e0018 at pc 0x5ea8d1acfde9 bp 0x7ffc5a33efc0 sp 0x7ffc5a33efb8
READ of size 8 at 0x7001435e0018 thread T0
    #0 0x5ea8d1acfde8 in bare_object_foreign_callbacks_read_past_object::device_name src/main.rs
    #1 0x73e144917cc4 in format_line /target/crustify/builds/asan-ubsan/src/libavutil/log.c:343:20
    #2 0x73e144918ddf in av_log_default_callback /target/crustify/builds/asan-ubsan/src/libavutil/log.c:399:5
    #3 0x73e144919b22 in av_log /target/crustify/builds/asan-ubsan/src/libavutil/log.c:446:5
    #4 0x73e144939715 in av_opt_show2 /target/crustify/builds/asan-ubsan/src/libavutil/opt.c:1749:5
    #5 0x5ea8d1ad0350 in <ffmpeg::libavutil::opt::ObjectRef>::show crustify/rust/ffmpeg/libavutil/opt.rs:1590:18
    #6 0x5ea8d1ad019c in bare_object_foreign_callbacks_read_past_object::main src/main.rs
$ echo $?
1
```

The same read happens for a bare object passed to `parse_and_eval` (the
"Eval" context names its parent with the parent's `item_name`) or to
`parse_color_with_context`.

## Fix

`BareObject::new` also requires the class's `item_name` to be absent or
`av_default_item_name` (which reads only the class pointer; compared by
address through the new crate-private `ItemNameFn::is_default`) and the
class to have no `get_category`. `log::Class::new` builds exactly such
classes, so every documented use keeps working. Documented on `new`.

## Reproduce

Build as in `../set-flag-operand-clock-race/README.md`, then run the
binary `bare-object-foreign-callbacks-read-past-object` with the same
`LD_LIBRARY_PATH` (ASan aborts on its own; no option needed). On the fixed
tree it prints `BareObject::new refused the class` and exits 0.
