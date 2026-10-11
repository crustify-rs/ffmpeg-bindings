# Crypto wave 1, symbol batch 2 (wrap)

## Scope and contracts

All 25 scheduled symbols have exactly one filled anchor in their scheduled TU.
Sixteen were already implemented by the preceding type/lifecycle work and were
not duplicated. This batch adds nine missing C symbol wrappers: RC4 crypt,
RIPEMD/SHA/SHA512 update/final, and TEA/Twofish crypt, with separate in-place
forms and RC4's nullable-input keystream form. No public unsafe API is added.
No C, shim, ABI, dependency or bindgen allowlist changes are needed.

Full-size contexts remain C-owned and use their existing ffibox owner policies.
RC4 also supports its existing resource-free inline layout. Hash/RC4 operations use exclusive handles; TEA/Twofish ciphers borrow their
read-only schedules through shared handles (verified through every C helper); keys and inputs are copied/read synchronously and never
retained. Hash transform pointers name static C functions, not caller resources.
All owners release one av_malloc-family block, without field disposal.

Byte-slice lengths are checked before cipher calls. CBC IVs are exclusive
fixed-size block borrows, disjoint from data; ECB uses no IV. In-place C routines
save input/ciphertext before overwriting it. RC4's unused IV and direction are
omitted. Hash finalizers return a zero-padded maximum-capacity array, avoiding
external algorithm bookkeeping or a possibly undersized output after reset.
Empty hash inputs use an actual static byte for C's zero-length memcpy.

Wavefront records/callers/callees and C field touchers were inspected. Findings
for all scheduled symbols were submitted through `wavefront query symbols
--update`, using the prepared inventory/state and a narrowed inventory for
submission throughput. C consumers include hash/HMAC dispatch, ASF decryption,
Audible TEA decryption, MOV SHA derivation, and random-seed hashing. No input
pointer transfers, retained borrows, suballocations or thread-safety assertions
were introduced.

## Regression evidence

Agent-private logs and outputs: `/tmp/crustify-crypto-b2/`.
Reusable C builds are immutable: `crustify/build.json` version 3 matches the
orchestrator copy, all compiled C/header/configure content matches 1019f8f036,
and each variant's recorded compiler and instrumentation were checked.
No C rebuild or C baseline is required for this Rust-only wrapping change.

- Workspace check and clippy, including all-targets clippy.
- Full workspace Rust/C ASan/UBSan runtime regression gate, with leak detection
  and UBSan halt-on-error enabled.
- TSan: all scheduled TUs and simultaneous independent thread-local context
  construction, hashing, cipher operations and destruction.
- BSan: only six exact-name new `batch2_foreign_writes_across_reborrows` tests,
  one per newly wrapped TU. Each reborrows context/data after foreign writes,
  changes/drops inputs after use, and exercises nullable/in-place variants.
- Miri: the three new FFI-free count-validation boundary unit tests.
- Equivalence: independent C contexts versus public Rust methods, resets,
  streaming/checkpoint sequences, every digest variant, CBC/ECB, both cipher
  directions, in-place/out-of-place/keystream, unaligned buffers, preserved
  suffixes, return statuses and updated IV/state after each cipher call.
- Six new unit tests check length boundaries and rejection without mutation;
  nine new safe UB tests and two compile-fail doctests cover the new surface.
- Static safety scan and enumerated smell sites: none in the selected TUs.

The native stable compiler passes all 193 existing trybuild rejection snapshots;
nightly and source-free SDK experiments have unrelated snapshot excerpt/wording
drift. Full `cargo test --workspace` under ASan/UBSan was executed: runtime and
trybuild suites pass, but three pre-existing doctests (mem::Heap, eval::ExprBox,
opt::OptionRangesBox) demand E0599 where Rust correctly emits E0308 when cloning
a reference to a non-Clone CBox. These unrelated annotations were not changed.
A follow-up workspace gate skips exactly those three diagnostics. Rustdoc also
needs `RUSTDOCFLAGS=-Zsanitizer=address` when consuming ASan-instrumented crates.

## Coverage

C gcov and Rust LLVM coverage use the campaign coverage build, with agent-unique
`GCOV_PREFIX` and `LLVM_PROFILE_FILE`; shared `.gcda` files are never modified.
Selected C executable lines: Murmur3 35/35, RC4 28/28, RIPEMD 73/73,
SHA 64/64, SHA512 77/77, TEA 25/25, Twofish 46/46, XTEA alloc 2/2
(total 450/450). All selected reachable C branches are covered. RC4's non-byte
key error and SHA/SHA512's unsupported algorithm branches cannot be reached
through the typed safe initializers. CONFIG_SMALL alternatives are not compiled
in the prepared build.

Rust coverage reaches all newly added wrapper paths and length validation.
The only uncovered production lines in these TUs are the pre-existing
RIPEMD/SHA/SHA512 constructor cleanup branches for an initializer rejecting a
supported enum value. They require a different/mismatched C implementation,
not a safe input to the campaign build.
