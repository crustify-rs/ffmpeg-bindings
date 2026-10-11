# Crypto wave 0, batch 4 (wrap / type)

- `AVHMACType`: resource-free transparent integer wrapper, all six named
  algorithms and unknown encodings preserved. C allocation observes the enum.
- `AVHashContext`: opaque ffibox layout, shared/exclusive handles and sole owner;
  constructor pairs allocation with initialization; reset and read-only metadata.
  Child algorithm storage is owned, CRC storage borrowed/static. No cloning,
  refcounting, field disposal, public fields, or thread traits were discovered.
- Lifecycle/ownership findings submitted with Wavefront `--update`. Raw C
  allocation alone is not an initialized hashing constructor; the safe constructor
  supplies the separate init. Later hashing symbols remain a later batch's work.
- No C/shim change. Reused immutable build.json v3 prebuilds (original FFmpeg C
  unchanged from 1019f8f036; GCC 14.2 coverage and Clang 20.1.2 ASan/UBSan/TSan;
  BSan uses its matched toolchain). Coverage outputs use private GCOV_PREFIX and
  LLVM_PROFILE_FILE; GCC 14.2 gcov was extracted privately to read matching data.

## Evidence

- Workspace check and Clippy (including all targets): pass.
- All three new unit tests, three ub_safe tests and three equivalence tests:
  pass under ASan/UBSan and TSan. Three hash ownership/reborrow compile-fail
  doctests: pass. No public unsafe API was added (ffibox adoption seams are
  tested internally), hence no ub_unsafe suite was added.
- BSan: only exact tests `hash::owners_shared_views_and_exclusive_reborrows`,
  `hash::ephemeral_algorithm_name_is_not_retained`, and
  `hash::allocation_initialization_metadata_and_reset_match`, each alone: pass.
- Miri: enum representation/conversion unit test: pass. Hash lifecycle calls C
  and cannot run under Miri; ffibox supplies its layout/borrow primitives.
- Static scan: no undocumented unsafe in changed files, no new public raw or
  unsafe smells. The private pointer-to-pointer release adapter is an intentional
  CDrop seam with an explicit Safety contract; no safe caller can invoke it.
- Coverage: authored non-test Rust lines hash 24/24, hmac 12/12. All 15 hash
  init branches and six HMAC allocation choices plus unknown values exercised.
  Relevant C lifecycle/metadata/enum-observer lines: hash 50/52, hmac 60/62.
  Remaining lines are child-allocation OOM cleanup (environment-dependent);
  top-level OOM branch edges are also not forced. Other uncovered C routines
  (name enumeration and formatted hashing/HMAC operations) are outside this
  type workset.

## Existing workspace gate limitations

Full workspace sanitizer runs execute cleanly except pre-existing diagnostic
snapshots. Nightly 1.101 changes four trybuild diagnostics (`bprint`, `intfloat`,
`intreadwrite`, `log`); stable 1.99 still mismatches two (`intfloat` and
`intreadwrite`). All these illegal programs remain rejected with the expected
lifetime errors. Nightly also changes three pre-existing compile-fail doctest
error codes (`eval::ExprBox`, `mem::Heap`, `opt::OptionRangesBox`, E0599 to E0308).
These unrelated snapshots were not edited. Runtime suites were rerun excluding
only `compile_fail`, and this batch's doctests were run independently. Rustdoc
needs the same sanitizer flag in RUSTDOCFLAGS as rustc's RUSTFLAGS.

Agent-unique logs, scans, and coverage files: `/tmp/crypto-b4/`.
