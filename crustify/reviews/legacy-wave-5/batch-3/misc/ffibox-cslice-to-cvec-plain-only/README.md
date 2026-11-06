# ffibox: `CSlice::to_cvec` is implemented for plain elements only

- Affected crate: ffibox `main` at `695d52d` (`/opt/venv/share/ffibox`), the
  path dependency of `crustify/rust`
- Affected API: `CSlice::<'_, T>::to_cvec` / `to_cvec_with` (`src/refs.rs`),
  for every `define_ctype!` type that is `Copy`; in this workset,
  `rational::Rational` (`crustify/rust/ffmpeg/libavutil/rational.rs`)
- Kind: ergonomics gap in a lifecycle form (borrowed run → owned copy); no
  unsoundness
- Patch: ffibox branch `crustify/review-20261005/cslice-to-cvec-copy-cells`
  (`196c5fb`), worktree `/opt/venv/share/ffibox-review-cslice-to-cvec-copy-cells`

## Finding

`Rational` is a `Copy` layout type: two `int`s that own nothing. Its owned
array form, `HeapVec<Rational>`, clones through `CLenClone` (`av_memdup`),
which ffibox admits for any `T: Copy`, and `HeapVec::from_slice` copies a
Rust `&[Rational]` into an `av_malloc` block on the same terms. But the
borrowed run form — `CSlice<'_, Rational>`, which `HeapVec::as_handles`
returns and which a wrapper over a C array field of `AVRational`
(`AVCodec.supported_framerates`, the `{0, 0}`-terminated list
`av_find_nearest_q_idx` takes) hands out — cannot be copied into an owner:
`CSlice::to_cvec` lives in the `impl<'a, T: CPlainElem> CSlice<'a, T>` block,
which a `CCell` type never satisfies. The method's own `where T: Copy`
clause (redundant under `CPlainElem`'s plain-data contract) and its doc
("read straight from the C object, never through a `&[T]`") show the wider
intent. The safe workaround is an element-by-element rebuild:
`alloc_array_from_fn(len, |i| run.get(i).unwrap().get())`.

Nothing about the restriction is needed for soundness: `copy_run` reads
`len` initialised elements the `CSlice` contract guarantees and asks the
policy's `c_clone_len` for a byte copy, which `T: Copy` makes `len` whole
values — the argument `CVec::clone` already relies on for the same `T`.

## Reproduce

The reproducer is the safe scenario above (`#![forbid(unsafe_code)]`). On the
affected commit it does not build, so `cargo run` exits non-zero:

```console
$ cargo run --offline
error[E0599]: the method `to_cvec` exists for struct `ffibox::refs::CSlice<'_, Rational>`, but its trait bounds were not satisfied
   --> src/main.rs:22:56
    |
 22 |     let copied: HeapVec<Rational> = owned.as_handles().to_cvec::<Heap>().unwrap();
    |                                                        ^^^^^^^ method cannot be called on `ffibox::refs::CSlice<'_, Rational>` due to unsatisfied trait bounds
    |
    = note: the following trait bounds were not satisfied:
            `Rational: ffibox::traits::CPlainElem`
$ echo $?
101
```

`Cargo.lock` is seeded from `crustify/rust/Cargo.lock`, so `--offline`
resolves. The binary links the prebuilt plain variant, which an external
package does not get an rpath for, so run it with
`LD_LIBRARY_PATH=<repo>/crustify/builds/plain/libavutil`.

## Patch

`to_cvec` and `to_cvec_with` move to an `impl<'a, T: Copy> CSlice<'a, T>`
block, with the same body and safety argument. ffibox's
`tests/cvec_from_slice.rs` gains
`to_cvec_copies_a_borrowed_run_of_wrapped_copy_objects`, which copies a run
of a `define_ctype!` `{ num, den }` type out of a C array, lets C write
through its kept pointer, and clones the copy; ffibox's suite, Miri on that
test file, clippy (`-D warnings`) and rustfmt pass. Against the branch the
reproducer builds and runs:

```console
$ LD_LIBRARY_PATH=../../../../../../../../builds/plain/libavutil \
  cargo run --offline --config 'paths=["/opt/venv/share/ffibox-review-cslice-to-cvec-copy-cells"]'
24000/1001 == 24000/1001
25/1 == 25/1
0/0 == 0/0
```

The regression test lives in ffibox, not in the ffmpeg crate's suites: the
workspace builds against ffibox `main`, where a `to_cvec` on
`CSlice<'_, Rational>` fails to compile until the branch is merged. The
ffmpeg crate's new `ub_safe` test
`rational::rust_slices_copy_into_heap_arrays_and_back` covers the borrowed
run → owned copy through the element-wise workaround, which stays valid once
`to_cvec` is available.
