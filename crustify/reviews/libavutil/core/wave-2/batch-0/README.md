# Review: AVBPrint appenders, initialisers and finalizer (core wave 2)

Route: **symbol** (12 items, all homed in `crustify/rust/ffmpeg/libavutil/bprint.rs`).
Objective: **review**. Affected base: `9ff8fb29d7ea1c3174abaac31e506c229f3f9115`.
No BSan gate log was filed for the wave, so there was no candidate to start from.

Items: `av_bprint_append_data` (`BPrintMut::append_bytes`), `av_bprint_chars`
(`append_repeated`), `av_bprint_clear` (`clear`), `av_bprint_escape`
(`append_escaped`), `av_bprintf` (`append_fmt`, and `fmt::Write`), `av_vbprintf`
(`append_c_str` through the `crustify_av_vbprintf` shim), `av_bprint_strftime`
(`append_strftime` with the hand-written `Tm`), `av_bprint_get_buffer`
(`append_with`), `av_bprint_is_complete` (`BPrintRef::is_complete`), and the
three lifecycle routines the core wave-1 type run emitted and this batch
re-checked: `av_bprint_init` (`BPrint::new`), `av_bprint_init_for_buffer`
(`BPrint::for_buffer`) and `av_bprint_finalize` (`BPrintFinalize`,
`into_string`, `into_bytes`).

## Findings and scope

- **No UB defect** in the safe public API, and the workset adds no public
  unsafe API (nothing to evaluate for `ub_unsafe`). Each wrapper's unsafe
  block was read against the C body: the appenders write only within `size`
  and grow only a string the buffer owns; `append_with` hands out
  `min(room - 1, size, LEN_MAX - len)` zeroed bytes at `str + len` and its
  drop guard re-terminates at `min(len, size - 1)` on every exit, so the
  `BPrint` invariants (`str` valid for `size` bytes, written and
  NUL-terminated at `min(len, size - 1)`) hold after every path, including a
  `strftime` attempt that clobbers the terminator before `av_bprint_grow(buf, 0)`
  restores it. The layout newtypes keep their fields private, no owner
  dereferences to a `BPrint`, and `BPrintOnBuffer<'b>` holds the caller
  buffer's `&'b mut [u8]` borrow (the E0503 doctest and the two compile-fail
  cases cover the borrow conflicts).
- **No equivalence defect.** Besides the existing step-by-step suites, a
  differential probe (`probe/`, kept for reuse) ran 20 000 random sequences
  on the plain build and 15 000 under ASan/UBSan over every init shape,
  every appender, C-style truncation, `av_max_alloc` limits toggled
  mid-sequence and both finalizers, comparing the full state after every
  step: no mismatch, no sanitizer report.
- **Two documentation defects, fixed** (`misc/`): `is_complete` claimed a
  count-only buffer is incomplete only once appended to, where C's
  `len < size` is false from initialisation (`clear`'s doc repeated the
  slip); `TmRef::zone` claimed a NULL `tm_zone` always names the local zone,
  where glibc does so only for `tm_isdst >= 0` and formats nothing otherwise.
  Both get doctests as regression evidence.
- **No Wavefront correction needed**: the findings for all 12 symbols record
  the borrow-for-the-call `buf`, the read-only `data`/`src`/`special_chars`/
  `fmt`/`tm` inputs, the nullable `special_chars` and `ret_str`, the
  `by_ref` out-slots of `av_bprint_get_buffer` and `av_bprint_finalize`, the
  `other`-lifetime caller buffer of `av_bprint_init_for_buffer`, and the
  count-only finalize defect, as the C bodies show. The `tm` note's "the
  local zone name is used" for a NULL `tm_zone` is the same nuance as the doc
  fix above (empty for `tm_isdst < 0`) and does not affect ownership or
  safety, so it was left as is.
- **No lifecycle work**: symbol-only route; `av_bprint_finalize` is correctly
  recorded as the disposer of a caller-stored structure.
- **No convention deviation**: clippy `--all-targets -D warnings`, rustdoc
  `-D warnings`, every unsafe block documented; `crustify scan-unsafe` shows
  no smell at any site this batch added (the `unsafe_fns_priv_smell` and
  `raw_ptr_priv_smell` sites in `bprint.rs` are the wave-1 lifecycle
  helpers `finalize_and_free`, `free_storage`, `init_with` and `finalize`,
  private and documented, and the one `unsafe_fns_pub_smell` is
  `BPrintFieldsMut::set_str_borrowed`, a wave-1 stored-borrow setter whose
  contract states the referent lifetime).

### Observation, not changed

`BPrint::into_string` / `into_bytes` consume the owner, so when
`av_bprint_finalize` fails to copy an inline string out (`AVERROR(ENOMEM)`,
only reachable through `av_max_alloc`), the string is lost with the
structure, whereas C's caller keeps a usable buffer. The consuming signature
was chosen by the wave-1 type run and accepted by its review; handing the
buffer back in the error would change a landed API for an allocation of at
most 1004 bytes, so it is noted here rather than changed.

## Regression evidence and coverage

Added: doctests on `BPrintRef::is_complete` (count-only `new`, cleared after
an append, `for_buffer(&mut [])`, and a complete automatic buffer) and on
`TmRef::zone` (`[%Z]` with no zone and `tm_isdst = -1`, then a named zone).

`coverage.json`: `bprint.c` at 100% of lines, 100% of branches executed and
96.3% taken; the untaken ones are the `INT_MAX` arm of `av_bprint_strftime`'s
room doubling (needs more than 1 GiB of room) and gcov's unwind edges.
`bprint.rs` at 100% of lines, functions and regions. `av_bprint_is_complete`
is a header inline compiled into the `-sys` shim, outside the gcov build.

## Validation

All commands ran from `crustify/rust` with private target and log paths
under `/tmp/rw2b0`; the reusable C builds were left immutable (`GCOV_PREFIX`
redirected the coverage writes). No binding, shim or C source changed, so no
C rebuild or baseline was needed.

- `cargo check --workspace`, `cargo clippy --workspace --all-targets --
  -D warnings`, `RUSTDOCFLAGS=-D warnings cargo doc -p ffmpeg`: pass.
- `cargo test --workspace`: pass (compile-fail and doctests included).
- ASan/UBSan: whole crate, `--lib --tests` under `-Zsanitizer=address`
  against `CRUSTIFY_FFMPEG_VARIANT=asan-ubsan`, `--skip compile_fail`: 897
  passed, no `runtime error`, no leak.
- TSan: same suites under `-Zsanitizer=thread` against the tsan build: pass,
  no report.
- BSan: lib, equiv, ub_safe and ub_unsafe `bprint` suites with `cargo bsan
  test` against the bsan build (`gc_hang_threshold_ms=600000`): pass.
- Miri: the eleven C-free `bprint::unit_tests` (layout, constants, the
  `BPrintFields` and `Tm` values, handles and `Debug`): pass.
- `crustify scan-unsafe <workdir> --json --sites all`: see above.
