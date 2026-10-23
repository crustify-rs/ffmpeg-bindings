# `ObjectMut::remove_array` leaves a stale count when C cannot shrink the array: double free

- File: `crustify/rust/ffmpeg/libavutil/opt.rs`
- Affected commit: `c771c3e8bd` (options wave-1 head at review time;
  translation batch 1)
- Item: `av_opt_set_array` (`ObjectMut::remove_array`, its NULL-`val`
  removal path)
- Sanitizer: ASan, `attempting double-free` in `opt_free_elem`
  (`opt.c:132`) under `av_opt_free`; reading the array after the failure is
  a heap use after free when the removed element was the last one
- Sanitizer (also): BSan, `trying to access an allocation that has been
  freed` at `av_free` (`mem.c:243`) on the same run
- Suite: `tests/ub_unsafe/opt.rs`,
  `remove_array_shrink_failure::failed_shrinks_leave_the_remaining_elements_once`;
  unit test `opt::unit_tests::typed_setters::failed_shrinks_store_the_new_count`;
  `tests/equiv/opt.rs::typed_setters::allocation_failures_agree` now repairs
  the C reference's count the same way before comparing (its `ints` removal
  at limits 0 and 1 hit the failed shrink and compared C's stale count)

## Finding

C's removal path of `av_opt_set_array` (`opt.c:2405`):

```c
    if (!val) {
        void *array = *(void**)parray;

        for (unsigned i = 0; i < nb_elems; i++) {
            opt_free_elem(o->type,
                          opt_array_pelem(o, array, start_elem + i));
        }

        if (new_size > 0) {
            memmove(opt_array_pelem(o, array, start_elem),
                    opt_array_pelem(o, array, start_elem + nb_elems),
                    elem_size * (*array_size - start_elem - nb_elems));

            array = av_realloc_array(array, new_size, elem_size);
            if (!array)
                return AVERROR(ENOMEM);
            ...
        *array_size = new_size;
```

It releases the removed elements and shifts the tail down *before* it
shrinks the block, and when `av_realloc_array` fails it returns
`AVERROR(ENOMEM)` without storing `new_size`. The array then keeps its old
count over a block whose last `nb_elems` slots still hold their old bytes:
pointers that were shifted into the slots before them (now owned twice), or
pointers to elements just released. Every later reader or releaser of the
array walks the stale count: `av_opt_get` / `av_opt_get_array` copy a
released string, dictionary or layout map (use after free), and
`av_opt_free` releases a shifted one twice (double free).

A shrinking `av_realloc` fails whenever the `av_max_alloc` limit is below
the new size, and `mem::set_max_alloc` is a safe function, so the safe
`remove_array` reached this with a well-formed object. The reproducer's
class has one string array option (no definition: no bounds, `,`
separator); the object is adopted with `ObjectMut::from_ptr` within its
contract, then only safe calls follow: insert `a`, `b`, `c`, set the limit
to 8 bytes (two pointers no longer fit), remove element 0, restore the
limit, read the array back and free the options.

```console
$ CRUSTIFY_FFMPEG_VARIANT=asan-ubsan RUSTFLAGS=-Zsanitizer=address \
    cargo +nightly build -Zbuild-std --target x86_64-unknown-linux-gnu --offline
$ B=/target/crustify/builds/asan-ubsan
$ LD_LIBRARY_PATH=$(printf '%s:' $B/lib*) \
    target/x86_64-unknown-linux-gnu/debug/remove-array-shrink-failure-double-free
remove_array: Err(-12)
left: [Some("b"), Some("c"), Some("c")]
=================================================================
==1549239==ERROR: AddressSanitizer: attempting double-free on 0x79a84f3e0050 in thread T0:
    #0 0x5917d15152c6 in free
    #1 0x7d885076852a in opt_free_elem src/libavutil/opt.c:132:9
    #2 0x7d885076852a in opt_free_array src/libavutil/opt.c:151:9
    #3 0x7d88507682d5 in av_opt_free src/libavutil/opt.c:2034:13
    #4 0x5917d155ff3b in <ffmpeg::libavutil::opt::ObjectMut>::free_options crustify/rust/ffmpeg/libavutil/opt.rs:1997:18
    #5 0x5917d1554359 in remove_array_shrink_failure_double_free::main src/main.rs:126:11

0x79a84f3e0050 is located 0 bytes inside of 2-byte region [0x79a84f3e0050,0x79a84f3e0052)
freed by thread T0 here:
    #0 0x5917d15152c6 in free
    #1 0x7d885076852a in opt_free_elem src/libavutil/opt.c:132:9
    #2 0x7d885076852a in opt_free_array src/libavutil/opt.c:151:9
    #3 0x7d88507682d5 in av_opt_free src/libavutil/opt.c:2034:13
    #4 0x5917d155ff3b in <ffmpeg::libavutil::opt::ObjectMut>::free_options crustify/rust/ffmpeg/libavutil/opt.rs:1997:18
    #5 0x5917d1554359 in remove_array_shrink_failure_double_free::main src/main.rs:126:11

previously allocated by thread T0 here:
    #0 0x5917d1515894 in malloc
    #1 0x7d885074eb4d in av_realloc src/libavutil/mem.c:164:11
    #2 0x7d885074eb4d in av_strdup src/libavutil/mem.c:277:15
    #3 0x7d885076a4ce in opt_copy_elem src/libavutil/opt.c:2144:21
    #4 0x7d885076da1c in av_opt_set_array src/libavutil/opt.c:2450:19
    #5 0x5917d154950f in <ffmpeg::libavutil::opt::ObjectMut>::store_array::<&core::ffi::c_str::CStr> crustify/rust/ffmpeg/libavutil/opt.rs:3335:13
    #6 0x5917d1549681 in <ffmpeg::libavutil::opt::ObjectMut>::insert_array::<&core::ffi::c_str::CStr> crustify/rust/ffmpeg/libavutil/opt.rs:3261:14

SUMMARY: AddressSanitizer: double-free in free
==1549239==ABORTING
$ echo $?
1
```

(`left` already shows the stale third element, `c` owned twice.) The
reproducer is classed `ub_unsafe` because an object with options can only
be adopted through the documented `ObjectMut::from_ptr` seam today; the
defect itself is in the safe `remove_array`.

## Fix

The only `AVERROR(ENOMEM)` of C's removal path is that failed shrink, and
it comes after C released the range and shifted the rest down. So
`remove_array` now reads the array's count before the call (through the
option found the same way C finds it, children included) and, on `ENOMEM`,
writes back `old - count`: the remaining elements are exactly the first
`old - count` slots, and the block C kept is merely larger than they need
(later reallocations and `av_opt_free` accept it). The error is still
returned, as C returns it, and documented: the elements are removed all the
same. With the fix the reproducer prints `left: [Some("b"), Some("c")]` and
exits 0 under ASan and under BSan (run alone, as this non-final wave's
BSan policy prescribes; on the affected commit BSan also reports the
second `av_free` of the shifted string).

The regression test removes each position of three-element string,
dictionary and custom-layout arrays, on the object and through
`SearchFlags::CHILDREN` on its child, under a 1-byte limit, then reads every
remaining element back and lets the fixture free the object: on the
affected commit ASan reports the double free there; with the fix it is
clean under ASan/UBSan and TSan, and passes plain. It runs alone in a child
process, since the limit is process-wide.

The insert and replace paths are not affected: they convert into a separate
block first and roll back on any failure, the failed `av_realloc_array`
included.
