# UB (safe API, leak): an in-place clone leaks a partly copied metadata dictionary

- Item: `av_frame_side_data_clone`, wrapped by `SideDataArrayMut::add_clone` in
  `crustify/rust/ffmpeg/libavutil/frame.rs`.
- Affected commit: the wave tip `2a232726c8`. The wrapper came from translation
  batch 1, `63d684f623`.
- Instrument: LeakSanitizer, part of the asan-ubsan build. The trace is in
  `asan.trace`. This is a leak, not undefined behaviour. The campaign lists
  leaks among the ASan obligations, and a safe call reaches this one.
- Status: fixed in `frame.rs`. `add_clone` now runs C's in-place replacement
  itself (`SideDataMut::assign_clone`). Regression tests:
  `tests/ub_safe/frame.rs::side_data_array_routines::in_place_clones_free_a_partial_metadata_copy`
  (LSan, in an isolated child) and
  `tests/equiv/frame.rs::side_data_array_routines::in_place_clones_match_c`.

## Finding

`av_frame_side_data_clone` replaces an entry in place when `REPLACE` is set,
the source's kind is not `MULTI`, and the array already has an entry of that
kind (`libavutil/side_data.c:268-290`). It copies the source's metadata
first, into a local dictionary:

```c
        AVDictionary *dict = NULL;
        ...
        ret = av_dict_copy(&dict, src->metadata, 0);
        if (ret < 0)
            return ret;
```

`av_dict_copy` stops at the first `av_dict_set` that fails, and it keeps
the entries it already copied. `av_dict_set` frees the dictionary only when
the dictionary is still empty (`dict.c:166-169`). So a source with two or
more distinct keys, whose copy fails after the first key, leaves a
non-empty `dict`. Nothing references it, and C returns without freeing it.
The append path does not have this problem: there, a failed copy goes into
the new entry's own `metadata`, and `remove_side_data_by_entry` frees it.

The safe API reaches the leak directly. `mem::set_max_alloc` is safe, and
any allocation failure (a real out-of-memory condition included) does the
same. In the reproducer, the source's second key is 150 bytes and the limit
is 100 bytes. The first entry is copied, the second `av_strdup` fails, and
`add_clone` returns `AVERROR(ENOMEM)`. LSan then reports the dictionary
struct, its entry array and the two strings of the first entry: 36 bytes
in 4 allocations.

The equivalence suite's limit test
(`failures_at_the_av_max_alloc_limit_match_c`) missed this. Its metadata
has a single entry, and when a single-entry copy fails, the dictionary is
still empty, so C frees it. The ASan pass of translation batch 1 therefore
saw no leak.

## Fix

`add_clone` decides up front whether C would take the in-place path. The
same check (`SideDataArrayRef::replaces_in_place`) already guarded
`alloc_*` against C's NULL dereference, and both wrappers now share it. On
that path, `add_clone` does the replacement itself, in C's order:

1. Copy the metadata with `av_dict_copy(…, 0)`. The flags are C's, so keys
   equal up to ASCII case still merge. The copy goes straight into a `Dict`
   owner, so a partial copy is freed.
2. Call `av_buffer_replace` on the entry's own slot.
3. Write `src`'s `data` and `size`, and put the copy in place of the old
   metadata.

The order matters: under an allocation limit, the copy fails where C's
would. On failure the entry stays as it was, as in C. Every other path
still calls C. The new equivalence test compares the rewritten path with
C's in these cases:

- source metadata that C merges (keys differing only in case, exact
  duplicates) or keeps;
- destination entries with and without metadata;
- an entry that already shares the source's buffer (C then only takes the
  window);
- a kind C has no name for;
- in an isolated child, every limit from 0 to 64 bytes for the sources
  whose copy cannot fail part way.

Sources whose copy could fail part way run only without a limit there,
because C itself would leak.

Later wrappers that reach this C path should avoid it the same way. One
example is a frame-level clone of side data in wave 4: check
`av_frame_copy_props` / `frame_copy_props` in `frame.c` for
`av_frame_side_data_clone` with `REPLACE`.

## Reproducer

`src/main.rs` (`#![forbid(unsafe_code)]`). Built for ASan in this directory, it
exits 1 on the affected commit with the LSan report and exits 0 on the fix:

```sh
CARGO_TARGET_DIR=/tmp/repro CRUSTIFY_FFMPEG_VARIANT=asan-ubsan RUSTFLAGS=-Zsanitizer=address \
  cargo +nightly build -Zbuild-std --target x86_64-unknown-linux-gnu --offline
ASAN_SYMBOLIZER_PATH=/usr/lib/llvm-18/bin/llvm-symbolizer \
LD_LIBRARY_PATH=../../../../../../../../builds/asan-ubsan/libavutil \
  /tmp/repro/x86_64-unknown-linux-gnu/debug/side-data-clone-replace-leaks-partial-metadata
```

Abridged trace (the full text is in `asan.trace`):

```
add_clone: Err(-12)
==ERROR: LeakSanitizer: detected memory leaks
Direct leak of 16 byte(s) in 1 object(s) allocated from:
    #1 in av_malloc        libavutil/mem.c:107:9
    #2 in av_mallocz       libavutil/mem.c:258:17
    #3 in av_dict_set      libavutil/dict.c:122:19
    #4 in av_dict_copy     libavutil/dict.c:252:19
    #5 in av_frame_side_data_clone   libavutil/side_data.c:275:15
    #6 in <ffmpeg::libavutil::frame::SideDataArrayMut>::add_clone   frame.rs:2289:13
    #7 in side_data_clone_replace_leaks_partial_metadata::main      main.rs:42:10
Indirect leak of 16 byte(s) ... av_dict_set dict.c:147 (the entry array)
Indirect leak of 2 byte(s) ... av_strdup (key "a"), and 2 byte(s) (value "b")
SUMMARY: AddressSanitizer: 36 byte(s) leaked in 4 allocation(s).
```

As the BSan policy requires, the reproducer was also run alone under BSan, on
the fixed tree. BSan does not check for leaks, and it reported nothing.
