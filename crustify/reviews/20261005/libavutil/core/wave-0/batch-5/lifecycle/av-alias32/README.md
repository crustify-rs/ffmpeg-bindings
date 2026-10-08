# Lifecycle: `av_alias32` had no borrowed form over caller bytes

- File: `crustify/rust/ffmpeg/libavutil/intreadwrite.rs`
- Affected commit: `8a4ef630c5` (wrap of `av_alias16` / `av_alias32`)
- Item: `av_alias32` (`Alias32`, `Alias32Ref`, `Alias32Mut`)

## Finding

The wavefront finding submitted with the wrap is right: `av_alias32` owns
nothing, has no constructor, destructor or clone routine, and "C casts existing
storage (`Mv`, `VP8mv`, `VP9mv`, byte buffers) to `av_alias32 *` or holds it by
value (`exif.c`)". The wrapper implemented only the second form. A value
(`Alias32::from_*`, `zeroed()`), a C-allocated box (`HeapBox<Alias32>`) and an
array (`HeapVec<Alias32>`) all exist, but there was no safe way to form an
`Alias32Ref` / `Alias32Mut` over bytes the caller already holds: the handles'
`from_ptr` is `pub(crate)`, and nothing public called it.

That borrowed form is how the C API actually uses the type. Every public
`intreadwrite.h` macro over it is a cast of an existing, aligned byte pointer:

```c
#define AV_RNA(s, p)    (((const av_alias##s*)(p))->u##s)   /* AV_RN32A */
#define AV_WNA(s, p, v) (((av_alias##s*)(p))->u##s = (v))   /* AV_WN32A */
#define AV_COPY(n, d, s) (((av_alias##n*)(d))->u##n = ((const av_alias##n*)(s))->u##n)
#define AV_ZERO(n, d)   (((av_alias##n*)(d))->u##n = 0)
```

and the tree has 400+ such sites (`grep -rn 'AV_RN32A\|AV_WN32A\|AV_COPY32\|AV_ZERO32' libav*` ;
`libavutil/aes_ctr.c:122` is the one inside libavutil, reading the keystream
through `AV_RN32A(&a->encrypted_counter[len])`). The by-value form is used by
two files (`libavcodec/exif.c`, `libavcodec/tiff_common.c`). A Rust consumer
mirroring any of the cast sites had to copy the bytes into an `Alias32` and
back, or reach for the raw `ffi::av_alias32`, which is the gap the type route's
"implement every ownership variant supported by the analysis findings" step
asks to close. The `unaligned_N` siblings in the same module already have it
(`Unaligned32Ref::from_bytes(&[u8; 4])`); the aligned unions were left without.

The one contract the C macros add over the unaligned ones is alignment: the
header says `AV_[RW]N[8-64]A` "require aligned access", and a misaligned cast
is undefined behaviour C leaves to the caller.

## Fix

- `Alias32Ref::from_bytes(&'a [u8; 4]) -> Option<Alias32Ref<'a>>` and
  `Alias32Mut::from_bytes(&'a mut [u8; 4]) -> Option<Alias32Mut<'a>>` view the
  bytes in place, as the `AV_RN32A` / `AV_WN32A` casts do, and return `None`
  when the address is not 4-aligned (checked with `<*const T>::is_aligned`),
  turning C's unchecked precondition into a checked one. The shared handle
  only reads, the exclusive one holds the `&mut` for `'a`, and every bit
  pattern is a valid `av_alias32`, so both are safe to call. The module docs
  point at the new entry points.
- Tests:
  - `mod unit_tests::alias_byte_views_need_the_union_alignment` sweeps an
    8-aligned buffer and checks that exactly the aligned offsets yield a view,
    that writes land where `AV_WN32A` would put them, and that the handles
    address the viewed bytes (also runs under Miri, strict provenance and Tree
    Borrows).
  - `tests/ub_safe/intreadwrite.rs`: `alias_byte_views_stay_in_bounds_of_exact_rust_buffers`,
    `heap_alias_byte_views_stay_in_bounds_of_exact_c_buffers`,
    `alias_views_overlap_consistently`,
    `alias_view_reborrows_hand_back_the_exclusive_handle` and
    `alias_views_cross_threads` (ASan/UBSan, TSan, BSan, Miri for the FFI-free
    ones); compile-fail `intreadwrite_alias_view_outlives_bytes.rs`.
  - `tests/equiv/intreadwrite.rs`:
    `aligned_views_read_what_the_aligned_c_macros_read` and
    `aligned_views_write_what_the_aligned_c_macros_write` compare the views
    with `AV_RN32A` / `AV_WN32A` / `AV_COPY32` / `AV_ZERO32` (and the 16-bit
    forms) run by C: `libavutil-sys` compiles each macro as a
    `crustify_AV_<NAME>` shim (`shim.c` / `shim.h`, as the `av_alias64` review
    did for the 64-bit forms), applied at every aligned offset of
    independently owned buffers and compared after each step, on Rust,
    `av_mallocz`'d and `av_calloc`'d storage.

## Not changed

`av_alias64` (`Alias64`, another batch's item in the same file) has the same
gap and would take the same two `from_bytes` with `[u8; 8]`; left for its own
review so this batch's diff stays on its items.
