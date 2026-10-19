# RefStruct up-refs justified "the count cannot overflow", which leaked clones break

- File: `crustify/rust/ffmpeg/libavutil/refstruct.rs`
- Affected commit: `adfecd6c81` (wave tip). The pool policy dates from `b32232d00c`.
- Items: `av_refstruct_ref`, `av_refstruct_ref_c`, `av_refstruct_replace`,
  through every wrapper that adds a counted reference:
  - `CRefClone for RefStructUnref`, which drives `RefStructArc::clone` /
    `try_clone`;
  - `CRefClone for ConstRefStructUnref`, which drives `ConstRefStructArc`'s
    clones;
  - `to_const` and `replace`;
  - `CRefClone for RefStructPoolUnref`, which drives `RefStructPoolArc`'s
    clones (from the same wave's type batch 3, a wrapper of `av_refstruct_ref`).
- Category: an incorrect `// SAFETY:` justification on an `unsafe impl`. It
  is unsound on targets narrower than 64 bits and sound on the x86_64
  campaign host.

## Finding

C increments the count unchecked (`atomic_fetch_add_explicit(..., 1,
memory_order_relaxed)` on an `atomic_uintptr_t`). Its justification is in
`RefCount`'s comment: "An uintptr_t is big enough to hold the address of
every reference, so no overflow can happen ... as long as the user does not
throw away references." The wrappers repeated that justification as a fact:

- `RefStructUnref`: "a `uintptr_t` count cannot overflow, since every
  reference would need its own address";
- `RefStructPoolUnref`: "The `uintptr_t` count cannot overflow while every
  reference is a live owner".

Safe Rust throws references away whenever it likes: `mem::forget(a.clone())`
leaks a counted reference without storing it anywhere, and `replace` into a
slot followed by `mem::forget(slot.take())` does the same. The count is then
bounded only by how many increments the process makes. With a 32-bit
`uintptr_t`, 2^32 leaked clones (seconds of a loop, no memory: RefStruct
references allocate nothing) wrap the count. The next release then frees
the object while live `RefStructArc`s still point at it: a use after free
and a double free, all from safe code. With 64 bits that takes 2^64
increments, centuries at one per nanosecond. The crate is not
64-bit-only: `xtea.rs`, `hmac.rs` and `opt.rs` keep explicit 32-bit paths.

The x86_64 campaign host has no 32-bit build, so the mechanism is
demonstrated under Miri, ASan and BSan with an 8-bit count over ffibox's
`CArc` in `../../ub_unsafe/ffibox-crefclone-contract-allows-count-wrap`.
That report also covers ffibox's `CRefClone` contract, which did not
mention wrapping.

## Fix

A private `const COUNT_IS_WIDE: bool = usize::BITS >= 64` documents the
reasoning. Every path that adds a reference is gated on it:

- the generic up-refs (`CRefClone::c_up_ref` of `RefStructUnref` and
  `ConstRefStructUnref`), `to_const` and `replace` carry
  `const { assert!(COUNT_IS_WIDE, ...) }`. The assertion is evaluated only
  where they are instantiated, so on a narrower target the crate builds and
  only code that clones or replaces a RefStruct reference fails to compile;
- `RefStructPoolUnref`'s up-ref is not generic, so a `const` assertion there
  would reject the whole crate. It returns `false` instead, which
  `try_clone` reports as `None` and `clone` aborts on. It never calls C in
  that case.

The `// SAFETY:` comments now rest on the gate, and the module docs state
the narrow-target behaviour. On the x86_64 host nothing changes: every
existing test passes unchanged.

## Evidence

The host is 64-bit, so the narrow-target branch cannot run here. To check
it, the gate was temporarily set to `usize::BITS >= 128`. Then
`cargo build -p ffmpeg --lib` still succeeded, and `cargo build -p ffmpeg
--test ub_safe` failed with nine `E0080: evaluation panicked: a RefStruct
count can wrap here`, one for each instantiation the suite makes:

```
note: the above error was encountered while instantiating `fn <RefStructUnref as CRefClone<IntFloat64>>::c_up_ref`
note: the above error was encountered while instantiating `fn <ConstRefStructUnref as CRefClone<IntFloat64>>::c_up_ref`
note: the above error was encountered while instantiating `fn to_const::<IntFloat64>`
note: the above error was encountered while instantiating `fn ffmpeg::libavutil::refstruct::replace::<IntFloat64, RefStructUnref>`
...
```

No regression test is added: a 64-bit test cannot observe either branch,
and the only instrument that could is a 32-bit build of FFmpeg and the
crate, which the campaign does not prepare.
