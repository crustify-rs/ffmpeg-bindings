# Lifecycle: `AVDES` has no copy or clone strategy

- File: `crustify/rust/ffmpeg/libavutil/des.rs`
- Affected commit: `f7cdef0c4d` (wave tip `7935c403e0`)
- Item: `AVDES` (`des::Des`, owned as `DesBox = HeapBox<Des>`)

## Finding

`AVDES` is a public, resource-free struct (`uint64_t round_keys[3][16]; int
triple_des;`): no pointer field, no sub-allocation, no lock, no address
identity. C code copies it by assignment, and `av_des_crypt`/`av_des_mac`
read nothing outside the struct, so a byte copy is an independent, fully
usable context. The wrapper supports it inline (`Des::new`, `Des::zeroed`)
and boxed (`DesBox::new`, `Des::new_boxed`), but `Des` was not `Copy` and
`DesBox` therefore had no `Clone`: `Heap`'s `CDupClone<T>` (`av_memdup`)
requires `T: Copy`. Its three resource-free siblings in the same wave,
`Blowfish`, `Rc4` and `Xtea`, all derive `Clone, Copy` and so clone both
inline and boxed; DES was the odd one out, leaving a valid ownership
variant (independent copies, e.g. a keyed checkpoint reused across
messages) unrepresented.

## Fix

`define_ctype!` for `Des` now derives `Clone, Copy`, which gives `DesBox`
its `Clone` through `av_memdup`. Docs on the module and `DesBox` state it.

Regression tests:

- `des::unit_tests::inline_copies_are_independent_contexts` (FFI-free, Miri)
  and `boxed_clones_are_independent_allocations`;
- `tests/ub_safe/des.rs::copies_and_clones_outlive_their_sources`: clones
  from keyed, re-keyed and `av_des_alloc` boxes, parents dropped first,
  clones of clones, inline copies (ASan/LSan, TSan);
- `tests/equiv/des.rs::copies_and_clones_track_c_struct_copies`: a cloned
  box and an inline copy match a C struct copy of the same keyed context in
  their fields and in `av_des_crypt` output, after the originals diverge.

The wavefront finding for `AVDES` ("no cloner") is about C routines and
stays correct; nothing to resubmit.
