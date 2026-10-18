# Misc: option tables and range lists could not cross threads

- File: `crustify/rust/ffmpeg/libavutil/opt.rs`
- Affected commit: `ad7acaeaca` ("wrap libavutil's AVOption and AVOptionRanges")
- Items: `OptionDef` / `OptionDefRef` / `OptionDefMut`, `OptionDefault`,
  `OptionRanges` / `OptionRangesRef` / `OptionRangesMut`, `OptionRangesBox`
- Kind: missing `Send` / `Sync` (API ergonomics; no unsoundness)

## Finding

`define_ctype!` gives every layout type a `PhantomData<*const ()>` marker,
and its handles wrap a `NonNull`, so a wrapped type is `!Send + !Sync` until
the wrapper opts in with a proof (as `rational.rs` and `intfloat.rs` in the
same wave do). The opt wrappers never opted in, although both objects are
immutable and thread-agnostic:

- An `AVOption` lives in a `static const` table that no C path writes once an
  `AVClass` publishes it (the submitted wavefront finding, re-verified: no
  `sizeof(AVOption)` allocation or non-`const` table exists in the
  libraries), and C reads one table from any thread (`av_opt_set`,
  `av_opt_find` on a shared context). Its pointers address string literals
  and a `static const AVOptionArrayDef`.
- An `AVOptionRanges` tree (list, `range` array, ranges, their `str`) is
  `av_malloc`ed by the query, handed over whole, and released by
  `av_opt_freep_ranges` through `av_free`, which any thread may call. C keeps
  no pointer to it after the query returns (`av_opt_query_ranges` writes
  `nb_components` before returning; `opt_list`, the only in-tree reader, frees
  what it queried). The wrapper exposes no setter, and the policy's own doc
  already said "thread-agnostic".

So a consumer could not move a queried list to a worker thread, drop it
there, share it between readers, or read a table entry's handle from another
thread, and even a read `OptionDefault` was `!Send` because its `Array`
variant holds the raw `*const AVOptionArrayDef` seam. On the affected commit
the package beside this report fails to build with 15 `E0277` errors, among
them:

```text
error[E0277]: `NonNull<OptionRanges>` cannot be sent between threads safely
   --> src/main.rs:69:17
 69 |     send_sync::<OptionRangesRef<'static>>();
    |                 ^^^^^^^^^^^^^^^^^^^^^^^^ `NonNull<OptionRanges>` cannot be sent between threads safely
    = help: within `OptionRangesRef<'static>`, the trait `Send` is not implemented for `NonNull<OptionRanges>`
note: required because it appears within the type `ffibox::refs::CBorrowedPtr<'static, OptionRanges>`

error[E0277]: `NonNull<OptionDef>` cannot be shared between threads safely
  --> src/main.rs:70:17
 70 |     send_sync::<OptionDefRef<'static>>();
   |                 ^^^^^^^^^^^^^^^^^^^^^ `NonNull<OptionDef>` cannot be shared between threads safely

error[E0277]: `*const ()` cannot be sent between threads safely
   --> src/main.rs:68:17
 68 |     send_sync::<OptionRangesBox>();
    |                 ^^^^^^^^^^^^^^^ `*const ()` cannot be sent between threads safely
    = help: within `OptionRanges`, the trait `Send` is not implemented for `*const ()`
note: required because it appears within the type `PhantomData<*const ()>`
```

With the fix it compiles and `cargo run` exits 0 (it shares a list with a
scoped thread and drops it on another).

## Fix

`opt.rs` adds `unsafe impl Send` / `Sync` with the proofs above for
`OptionDef`, `OptionDefRef`, `OptionDefMut`, `OptionDefault`, `OptionRanges`,
`OptionRangesRef` and `OptionRangesMut`; `OptionRangesFree` is a unit struct,
so `OptionRangesBox = CBox<OptionRanges, OptionRangesFree>` becomes
`Send + Sync` on `Box`'s terms. The `Mut` handles have no setters, so their
`Sync` is the shared handle's. `OptionDefault`'s impls are tied to the raw
`AVOptionArrayDef` seam and go away with it when that type's batch replaces
the pointer by a handle.

## Regression evidence

- `opt.rs` `unit_tests::options_and_defaults_are_send_and_sync`: static
  `Send + Sync` assertions for every type above, four threads reading one
  copied `OptionDefRef` and its default concurrently, and a handle returned
  from another thread (Miri-clean; the table is a Rust static).
- `tests/ub_unsafe/opt.rs`
  `query::lists_are_read_moved_and_dropped_across_threads`: a queried list
  read by four scoped threads through `&OptionRangesBox` and a sent
  `OptionRangesRef`, dropped on another thread, built on one thread and read
  on the next, and eight concurrent queries of one shared class. Run under
  ThreadSanitizer (`CRUSTIFY_FFMPEG_VARIANT=tsan`, `-Zsanitizer=thread`) with
  no report, and under ASan/LSan and BSan.
