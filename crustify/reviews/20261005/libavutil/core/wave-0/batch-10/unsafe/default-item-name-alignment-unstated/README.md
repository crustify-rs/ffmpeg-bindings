# Unsafe: `default_item_name`'s contract did not require an aligned context

- File: `crustify/rust/ffmpeg/libavutil/log.rs`
- Affected commit: `35f30a7185`
- Item: `default_item_name` (`av_default_item_name`), the batch's one
  `pub unsafe fn`
- Sanitizer: UBSan (`-fsanitize=alignment`) in the `asan-ubsan` library

## Finding

`av_default_item_name` reads the class pointer as `(*(AVClass **) ptr)`: a
pointer-typed load from `ptr`, which C requires to be aligned for a pointer.
The wrapper's `# Safety` section asked for "readable memory that starts with
a non-NULL `const AVClass *` to a live class", which a caller can satisfy to
the letter with a byte buffer holding the pointer at an odd offset. That
call is undefined behaviour in C, and UBSan reports it inside
`av_default_item_name`:

```console
$ CRUSTIFY_FFMPEG_VARIANT=asan-ubsan RUSTFLAGS=-Zsanitizer=address \
    cargo +nightly run -Zbuild-std --target x86_64-unknown-linux-gnu --offline
object at 0x6f45babf0161, which is not aligned to 8
src/libavutil/log.c:243:13: runtime error: load of misaligned address 0x6f45babf0161 for type 'AVClass *' (aka 'struct AVClass *'), which requires 8 byte alignment
0x6f45babf0161: note: pointer points here
 00 00 00  00 f0 01 bf ba 45 6f 00  00 00 00 00 00 00 00 00  00 00 00 00 00 00 00 00  00 00 00 00 00
              ^
    #0 0x7dce0f585473  (.../crustify/builds/asan-ubsan/libavutil/libavutil.so.61+0x29f473)
    #1 0x6000b0aa6324  (.../default-item-name-alignment-unstated+0x24b324)
    ...
SUMMARY: UndefinedBehaviorSanitizer: undefined-behavior src/libavutil/log.c:243:13
$ echo $?
1
```

UBSan recovers by default, so `src/main.rs` re-runs itself with
`UBSAN_OPTIONS=halt_on_error=1` and exits with the child's status. The
package depends on `libavutil-sys` directly only so its `build.rs` can embed
the selected variant's directory as an rpath, as the ffmpeg crate's own
`build.rs` does for its tests. `Cargo.lock` is seeded from
`crustify/rust/Cargo.lock`, so `--offline` resolves.

Every real `AVClass`-enabled object is a C struct whose first member is the
class pointer, so every caller passing one is fine; the gap was in what the
contract let a caller believe was enough.

## Fix

- The `# Safety` section now names the obligation C relies on: `ctx` is a
  `const AVClass *` first member, readable and aligned for a pointer, and
  says why ("C loads it as `*(AVClass **) ctx`, so a byte offset into a
  buffer does not qualify").
- A `debug_assert!` checks the alignment before the FFI call, so a debug
  build reports a misaligned context as a panic instead of reaching C's
  undefined load.
- Regression tests: `log::unit_tests::default_item_name_rejects_a_misaligned_context_before_c`
  (debug builds; the assertion fires before any C read, so the test performs
  no misaligned load) and
  `tests/ub_unsafe/log.rs::item_name::class_pointer_slot_inside_a_larger_object`,
  which exercises the aligned-but-interior case the contract admits: a
  context pointing at the class member of a larger heap object.

The reproducer still runs against the fixed commit, since it violates the
(now explicit) contract; in a debug build it now stops at the assertion
(`context 0x... is not aligned for its class pointer`) before C runs.
