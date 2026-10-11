# AVTEA / AVTWOFISH (type route, wrap)

Both worklist items are structs, not callbacks or macros. Their public headers
publish forward declarations only; all fields are private inline scalars or
integer arrays. No pointer fields, nested disposers, cloners, refcounts, locks,
retained borrows or thread-local resources exist. No field accessors are emitted.
The ffibox layout/view trio uses the public opaque binding, not a copied private
body. Neither layout is zeroable through the safe API or allocated by Rust.

`av_tea_alloc` / `av_twofish_alloc` allocate fresh zero-filled `av_mallocz`
blocks; null means failure. The matching release is `av_free` (or the equivalent
`av_freep` slot release), already bound by `mem::Heap`; constructors bind these
allocations to `HeapBox` without duplicating its release implementation. Init
copies key bytes and scalar settings into the inline layout and retains nothing.
No C copy primitive or shared-ownership variant exists. Full construction is
encapsulated; callers never promote uninitialized storage. No Send/Sync assertion
is made. Shared and exclusive borrows are ffibox handles, never Rust references
to the C object. Reinitialization requires the exclusive handle.

Inspected complete TUs and headers, their Wavefront footprints and dependencies,
C tests, and `libavformat/aadec.c`: that caller owns its allocated TEA context,
rekeys it twice, decrypts packets, and frees it in `aa_read_close` via `av_freep`.
There are no existing Rust consumers beyond the scaffold. C's private field
touchers are confined to these two TUs; C allocation/layout remains unchanged.

TEA accepts every signed round count at initialization (only a scalar store).
The fixed 16-byte key array represents its full read extent. Twofish's observed
behavior differs from imprecise header prose: negative bits return EINVAL before
mutation, nonnegative counts read `min(bits,256)>>3` bytes, including zero; partial
bytes are ignored. Key schedules pad to 128/192/256 bits and >256-bit inputs read
only 32 bytes. C returns zero for exactly 128/192/256 and one otherwise. Typed
status preserves both successes; negative counts and insufficient byte slices
are rejected before allocation or mutation. Extra bytes are permitted/ignored.
Wavefront's TEA key array is absent from pointer slots; its ownership finding
therefore documents that byte-array contract in the context slot note.

Cipher routines are not wrapped by this type batch (they are scheduled symbols).
Equivalence tests use those C readers on independently owned raw-reference and
safe-constructor contexts to observe key schedules and rekey state, CBC IV updates,
and encrypt/decrypt buffers after each call. Raw observations are equiv, not
ub_unsafe. No new public unsafe API is emitted beyond ffibox's ownership seams;
internal round-trip fixtures cover those seams with the matching heap policy.
