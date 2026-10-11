# AVSHA / AVSHA512 type wrappers

Route: `type`; objective: `wrap`. C implementation, public ABI and opaque
representation are unchanged. Both bodies remain private to their C TU. Each
allocator returns one zero-filled `av_mallocz` block, not yet a usable hash;
initialization selects the algorithm and resets the count. Only AVSHA contains
a pointer: its transform is a static C function, never a heap resource. Neither
context retains input, owns sub-allocations, has a refcount, or has a C clone or
field disposer. Hash/HMAC/Mov consumers release storage with `av_free`/`av_freep`;
random_seed uses full-sized aligned scratch. Findings were submitted through
Wavefront for both types and their four allocation/initialization primitives.

`Sha`/`Sha512`, their ffibox shared/exclusive handles and `HeapBox` aliases retain
C-owned storage and reuse the existing `Heap` release strategy. Typed constructors
allocate the full C size and initialize before adoption. The `CNew` policy defaults
to SHA-256/SHA-512. Exclusive `init` admits only supported variants. No public fields,
new public unsafe functions, raw public wrapper arguments, clone, Send or Sync
assertions were added. Update/final symbols are bound only as equivalence probes;
their safe wrappers remain the symbol route's responsibility.

## Evidence

- Six inline unit tests (lengths, pointer/niche layout, ownership seam round-trip).
- Four safe integration tests (all variants, owner/shared/exclusive/reborrow paths,
  mutable/shared Debug, destruction, process-isolated allocation failure).
- Four equivalence integration tests. Independently owned raw C references observe
  constructor/default/reinitialization effects by hashing independently; checks
  include seven known `abc` digests, repeated algorithm changes, empty updates,
  padding/block boundaries and digest output cardinality.
- Four compile-fail doctests prevent borrowed handles outliving their owner and
  mutation during a shared reborrow. No workset public unsafe API needs ub_unsafe.
- Static `crustify scan-unsafe .../crustify/rust/ffmpeg --json --sites all`:
  workset raw/unsafe sites are FFI, ffibox seams and lifecycle impls; no new smell
  sites or undocumented unsafe blocks.

Reusable build.json version 3 builds were used without changing them. C matches
campaign revision 1019f8f036; no C/shim rebuild or full C baseline is needed.
All Cargo targets, sanitizer logs and GCOV/LLVM profiles are agent-private under
`/tmp/crypto-batch7`; GCOV_PREFIX keeps the shared coverage build immutable.
GCC 14.2 gcov was unpacked privately to match the prepared .gcno format.

Checks and Clippy pass. Workspace runtime tests pass under ASan/UBSan, with
compile_fail excluded from that instrument (trybuild uses compiler diagnostic
snapshots; rustdoc otherwise omits the sanitizer runtime flags). Plain workspace
validation initially passed; a later repeat had two unrelated snapshot-only
mismatches: intfloat_mut_handle_sent_past_value and intreadwrite_view_sent_past_bytes
now additionally print the std thread bound's source excerpt. Both still reject
with the expected E0597. Existing files/snapshots were not changed. The workset's
four compile-fail doctests pass. TSan passes the workset; BSan passes only the exact
`sha::owners_and_reborrows` and `sha512::owners_and_reborrows` tests, never a suite.
Miri passes the four Rust-only length/layout tests; it cannot run the C constructors.

## Coverage

Focused safe/unit/equivalence runs: C sha.c 114/114 lines and sha512.c 103/103 lines
(100% of compiled executable lines); branches taken 91.67% and 93.33%. The untaken
initializer default branches reject invalid sizes, excluded by the safe enums;
CONFIG_SMALL/big-endian alternatives are not compiled by the prepared build.
Authored non-test Rust: 28/30 lines in each TU (93.33%). Only defensive cleanup
following a nonzero initializer result is uncovered: every representable algorithm
returns zero in this C revision. All reachable constructor, OOM, reinit, Debug and
ownership paths execute.
