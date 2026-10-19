# Misc: a trybuild E0277 snapshot broke when the wave added a `CAlloc` impl

- File: `crustify/rust/ffmpeg/tests/ub_safe/compile_fail/hash_context_no_rust_sized_allocation.{rs,stderr}`
- Affected commits: `e6ffc11569` (the case, MD5/Murmur3 batch) broken by
  `876addbd5e` (`AesFree`/`AesCtrFree: CAlloc`, AES batch); wave tip `c45fd5b3d1`
- Items: `AVMD5` (`Md5Box`), `AVMurMur3` (`Murmur3Box`)

## Finding

`cargo test -p ffmpeg --test ub_safe` fails at the wave tip: the
`compile_fail` trybuild harness reports a mismatch for
`hash_context_no_rust_sized_allocation`. The case checks that
`Md5Box::new_uninit()` / `Murmur3Box::new_uninit()` do not compile, which is
right, but its `.stderr` snapshot pins rustc's E0277 help text, which lists
the implementors of `CAlloc`. It was blessed when only `Heap` implemented
it; the AES batch then added `AesFree` and `AesCtrFree`, and the expected
"the trait `CAlloc<T>` is implemented for `Heap`" became "the following
other types implement trait `CAlloc<T>`" with three entries. The test now
fails on every later wave that adds an allocator policy, whatever the
code's correctness. Batch 8 kept the mismatch in its notes
(`crustify/campaigns/.../wave-0/batch-8/trybuild-mismatches.txt`) rather
than fixing it. (Its name also refers to the hash context, which it does
not touch.)

## Fix

The trybuild case and its snapshot are removed. The same check is a
`compile_fail,E0277` doctest on `Md5Box` and `Murmur3Box`, which checks the
error code but not the implementor list, alongside the new ones on the five
opaque boxes fixed in `unsafe/opaque-contexts-offer-rust-sized-allocation`.
