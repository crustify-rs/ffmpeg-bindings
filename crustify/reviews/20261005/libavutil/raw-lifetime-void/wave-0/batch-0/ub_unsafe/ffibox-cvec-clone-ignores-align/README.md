# ffibox: `CVec::clone` ignores `CLenClone::ALIGN`

- Affected crate: ffibox `main` at `508e478` (`/opt/venv/share/ffibox`), the
  path dependency of `crustify/rust`
- Affected API: `CVec::<T, S>::try_clone` and `impl Clone for CVec<T, S>`
  (`src/refs.rs`), reached safely from any `CVec` whose policy declares
  `ALIGN < align_of::<T>()`
- Sanitizer: Miri (no FFI involved); the debug build's `ub_checks` also abort
- Patch: ffibox branch `crustify/review-20261005/cvec-clone-honours-align`
  (`695d52d`), worktree `/opt/venv/share/ffibox-review-cvec-clone-honours-align`

## Finding

`CLenClone::ALIGN` is "the alignment every allocation `c_clone_len` returns is
guaranteed to have", and the copying constructors enforce it:
`copy_run` (behind `CVec::from_slice` and `CSlice::to_cvec`) rejects
`align_of::<T>() > S::ALIGN` at compile time. `CVec::try_clone`, which also
copies through `c_clone_len`, does not. A buffer adopted through
`from_raw_parts` is aligned for `T` by that function's contract, but its clone
is only as aligned as `ALIGN` vouches for, and `as_slice` / `as_handles` then
build a reference from the misaligned pointer.

Nothing in the reproducer breaks an `unsafe` contract: `Bytes` is an honest
byte allocator that keeps the default `ALIGN = 1`, and the adopted buffer holds
four aligned `u32` from that allocator. The UB happens in safe code:

```rust
let v = unsafe { CVec::<u32, Bytes>::from_raw_parts(block, 4) }.unwrap(); // contract met
let copy = v.clone();                 // safe: c_clone_len hands back an odd address
let s: &[u32] = copy.as_slice();      // safe: misaligned &[u32]
```

ffibox's own tests hit the pattern without noticing: `tests/macros.rs`
cloned `CVec<u32, MockVecFree>` under `impl_clenclone!(MockVecFree,
mock_memdup)` (ALIGN 1) and only passed because its mock allocator happened
to align to 8.

The ffmpeg crate's `Heap` declares `ALIGN = 16` and every safe `HeapVec`
constructor bounds `align_of::<T>()` by 16, so its safe surface does not reach
this; an over-aligned `HeapVec` adopted through `from_raw_parts` would.

## Reproduce

```console
$ cargo +nightly miri run --offline
copy at 0x21201, align_of::<u32>() = 4
error: Undefined Behavior: constructing invalid value of type &[u32]: encountered an unaligned reference (required 4 byte alignment but found 1)
   --> /opt/venv/share/ffibox/src/refs.rs:965:18
    |
965 |         unsafe { core::slice::from_raw_parts(self.data().as_ptr(), self.len) }
    |                  ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ Undefined Behavior occurred here
    |
    = note: stack backtrace:
            0: ffibox::CVec::<u32, Bytes>::as_slice
                at /opt/venv/share/ffibox/src/refs.rs:965:18: 965:77
            1: main
                at src/main.rs:93:21: 93:36

$ cargo run --offline
copy at 0x5dbe56c86a81, align_of::<u32>() = 4
thread 'main' panicked at /opt/venv/share/ffibox/src/refs.rs:965:18:
unsafe precondition(s) violated: slice::from_raw_parts requires the pointer to be aligned and non-null, and the total size of the slice not to exceed `isize::MAX`
thread caused non-unwinding panic. aborting.
$ echo $?
134
```

`Cargo.lock` is seeded from `crustify/rust/Cargo.lock`, so `--offline`
resolves. The ffibox path is absolute because ffibox lives outside the
repository; `crustify/rust/Cargo.toml` names it the same way.

## Patch

`try_clone` now carries the same post-monomorphization assertion as
`copy_run` (`align_of::<T>() <= S::ALIGN`), so a clone can never be less
aligned than the element type, with a `compile_fail,E0080` doctest as the
regression test. The two ffibox test policies that cloned `u32` buffers now
declare the alignment they really provide. ffibox's suite, clippy and rustfmt
pass, and the ffmpeg crate builds and passes its tests against the branch.
Against the patched ffibox the reproducer no longer compiles:

```console
$ cargo run --offline --config 'paths=["/opt/venv/share/ffibox-review-cvec-clone-honours-align"]'
error[E0080]: evaluation panicked: the policy's allocator cannot align T
note: the above error was encountered while instantiating `fn CVec::<u32, Bytes>::try_clone`
```

The regression test lives in ffibox, not in the ffmpeg crate's `ub_unsafe`
suite: the ffmpeg workspace builds against ffibox `main`, where it would fail
until the branch is merged.
