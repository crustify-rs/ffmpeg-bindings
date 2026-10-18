# Wavefront: the in-place `av_buffer_realloc` of a pool buffer went unrecorded

- Entities (symbols, `libavutil/buffer.c`): `av_buffer_realloc`,
  `av_buffer_pool_init2`, `av_buffer_pool_init`, `av_buffer_pool_get`, and
  `av_buffer_create` (a Rust-mapping update only); type `AVBufferPool`
  (fields `alloc`, `alloc2`).
- Earlier submissions: buffer-frame wave 2, translation batch 1
  (`0265949f74`), and for `pool_init`/`init2` the wave-0/1 type and symbol
  runs.
- Resubmitted: one `<symbol>.json` here per entity, through `query symbols
  --name <symbol> --file libavutil/buffer.c --update <file>`. Only `note`
  strings change. Every ownership property, nullability, mutability,
  lifetime and `lifetime` block is kept as it was.

## Finding

The records described each routine correctly in isolation, but missed how
two of them interact.

- `av_buffer_realloc` (`pbuf`): "a sole, writable, REALLOCATABLE reference
  whose window starts at its buffer's data is av_realloc'ed in place". The
  record never says this branch also covers pool buffers. Only
  `av_buffer_realloc` of an empty slot sets `BUFFER_FLAG_REALLOCATABLE`
  (`buffer.c:202`). But `pool_alloc_buffer` (`buffer.c:379-385`) takes over
  an allocator result's `opaque`/`free` and leaves its `flags_internal`. A
  pool whose allocator returns reallocatable buffers therefore hands out a
  first buffer that the in-place branch `av_realloc`s
  (`buffer.c:224-229`). That frees or shrinks the block the
  `BufferPoolEntry` still records (`buf->data`, `buffer.c:379`), which the
  pool recycles (`buffer.c:399`) and frees again at flush (`buffer.c:308`).
- `av_buffer_pool_init2` (`alloc`) and `av_buffer_pool_init` (`alloc`):
  "Results must be NULL or a sole, writable reference whose AVBuffer data
  holds at least size bytes". That is complete as C's contract, but it left
  out that a reallocatable result is a hazard for every later
  `av_buffer_realloc` of its pool buffers. The Rust wrapper built from
  these records, `BufferPool::with_allocator`, inherited the gap.
- `av_buffer_pool_get` (return): "init2 allocators: their reference as
  returned". It did not say the reference keeps its flags, so a first
  buffer can be reallocatable while recycled ones never are. Recycled
  buffers are rebuilt with flags 0 plus `BUFFER_FLAG_NO_FREE`.

Evidence: `../../ub_unsafe/pool-buffer-resized-in-place-under-its-entry/`
reproduces the use after free from a contract-abiding
`BufferPool::with_allocator(16, BufferReference::alloc_resizable)` under
ASan and BSan.

## Correction

- `av_buffer_realloc`: the in-place branch now says it applies to pool
  buffers too, and what follows. The Rust part says the wrappers run C's
  copy path themselves for a NULL window and for any buffer with an opaque
  (`BufferReferenceMut::realloc_by_copy`).
- `av_buffer_pool_init2`/`init` `alloc`: both append the flag-retention
  hazard. `init2`'s note also adds the Rust mapping (allocator buffers
  from `alloc`/`alloc_resizable`/`UninitBuffer::from_owner`, and resize
  copies for opaque buffers).
- `av_buffer_pool_get` return: says the allocator's reference keeps its
  `flags_internal`, while recycled entries are rebuilt with flags 0 and
  `BUFFER_FLAG_NO_FREE`. It also says the Rust `UninitBuffer` is built
  directly, since its bytes are never read as initialised by any release.
- `av_buffer_create` (`data`, return): adds the new
  `UninitBuffer::from_owner` (`T: AsMut<[MaybeUninit<u8>]>`) to the Rust
  mapping, and notes that `UninitBuffer::try_from` refuses buffers with an
  opaque (`../../misc/uninit-view-over-initialised-rust-owner`). This is
  not a defect of the old record.

The type record `AVBufferPool` (`libavutil/buffer_internal.h`, fields
`alloc` and `alloc2`) stated the same result contract. Both field notes
get the same hazard appended, through `query types --name AVBufferPool
--update AVBufferPool.json`. `_comment_agent` and the other fields are
unchanged.
