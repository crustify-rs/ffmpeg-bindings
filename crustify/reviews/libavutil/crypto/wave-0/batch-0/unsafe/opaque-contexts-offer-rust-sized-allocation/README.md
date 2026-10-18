# Safe boundary: five opaque contexts offer a Rust-sized allocator

- Files: `crustify/rust/ffmpeg/libavutil/{sha,sha512,camellia,tea,twofish}.rs`
- Affected commits: `eccfa5bb34` (`AVSHA`, `AVSHA512`), `83f598388c`
  (`AVCAMELLIA`), `24e96ff52f` (`AVTEA`, `AVTWOFISH`); wave tip `7935c403e0`
- Items: `AVSHA` (`ShaBox`), `AVSHA512` (`Sha512Box`), `AVCAMELLIA`
  (`CamelliaBox`), `AVTEA` (`TeaBox`), `AVTWOFISH` (`TwofishBox`)

## Finding

The five headers declare their contexts only forward (`struct AVSHA;`), so
bindgen emits `pub struct AVSHA { _unused: [u8; 0] }` and the layout types
are zero-sized; only C (`av_*_alloc`, `av_*_size`) knows the real size. The
five owners were aliased to `HeapBox<T> = CBox<T, Heap>`, and `Heap`
implements `CAlloc<T>` for every `T: CCell` by calling
`av_malloc(size_of::<T>())`. So each of them exposed the safe constructor
`new_uninit()`, which returns construction-phase storage of **zero** bytes
(`av_malloc(0)` hands out one byte) for a context of hundreds:

```rust
#![forbid(unsafe_code)]
let slots = (ShaBox::new_uninit(), Sha512Box::new_uninit(),
             CamelliaBox::new_uninit(), TeaBox::new_uninit(), TwofishBox::new_uninit());
// all Some(..); size_of::<Sha>() == 0
```

(`probe-output.txt`: this compiled and passed against the wave tip.)

There is no sound use of that storage: the only way forward is the
`unsafe` `assume_init`, whose obligation (C has initialised the object
through `as_c_ptr`) cannot be met without a heap overflow, because every C
initialiser (`av_sha_init`, `av_camellia_init`, ...) writes the full private
body. The API therefore offered a constructor whose only continuation is
UB, and it contradicted the types' own docs ("not by allocating the opaque
layout's Rust size"). The wave's other opaque contexts had already avoided
this with dedicated storage-only policies (`Md5Free`, `Murmur3Free`,
`RipemdFree`, `Cast5Free`, `AesFree`, `AesCtrFree`), and batch 8's
`hash_context_no_rust_sized_allocation` case asserted it for MD5 and
Murmur3 only. Not a UB report: the safe calls themselves are sound; the
defect is an unusable unsafe seam on the public surface.

## Fix

Each type gets a storage-only policy, as `RipemdFree` does: `ShaFree`,
`Sha512Free`, `CamelliaFree`, `TeaFree`, `TwofishFree`, each a `CDrop<T>`
that delegates to `Heap`'s `av_free` and implements no allocator. The box
aliases become `CBox<T, TFree>`; `CNew` for SHA and SHA-512 (which
allocates through `av_sha*_alloc` and initialises) moves from `Heap` to the
new policies, so `ShaBox::new()` / `Sha512Box::new()` keep working. Every
safe constructor, `from_c`/`into_c` and the handles are unchanged.

Regression: a `compile_fail,E0277` doctest on each box alias (`ShaBox`,
`Sha512Box`, `CamelliaBox`, `TeaBox`, `TwofishBox`, and now also `Md5Box`
and `Murmur3Box`, replacing the trybuild case, see
`misc/trybuild-e0277-snapshot-lists-calloc-implementors`) rejects
`new_uninit()`. Doctests rather than trybuild because an E0277 snapshot
lists every implementor of `CAlloc` and breaks whenever one is added.
