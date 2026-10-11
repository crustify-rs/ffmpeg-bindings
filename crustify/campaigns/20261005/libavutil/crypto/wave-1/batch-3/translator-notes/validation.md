# Crypto wave 1 / batch 3: XTEA cipher symbols

Route verified: `symbol`; objective: `wrap`. Workset: `av_xtea_crypt`,
`av_xtea_init`, `av_xtea_le_crypt`, `av_xtea_le_init`, all homed in
`crustify/rust/ffmpeg/libavutil/xtea.rs`.

## Analysis and safe boundary

- The type batch already emitted `XteaMut::init` / `init_le`, their anchors,
  and safe inline constructors. They are retained rather than duplicated.
  Both initializers copy exactly sixteen key bytes into four words and leave
  the public array's remaining twelve words unchanged. No input is retained.
- Both block helpers, the CBC loop, both `CONFIG_SMALL` branches, public
  declarations, the C regression tests, `tools/crypto_bench.c`, and
  `libavformat/rtmpcrypt.c` were inspected. C's cipher context argument is
  mutable in the declaration but only read on every implementation path.
  Consequently cipher methods belong on the copyable shared `XteaRef` handle;
  their internal cast cannot cause a foreign write to shared context storage.
- Buffers borrow exactly `count * 8` bytes, with disjoint or exact in-place
  overlap. Arbitrary partial overlap is not exposed. A separate in-place
  method represents exact overlap without aliased Rust references. The IV is
  either absent (ECB) or eight exclusive, disjoint bytes (CBC). Encryption and
  decryption both advance it to the last ciphertext block; empty calls leave
  it unchanged. Decryption copies ciphertext to the IV before overwriting it.
- Negative C counts run beyond the buffers. Rust derives a nonnegative count
  after rejecting mismatched lengths, partial blocks and counts beyond `int`.
  Rejection precedes all FFI and preserves output, IV and key. Empty slices
  safely represent C's no-op count-zero path. C accepts NULL context/buffers
  only on that no-op path; metadata records this conditional nullability.
- Key-initializer endianness and block endianness remain independent choices.
  No cipher allocation, ownership transfer, retention, freeing, locking,
  callback exposure, or global state exists. The existing ffibox layout,
  shared/exclusive handles, inline values and `HeapBox` ownership are reused.
  No hand-written owner, new public unsafe function, raw public argument,
  `Send`/`Sync` assertion, C source change or compiled-shim change is introduced.
- Wavefront symbol records, schema, dependency closure and call walks were
  queried. Its extracted caller walk is empty for these symbols; textual
  inspection supplied the actual callers named above. Both cipher ownership
  findings were submitted through `query symbols --update`. Existing init
  findings already capture their lifecycle roles and extraction's omitted
  array argument. The public dependency closure contains the already wrapped
  `AVXTEA` only. Required FFI bindings were already allowlisted.

## Regression evidence

New inline tests check all validation errors, including `INT_MAX` block-count
boundaries without giant allocations, and unchanged state on rejection.
Safe integration tests exercise inline, heap, clone-after-parent-drop and array
contexts, shared and exclusive reborrows, misaligned byte buffers, zero and
multiple blocks, CBC/ECB, both endiannesses, in-place processing, canaries and
foreign-buffer reborrowing. The safe test binary forbids unsafe code.

Equivalence compares independent contexts and buffers through raw C and the
public API. The matrix combines four keys, independent initializer/block
endiannesses, ECB/CBC, both aliasing forms and 0/1/2/17 blocks. Multi-call
sequences compare output, IV and all key words after every transition, including
empty calls and chunked encryption/decryption. The previous type-only raw cipher
observation was replaced on the wrapped side with the new safe methods.

Three compile-fail cases reject aliased input/output, aliased IV/output and
reinitialization during an outstanding shared cipher handle. Existing type
compile-fail doctests establish handle keepalive and exclusive reborrow rules.
No new unsafe public API requires a `ub_unsafe` suite.

Logs, target directories and profiles are agent-unique under
`/tmp/crypto-w1-b3/`. Reusable build revision `1019f8f036`, `build.json` version 3,
and configured C compilers/instrumentation were checked: GCC 14.2 coverage/plain,
Clang 20.1.2 ASan/UBSan and TSan, and the matched BSan toolchain/plugin. The C tree
is unchanged, and the shared build directories were not modified. The LLVM 23
sanitizer linker is used for stable Rust; nightly runtime gates also instrument
Rust and rebuild std. No private C build or full C baseline is required.

### Results

- `cargo check --workspace` and `cargo clippy --workspace`: passed.
- `cargo test --workspace`: passed against the matching ASan/UBSan C library,
  including every existing/new compile-fail snapshot and all doctests. Stable
  linking used `-C linker=/usr/lib/llvm-23/bin/clang` and
  `-C link-arg=-fsanitize=address,undefined` for Rust and rustdoc, with instrumented
  generated C shims, leak detection and halt-on-UB enabled.
- Nightly ASan/UBSan and TSan: complete workspace runtime tests passed with
  matching C variants, `-Zbuild-std`, `--target x86_64-unknown-linux-gnu`, and
  `RUSTFLAGS=-Zsanitizer=address` / `-Zsanitizer=thread`. Only the compiler-snapshot
  harnesses were skipped in nightly runs; stable ran them all successfully.
- BSan: only the exact newly authored tests
  `xtea::cipher_inline_shared_handles_and_foreign_buffer_reborrows` and
  `xtea::cipher_heap_array_and_clone_keepalive` were run, separately, under
  `timeout 900 cargo +bsan bsan test ... -- --exact`. Both passed. No whole
  BSan test binary, suite or workspace was run.
- Miri: `buffer_validation_and_errors`,
  `rejected_buffers_preserve_output_iv_and_key`, and
  `ffi_free_layout_and_value_copy` all passed by exact name. Miri exercised
  rejected public-boundary storage/validation without making foreign calls.
- `crustify scan-unsafe crustify/rust/ffmpeg --json --sites all`: no unsafe,
  raw-pointer, foreign-reference, or undocumented-unsafe smell in XTEA.
- Coverage: focused XTEA unit/safe/equivalence tests passed with the reusable
  C coverage build and nightly `-C instrument-coverage`. `GCOV_PREFIX` and
  `LLVM_PROFILE_FILE` redirected every counter into private directories.
  Authored non-test Rust XTEA lines: **101/101 (100%)**. C XTEA TU:
  **136/136 lines**, **28/28 branches taken**, **6/6 calls executed**. All four
  selected C symbols and both block helpers executed every measured line.
  No reachable measured path remains uncovered. The big-endian alternate
  `CONFIG_SMALL` implementation is compiled out of the prepared builds; the
  selected small/unrolled configuration is C-owned and unchanged.
