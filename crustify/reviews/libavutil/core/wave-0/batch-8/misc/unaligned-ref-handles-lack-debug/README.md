# Misc: `Unaligned16Ref` / `Unaligned32Ref` had no `Debug`

- File: `crustify/rust/ffmpeg/libavutil/intreadwrite.rs`
- Affected commit: `809f4922ae` (wrap of `unaligned_16` and `unaligned_32`)
- Items: `Unaligned16Ref<'a>`, `Unaligned32Ref<'a>` (the shared handles of
  `Wraps: unaligned_16` / `Wraps: unaligned_32`)

## Finding

The layout types `Unaligned16` / `Unaligned32` implement `Debug` (through
`impl_value_traits!`), but their `Copy` shared handles do not. Every other
shared handle in the module and in the crate's wrapped value types prints:
`Alias16Ref`, `Alias32Ref`, `Alias64Ref` (`intreadwrite.rs`), `RationalRef`
(`rational.rs`), `IntFloat32Ref` / `IntFloat64Ref` (`intfloat.rs`). The
`unaligned_N` handles are the ones a consumer is most likely to hold on its
own, since `from_bytes` views a window of a byte buffer the way `AV_RN` /
`AV_WN` cast one, so a consumer struct such as

```rust
#[derive(Debug)]
struct Header<'a> { tag: Unaligned16Ref<'a>, len: Unaligned32Ref<'a> }
```

failed to compile (`Unaligned16Ref<'_>` doesn't implement `Debug`), and a
view could not be printed in an assertion message without first calling
`value()`.

This is an ergonomics and consistency gap, not a soundness or equivalence
defect: the C union has no observable formatting, and the fix adds no
`unsafe` code.

## Fix

`impl_value_traits!` now takes the shared handle too and implements `Debug`
on it (`Unaligned16(4660)`, the same rendering the value already had); the
value's `Debug` delegates to its handle, as `Alias16` does. The exclusive
handles stay without `Debug`, as every sibling `*Mut` handle in the crate.
The `unaligned_64` review (`5a5c971ca4`) moved `Unaligned64` onto the same
macro meanwhile, so after rebasing onto it `Unaligned64Ref` gets the same
`Debug` through the shared macro.

Regression evidence: the inline `shared_handles_debug_as_their_value` unit
test formats a view over a value and over bytes, in `{:?}` and `{:#x?}`, and
derives `Debug` on a struct holding one of each.

## Also added while here

The two wrappers lacked the width-rejection doctests their `Unaligned64`
sibling carries, so `Unaligned16Ref::from_bytes` and
`Unaligned32Mut::from_bytes` now carry a `compile_fail,E0308` doctest each
(a 4-byte array for the 16-bit view, a 2-byte array for the 32-bit one).

## Review verdict for the rest of the workset

No other defect was found. Verified on this batch:

- Wavefront: both type records (packed, `may_alias`, single `uintN_t l`
  member, no lifecycle routine, no function takes or returns one) match
  `libavutil/intreadwrite.h:217-221`; `--lifecycle-ops`, `--taking` and
  `--returning` are empty for both, and the recorded users
  (`mem.c` `fill16` / `fill24` / `fill32` / `av_memcpy_backptr` through
  `AV_RN16` / `AV_WN32` / `AV_COPY32U` / `AV_COPY16U`) are confirmed.
- Safe boundary: no public `unsafe fn` and no raw pointer in the two types'
  surface; the handle seams are `pub(crate)`.
- UB: ASan/UBSan, TSan (every asserted `Send` / `Sync`), and Miri (the
  FFI-free tests) pass on the `intreadwrite` unit, `ub_safe` and `equiv`
  suites; the six `compile_fail` programs that target the two types reject
  outliving, aliasing and write-through-shared.
- Equivalence: native-endian unaligned loads and stores at every offset match
  the raw `union unaligned_N` casts and the C library's `av_memcpy_backptr`
  paths that go through the unions.
- Coverage: `intreadwrite.rs` is at 100 % regions / lines / functions under
  the module's three suites, and every `mem.c` line that reads or writes
  through `unaligned_16` / `unaligned_32` executes under the `equiv` suite
  (the uncovered `back == 1` `memset` and `cnt >= 16` `memcpy` paths never
  touch the unions and belong to `av_memcpy_backptr`'s own batch).
