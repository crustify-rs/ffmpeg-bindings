# Review: libavutil/buffer-frame wave 0, batch 1 (the wave's symbols)

- Items (7), all from translation batch 5 (`2161c1d053`):
  - `av_buffer_default_free`, homed in `crustify/rust/ffmpeg/libavutil/buffer.rs`;
  - `av_refstruct_allocz`, `av_refstruct_exclusive`, `av_refstruct_ref`,
    `av_refstruct_ref_c`, `av_refstruct_replace` and `av_refstruct_unref`,
    homed in `crustify/rust/ffmpeg/libavutil/refstruct.rs`.

  `av_refstruct_ref` and `av_refstruct_unref` also back type batch 3's
  `RefStructPoolArc` policy (`aa7314ae80`). Those two wrappers were reviewed
  here as well.
- Reviewed commit: the wave tip `2161c1d053`.
- Wave 0 is not the sub-campaign's last wave (`schedule.json` lists waves
  0-4 for `buffer-frame`), so BSan ran only on this review's reproducer.
- Reports:
  - `unsafe/refstruct-count-wrap-by-leaked-references` (fixed)
  - `ub_unsafe/ffibox-crefclone-contract-allows-count-wrap` (reproducer;
    ffibox patch on branch `crustify/review-20261005/crefclone-count-wrap`)

## Verdicts by step

| step | finding |
|---|---|
| lifecycle | Not applicable: the worklist has symbols only. The policies these symbols implement were read against `refstruct.c`. `RefStructUnref` and `ConstRefStructUnref` release with `av_refstruct_unref`. They clone with `av_refstruct_ref` / `_ref_c`, and `RefStructUnref` also has the zeroed allocator `av_refstruct_allocz`. `default_free` releases a `HeapVec` as `av_free` does. Nothing is missing for wave 0. `av_refstruct_alloc_ext` and its release callbacks arrive in wave 1, and the `CDrop` safety comment will then need its "no release callback" premise extended. |
| wavefront | All 7 records are submitted and correct. `unref`'s and `replace`'s `void *` slot is recorded as a by-ref owned reference. `ref` / `ref_c` are recorded as up-ref cloners, `allocz` as the allocating constructor, `exclusive` as a non-null borrow, and `default_free` as a dropper of a nullable `av_malloc` array that ignores its opaque. Nothing was resubmitted. |
| unsafe boundary | No public `unsafe fn` and no raw pointer in a public signature in the workset. `scan-unsafe` finds no smell site in the workset's lines, before or after. (`refstruct.rs`'s six `raw_ptr_pub_smell` sites are `RefStructOpaque`'s cookie accessors, type batch 3, which are deliberate.) One `// SAFETY:` justification was wrong. Both up-ref policies, and the pool's, claimed the `uintptr_t` count "cannot overflow". Safe code can leak clones with `mem::forget`, so the count is unbounded and wraps on 32-bit targets. Fixed by gating every reference-adding path on a 64-bit count (`unsafe/refstruct-count-wrap-by-leaked-references`). |
| ub_safe / ub_unsafe | No UB reachable through the safe API on the campaign host. Points checked against the C: `get_mut`/`try_into_box` rely on `av_refstruct_exclusive`'s acquire load, which pairs with the acq_rel decrement and sees every counted reference; there are no weak or C-held references in wave 0. A `const` reference never yields `get_mut` and never converts back. `replace` leaves equal slots untouched, as C does. Payloads come only from `av_refstruct_allocz` (zeroed, `T: CZeroable`, aligned to at least 16 by `REFCOUNT_OFFSET`, also when `REFSTRUCT_CHECKED` makes it 48). `Send`/`Sync` follow `Arc` (`T: Send + Sync`). Payload resources leak as `HeapBox`'s do, as documented. The workset adds no public unsafe API, so it needs no `ub_unsafe` suite. The count-wrap finding also exposed a gap in ffibox's `CRefClone` contract, reproduced under Miri, ASan and BSan with an 8-bit count (`ub_unsafe/ffibox-crefclone-contract-allows-count-wrap`). |
| equiv | No divergence. The 400-step random sequences compare presence, identity, exclusivity and payload after every step against raw C pointers, covering every routine and both reference kinds. The zeroed bytes, `replace`'s release and `default_free` are also compared. |
| internal | No defect. The unit tests drive both policies' raw seams, the pool policy and the allocator's alignment. |
| conventions | No deviation. One `Wraps:` anchor per item, duplicated only where several wrappers represent one routine (the up-ref and down-ref policies, `to_const`, `is_exclusive`); `Handwritten` on the hand-written policies. `CArc` is foreign, so the operations on references are free functions. |
| misc | None. `default_free(v)` is equivalent to `drop(v)` for a `HeapVec`, as its doc says. It stays as the wrapper of the C callback. |

## Coverage

Details in `coverage.json`. C: every line of `av_refstruct_{ref,ref_c,unref,
replace,exclusive,alloc_ext_c}` and `av_buffer_default_free` runs. Rust:
`refstruct.rs` covers 99.66% of its lines and 100% of its functions. The one
missed line is the pool up-ref's narrow-target refusal, which cannot run on
the 64-bit host. `default_free` is fully covered. No reachable gap remained,
so no coverage test was added.

## Validation at the landed tree

- `cargo check --workspace` and `cargo clippy --workspace --all-targets -- -D warnings`
  are clean. `RUSTDOCFLAGS=-D warnings cargo doc -p ffmpeg --no-deps` is
  clean. `rustfmt` is clean on `refstruct.rs`.
- `cargo test --workspace` (plain, with trybuild `compile_fail` and the
  doctests): all pass.
- ASan/UBSan and TSan over the whole `ffmpeg` crate (`-Zbuild-std`,
  `--lib --tests --skip compile_fail`): all binaries pass, with no
  `runtime error:` and no sanitizer report.
- The narrow-target gate was checked by temporarily setting it to
  `usize::BITS >= 128`. The library still builds, and every clone,
  `to_const` and `replace` instantiation in `ub_safe` fails with `E0080`.
- Miri: not applicable to the workset, since every path calls C. The ffibox
  reproducer is pure Rust and fails under Miri, ASan and BSan
  (`cargo bsan run`, alone).
- `scan-unsafe`: counts are unchanged apart from code lines and statements,
  and the number of undocumented blocks is unchanged.
