# Lifecycle gap: no singleton clone strategy for `av_memdup`

- Item: `void` (raw-lifetime), home `crustify/rust/ffmpeg/libavutil/mem.rs`
- Affected commit: `42bb68057f` ("wrap libavutil's void raw-lifetime primitives")
- Kind: missing lifetime representation

## Finding

`av_memdup` is the void tier's only deep cloner, and C uses it on single objects
as well as on arrays:

```c
/* libavcodec/dnxhdenc.c:512 */
ctx->thread[i] = av_memdup(ctx, sizeof(DNXHDEncContext));
/* libavcodec/mpegvideo.c:105 */
s->thread_context[i] = av_memdup(s, slice_size);
```

The previous run bound it only as the array strategy (`CLenClone` through
`impl_clenclone!`, giving `HeapVec::from_slice` and `HeapVec: Clone`). The
singleton owner `HeapBox<T> = CBox<T, Heap>` got a constructor pair
(`CAlloc` / `CAllocZeroed`) and a releaser (`CDrop`), but no cloner, so the
singleton and array tiers were asymmetric: a `HeapVec<T>` of `T: Copy` cloned
through `av_memdup`, while a `HeapBox<T>` of the same `T` could not be cloned at
all. On the affected commit:

```rust
define_ctype!(#[derive(Clone, Copy)] Plain, PlainRef, PlainMut, ffi::AVRational);
unsafe impl CZeroable for Plain {}
let a = HeapBox::<Plain>::new_zeroed().unwrap();
let b = a.clone();
// error[E0599]: the method `clone` exists for struct `CBox<Plain, Heap>`,
//               but its trait bounds were not satisfied
//   = note: `Heap: CDupClone<Plain>` was not satisfied
```

## Fix

`Heap` now implements `CDupClone<T>` for `T: CCell + Copy` through
`av_memdup(ptr, size_of::<T>())`, anchored `/// Wraps: av_memdup`:

- `T: Copy` is the soundness bound. A byte copy is an independent object only
  when `T`'s bytes own nothing; a wrapped type whose accessors manage owned
  sub-allocations (a replacing setter that frees the old pointee) must not be
  duplicated bytewise, and such a type is not `Copy`. The same bound already
  gates `HeapVec`'s byte-copy `Clone`. A compile_fail doctest pins the
  rejection of a non-`Copy` wrapped type.
- The block is `av_malloc`-aligned, so the impl carries the same
  `align_of::<T>() <= 16` const assertion as `CAlloc` / `CAllocZeroed`.
- Release stays `CDrop<T>` (`av_free`), so the copy is settled by the same
  policy that frees the source.

## Regression evidence

- `mem.rs` `unit_tests`: `box_clone_is_an_independent_aligned_av_memdup_copy`
  (independence, alignment, `try_clone`, source dropped first) and
  `policy_plumbing_duplicates_one_object` (`c_dup` / `c_drop` directly).
- `tests/equiv/mem.rs`: `box_clone_matches_av_memdup_step_by_step` compares
  `HeapBox::clone` / `try_clone` with `av_memdup` byte for byte over a
  copy -> mutate-source -> copy-the-copy sequence.

## Primitives re-checked and left as they are

- `av_freep` (slot disposer): an owner holds the pointer, not the C slot; a
  field setter that drops the old `HeapBox` / `HeapVec` and stores the new
  pointer is the Rust form, so no separate strategy is needed in this tier.
- `av_realloc`, `av_realloc_f`, `av_reallocp`, `av_realloc_array`,
  `av_reallocp_array`, `av_fast_realloc`, `av_fast_malloc`,
  `av_fast_mallocz`, `av_dynarray*_add`: realloc-style (release and allocate
  the same slot), not constructors per the wavefront schema; they belong to
  the symbol route.
- `av_strdup` / `av_strndup`: string tier.
