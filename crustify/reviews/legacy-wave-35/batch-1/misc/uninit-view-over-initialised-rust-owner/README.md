# Misc (soundness): an uninitialised view over an initialised Rust owner's bytes

- Items: `av_buffer_create` (`BufferReference::from_owner`) and
  `av_buffer_pool_get` (`BufferPoolRef::get`), together with the
  `TryFrom<BufferReferenceBox> for UninitBuffer` conversion that `get`
  relies on (buffer-frame wave 1 batch 0), all in
  `crustify/rust/ffmpeg/libavutil/buffer.rs`.
- Affected commit: the wave tip `83bd5f2e64`.
- Filed as `misc`, not `ub_safe`: the UB is a read of uninitialised `u8`s
  in Rust. None of the prepared instruments detects that: ASan, UBSan, TSan
  and BSan do not track initialisation, MSan is not prepared, and Miri
  cannot call `av_buffer_create`. So there is no sanitizer reproducer. The
  safe program below compiles on the affected commit and is UB by the
  language's rules.
- Status: fixed. Regressions: `buffer.rs::unit_tests::uninit_conversion_refuses_buffers_with_an_opaque`,
  the `compile_fail,E0277` doctest on `UninitBuffer::from_owner`, and
  `tests/ub_safe/buffer.rs::uninit_owner`.

## Finding

`UninitBuffer::try_from` accepted any sole, writable reference and from
then on exposed its bytes as `&mut [MaybeUninit<u8>]`. For a buffer over C
heap (`av_malloc`, freed unread by `av_free`) that is sound. For a
`BufferReference::from_owner` buffer, those bytes belong to the Rust owner
`T: AsMut<[u8]>`, which the buffer drops when it is released. Viewing a
`&mut [u8]` as `&mut [MaybeUninit<u8>]` is exactly the conversion std
withholds: safe code can now store uninitialised bytes in memory that
`T` later reads as `u8`:

```rust
#![forbid(unsafe_code)]
use core::mem::MaybeUninit;
use ffmpeg::libavutil::buffer::{BufferReference, UninitBuffer};

struct Loud(Vec<u8>);
impl AsMut<[u8]> for Loud {
    fn as_mut(&mut self) -> &mut [u8] { &mut self.0 }
}
impl Drop for Loud {
    fn drop(&mut self) { println!("{}", self.0[0]); } // reads an uninit u8
}

fn main() {
    let buf = BufferReference::from_owner(Loud(vec![1])).unwrap();
    let mut u = UninitBuffer::try_from(buf).unwrap();
    u.as_mut_slice()[0] = MaybeUninit::uninit();
    drop(u); // release_owner drops Loud: UB
}
```

Pools made the same hole wider. The established way to give pool buffers
an opaque of their own (for `get_with_opaque`) was an allocator returning
`UninitBuffer::try_from(BufferReference::from_owner(owner))`. Each recycled
buffer then hands the owner's bytes to the next user as `UninitBuffer`
again (`get`), and the pool drops the owner at flush. The `equiv` and
`ub_unsafe` suites built such allocators.

## Fix

- `TryFrom<BufferReferenceBox> for UninitBuffer` now also requires the
  buffer to have no opaque. Without one, by `BufferReference`'s invariant
  (now stated: a buffer without an opaque releases its bytes unread), the
  bytes are an `av_malloc` block that `av_free` releases: `alloc_zeroed`,
  `from_heap_vec`, and the copies `make_mut`/`resize` make. Initialised
  owners, read-only owners and pool buffers come back as `Err`.
- New `UninitBuffer::from_owner<T: AsMut<[MaybeUninit<u8>]> + Send + 'static>`
  (another `av_buffer_create` wrapper, anchored `Wraps: av_buffer_create`).
  It builds a buffer over an owner whose bytes stay `MaybeUninit<u8>` to
  the owner, so the owner's drop cannot read them as initialised without
  its own `unsafe`. It replaces the conversion in pool allocators. A
  `compile_fail,E0277` doctest pins that an owner of initialised bytes
  (`Vec<u8>`) is rejected.
- `BufferPoolRef::get` builds its `UninitBuffer` directly after the same
  writability check. A pool buffer's bytes are its allocator buffer's,
  which are either an `av_buffer_alloc` block or an allocator's own
  `UninitBuffer`, and neither release reads them.
- The pool-allocator fixtures in `tests/equiv/buffer.rs::pool_get` and
  `tests/ub_unsafe/buffer.rs` now use `UninitBuffer::from_owner` over
  `Vec<MaybeUninit<u8>>`. The `Refill` step of
  `equiv/buffer.rs::reference` now refuses buffers with an opaque on its C
  side too, as the conversion does. `ub_unsafe`'s
  `after_an_initialised_owner_wrote_them` converts a `from_heap_vec` buffer
  instead of an owner one, and `after_a_rust_owner_initialised_them` covers
  `assume_init` over an `UninitBuffer::from_owner` buffer.
