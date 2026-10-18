# Misc: `Unaligned64` lacked the value traits its siblings carry

- File: `crustify/rust/ffmpeg/libavutil/intreadwrite.rs`
- Affected commit: `3770f8f483` (wave head); introduced by `7a060ef8a0`
  (`unaligned_64`) and left behind by `809f4922ae` (`unaligned_16` /
  `unaligned_32`)
- Item: `unaligned_64` (`Unaligned64`, `Unaligned64Ref`, `Unaligned64Mut`)

## Finding

The three `unaligned_N` unions have the same shape (one packed native-endian
integer, no pointer, no lifecycle routine) and the module wraps them the same
way. `809f4922ae` added `impl_value_traits!`, which gives a layout type
`Debug`, `Default`, `PartialEq`, `Eq`, `Hash`, `From<int>` and
`From<Self> for int`, all through the integer, and applied it to
`Unaligned16` and `Unaligned32`. `Unaligned64`, wrapped one batch earlier,
kept only a hand-written `From<u64>`.

So on the affected commit:

```rust
use ffmpeg::libavutil::intreadwrite::{Unaligned32, Unaligned64};
assert_eq!(Unaligned32::default(), Unaligned32::new(0)); // compiles
assert_eq!(Unaligned64::default(), Unaligned64::new(0)); // E0599: no `default`, no `PartialEq`, no `Debug`
let _: u32 = Unaligned32::new(1).into();                 // compiles
let _: u64 = Unaligned64::new(1).into();                 // E0277
```

An `Unaligned64` could not be compared, printed, hashed, defaulted or
converted back to its integer, while the module docs present the three as
one family ("a value: `Unaligned16::new`, `Unaligned32::new`,
`Unaligned64::new` (or `zeroed()`)"), and a `#[derive(Debug, PartialEq,
Default)]` on any struct embedding one failed to compile. Not a soundness
issue: an ergonomics and API-parity defect.

## Fix

`impl_value_traits!(Unaligned64, u64)` replaces the hand-written
`From<u64>`; `Unaligned64::new` uses the imported `PhantomData` and its
doc matches the siblings'. Regression test
`unit_tests::unaligned64_value_traits_follow_the_integer` asserts equality,
inequality, both conversions, hash agreement with the `u64`, `default() ==
zeroed()`, the `Debug` form and `HashSet` membership. A new compile-fail
case, `tests/ub_safe/compile_fail/intreadwrite_handle_outlives_heap_box.rs`,
covers the one borrow obligation of this item the suite had not written down
(a handle into a `HeapBox<Unaligned64>` cannot outlive the box).

The change adds no unsafe code (the diff is trait impls, docs and tests);
`crustify scan-unsafe --name unaligned_64` on the fixed tree reports only
the two pre-existing raw dereference sites (`value` / `set_value`).

## Verified clean, no report filed

- Lifecycle: `wavefront query symbols --lifetime-for unaligned_64` lists no
  dropper, disposer, cloner or constructor, and a tree-wide grep finds none:
  C only ever casts a caller-owned byte pointer to the union for one
  expression (`AV_RN(64, p)` / `AV_WN(64, p, v)`). Value, borrowed view and
  `av_malloc`-family owner (`HeapBox` / `HeapVec` through the void tier) are
  the complete set.
- Safe boundary: no public `unsafe fn`; the handles' `from_ptr` /
  `as_ptr` seams are `pub(crate)`.
- UB: `from_bytes` views are built from `&[u8; 8]` / `&mut [u8; 8]`, the
  field is projected with `addr_of!` / `addr_of_mut!` and read / written
  unaligned, and the `Send` / `Sync` proofs hold (no interior mutability,
  the exclusive handle's only write needs `&mut self`). The `unit_tests`,
  `ub_safe` and `equiv` intreadwrite suites pass under ASan/UBSan, TSan,
  BSan and (FFI-free subset) Miri on the fixed tree; Rust line, region and
  function coverage of `intreadwrite.rs` is 100%.
- Equivalence: the raw `ffi::unaligned_64` reads / writes and
  `av_memcpy_backptr`'s `AV_WN64` fills agree with the safe API at every
  offset; no divergence found.
