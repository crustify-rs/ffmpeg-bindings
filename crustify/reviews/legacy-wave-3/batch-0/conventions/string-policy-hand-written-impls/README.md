# Conventions: `StringHeap` hand-wrote what ffibox's string macros bind

- File: `crustify/rust/ffmpeg/libavutil/mem.rs`
- Affected commit: `271811c5bb`
- Items: `CDrop<c_char> for StringHeap` (`av_free`), `CDupClone<c_char> for
  StringHeap` (`av_strdup`)

## Finding

The translator role says to use ffibox's primitives where they apply, to
hand-write an implementation only when ffibox cannot express the proven
contract, and to mark a hand-written one with the `/// Handwritten` anchor.
The string policy's two lifecycle impls were hand-written `unsafe impl`
blocks, and neither carried the anchor.

ffibox already binds both shapes:

- `impl_cdrop_str!(P, f)` implements `CDrop<c_char>` with `f(*mut c_char)`,
  and its docs say a generic `void *` free like `av_free` goes behind an
  `unsafe fn` adapter taking `*mut c_char`;
- `impl_cdupclone_str!(P, f)` implements `CDupClone<c_char>` with a `strdup`
  of shape `*mut c_char -> *mut c_char`. `av_strdup` takes `*const c_char`,
  which a `*mut` argument coerces to.

`impl_cdupclone_str!` fits `av_strdup` exactly, so its hand-written impl was
unnecessary. `impl_cdrop_str!` fits `av_free` only through a raw-pointer
adapter, which the hand-written impl avoids (its `c_drop` takes the
`NonNull`), so keeping that one hand-written is a legitimate choice. It
still needed the `/// Handwritten` anchor.

## Fix

- `av_strdup`: the clone strategy is now `impl_cdupclone_str!(StringHeap,
  ffi::av_strdup)`, with no adapter. A doc comment on a macro invocation is
  dropped (`unused_doc_comments`), so the strategy's contract is a plain
  comment above it. The routine keeps its `/// Wraps: av_strdup` anchor on
  `string_from_c_str`.
- `av_free`: the `CDrop<c_char>` impl stays hand-written, now marked
  `/// Handwritten` with the reason it is.

The alternative, an anchored `unsafe fn` adapter for each routine as the void
tier's `free_len` / `memdup`, was tried and rejected: `crustify scan-unsafe
--name av_free av_strdup av_strndup` counted two more `unsafe fn`s, two more
raw-pointer arguments and one more raw-pointer return than the affected
commit. The chosen form leaves those counts unchanged (`unsafe_blocks` 21,
`unsafe_fns` 8, `raw_ptr_args` 4, `raw_ptr_rets` 1); the two `av_strdup`
raw-pointer sites the scan now names are the cast and call inside ffibox's
`impl_cdupclone_str!` expansion (`src/macros.rs:427,429`), not code in
this crate. The
public surface (`StringHeap`, `HeapString`, `Clone` / `try_clone`) is
unchanged; the inline `string_policy_plumbing_copies_and_frees` test drives
`c_dup` / `c_drop` directly and passes, as do the ub_safe and equiv string
suites under ASan/UBSan, TSan and BSan.

## Out of scope, noted

`Heap`'s generic `CDrop<T>`, `CDupClone<T>`, `CAlloc<T>` and
`CAllocZeroed<T>` impls (void tier) are hand-written by necessity, since
ffibox has no macro for a generic impl or for the constructor traits. They
carry no `/// Handwritten` anchor either. They belong to the reviewed void
tier, so they are left for its owner.
