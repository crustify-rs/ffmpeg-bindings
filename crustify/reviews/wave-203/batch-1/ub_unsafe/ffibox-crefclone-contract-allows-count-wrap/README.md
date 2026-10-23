# ffibox: the `CRefClone` contract allows a count that wraps

- Affected crate: ffibox `main` at `196c5fb` (`/opt/venv/share/ffibox`), the
  path dependency of `crustify/rust`
- Affected API: `unsafe trait CRefClone` and `impl_crefclone!`
  (`src/traits.rs`, `src/macros.rs`): their safety contracts, and every
  `CArc` built on a policy that meets them as written
- Sanitizers: Miri, ASan and BSan (the reproducer is pure Rust)
- Patch: ffibox branch `crustify/review-20261005/crefclone-count-wrap`
  (`630f4a5`), worktree `/opt/venv/share/ffibox-review-crefclone-count-wrap`
- Found while reviewing `av_refstruct_ref`, whose `uintptr_t` count has
  the same problem on 32-bit targets: see
  `../../unsafe/refstruct-count-wrap-by-leaked-references`

## Finding

`CRefClone`'s `# Safety` section asks that a `true` from `c_up_ref` "mean the
count was actually incremented, so the object owes one more `c_drop`".
`impl_crefclone!` asks only that "the up_ref increments the reference count
of a live object". Nothing says the count must not wrap.

C reference counts are rarely checked. They rely on every reference being
stored somewhere: RefStruct's `uintptr_t` "is big enough to hold the address
of every reference". `AVBuffer`'s count is an unchecked `atomic_uint`. Safe
Rust breaks that premise, because `mem::forget(arc.clone())` leaks a counted
reference that is stored nowhere. A safe loop can increment the count without
bound, so any count narrower than the loop can run wraps. The next down-ref
then frees an object that live `CArc`s still point at. `std::sync::Arc`
aborts above `isize::MAX` for exactly this reason. ffibox leaves the question
to the policy, but its contract never raises it, so a policy that follows the
contract to the letter is unsound.

The reproducer's policy is the README's own `impl_cdrop!` +
`impl_crefclone!` pattern, over a "C" object (written in Rust so Miri can run
it) whose up-ref increments an 8-bit count without checking it. 8 bits makes
the wrap take 255 leaked clones, where a 32-bit count takes 2^32. Each
statement the contract asks for holds: the up-ref increments the count of a
live object, cannot fail and returns `()`. After that, only safe code runs:

```rust
let a = unsafe { CArc::<Obj, ObjUnref>::from_c(sys::obj_new(42)) }.unwrap(); // contract met
for _ in 0..255 {
    core::mem::forget(a.clone()); // safe: the count wraps from 255 to 0
}
drop(a.clone());                  // safe: 0 -> 1 -> 0, the object is freed
a.as_ref().value();               // safe: use after free
```

## Reproduce

```console
$ cargo +nightly miri run --offline
error: Undefined Behavior: in-bounds pointer arithmetic failed: alloc295 has been freed, so this pointer is dangling
  --> src/main.rs:60:18
   |
60 |         unsafe { addr_of!((*self.as_ptr()).value).read() }
   |                  ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ Undefined Behavior occurred here
help: alloc295 was deallocated here:
  --> src/main.rs:50:13
   |
50 |             drop(unsafe { Box::from_raw(o) });
   |             ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
   = note: stack backtrace:
           0: ObjRef::<'_>::value
           1: main
               at src/main.rs:80:28: 80:46

$ RUSTFLAGS=-Zsanitizer=address cargo +nightly run --offline --target x86_64-unknown-linux-gnu
==809558==ERROR: AddressSanitizer: heap-use-after-free on address 0x… at pc 0x… bp 0x… sp 0x…
READ of size 8 at 0x… thread T0
$ echo $?
1

$ cargo bsan run --offline
error: Undefined Behavior: trying to access an allocation that has been freed.
  --> src/main.rs:60:51
   |
60 | unsafe { addr_of!((*self.as_ptr()).value).read() }
   |                                           ^
stack backtrace:
0: core::ptr::read::<u64>
1: <*const u64>::read
2: <ffibox_crefclone_contract_allows_count_wrap::ObjRef>::value
      at src/main.rs:60:51
```

`Cargo.lock` is copied from `crustify/rust/Cargo.lock`, so `--offline`
resolves. The ffibox path is absolute because ffibox lives outside the
repository, and `crustify/rust/Cargo.toml` names it the same way.

## Patch

The fix changes documentation only, because ffibox cannot read a C count. A
new bullet in `CRefClone`'s `# Safety` section says the count must never
wrap, however many clones safe code leaks. Either `c_up_ref` returns `false`
before the count would wrap (the C routine checks), or the count is too wide
to wrap within a process's lifetime (64 bits). An unchecked 32-bit count
does not qualify. `impl_crefclone!`'s `# Safety` section carries the same
assertion. ffibox's suite and its clippy pass with `-D warnings`, and
rustfmt passes on the two files the patch touches. A
`tests/thread_markers.rs` hunk that rustfmt reports was already on `main`.

Under the patched contract the reproducer's `impl_crefclone!` binding is no
longer a valid assertion: an unchecked 8-bit up-ref breaks the new bullet.
The reproducer still compiles, because a contract cannot be checked at
compile time.

For the ffmpeg crate: RefStruct counts are `uintptr_t`, so they meet the
patched contract only on 64-bit targets. `refstruct.rs` now refuses to add
references on narrower ones (`unsafe/refstruct-count-wrap-by-leaked-references`).
`AVBuffer`'s `atomic_uint` count gets its wrappers in wave 2. Wave 2 should
check whether a 32-bit count is reachable without a per-reference allocation
(`av_buffer_ref` allocates an `AVBufferRef` for each reference, which bounds
how many references can be leaked).
