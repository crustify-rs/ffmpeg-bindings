# Review: libavutil/hwcontext wave 0, batch 0 (the wave's types)

- Items: `AVHWDeviceType` and `AVHWFrameTransferDirection`
  (`libavutil/hwcontext.h`), homed in
  `crustify/rust/ffmpeg/libavutil/hwcontext.rs`:
  - `DeviceType`, an open `#[repr(transparent)]` `Copy` newtype with
    constants, `from_raw`/`into_raw`, a named `Debug`, `Default = NONE`
    and `CPlainElem`;
  - `TransferDirection`, a closed Rust `enum {From, To}` with
    `into_raw`.

  Both were wrapped by translation batch 0 (`01f8b6fdbf`).
- Reviewed commit: the wave tip `01f8b6fdbf`.
- Wave 0 is not the sub-campaign's last wave, because `schedule.json`
  lists waves 0-2 for `hwcontext`. This review emits no reproducer, so
  BSan did not run.
- Reports:
  - `misc/device-type-docs-call-the-context-type-writable`: documentation
    fixed.
  - `wavefront/device-type-context-type-recorded-as-user-writable`:
    resubmitted.

## Verdicts by step

| step | finding |
|---|---|
| lifecycle | None missing. Both are C enums passed by value: no allocation, release, clone or constructor. `query symbols --lifetime-for` returns no dropper, disposer, cloner or constructor for either type. |
| wavefront | One analysis defect, resubmitted. `AVHWDeviceType`'s note said `AVHWDeviceContext.type` is a field "users may write", and it also quoted the header's "never changed afterwards". Backends cast `hwctx` by that field, so it is read-only. The corrected note also records that `av_hwdevice_find_type_by_name` is case-sensitive, and the unchecked NULL names passed to `%s`. `AVHWFrameTransferDirection`'s record was checked against every C user and is right. |
| unsafe boundary | Clean. The workset's public API has no `unsafe fn` and no raw pointer. `from_raw`/`into_raw` take and return the bindgen enum by value. `scan-unsafe --sites` finds a single site in the home, the `unsafe impl CPlainElem for DeviceType` (line 160). Its `// SAFETY:` comment is right: the type is transparent over `c_uint`, so every bit pattern is valid, and it has no drop glue or C identity. No pub/priv smell, raw-pointer, void-pointer or deref site is in the home. |
| ub_safe / ub_unsafe | No defect. `DeviceType` accepts every bit pattern, so a C-written or C-allocated value (zeroed `av_calloc`, any `unsigned`) is always valid. `TransferDirection` is not `CPlainElem`, so C memory never reads as one (`compile_fail,E0277` doctest), and nothing converts a C value into it. The workset adds no `unsafe` API, so no `ub_unsafe` suite applies. |
| equiv | No divergence. Every constant equals its C enumerator. `Default` is C's zero. The set of values `Debug` names is exactly what C's name table names, plus `NONE`, over 0..=48 and the `unsigned` extremes. C's name and type lookups round-trip through the constants, and storage compares equal after every write. `TransferDirection` has no runnable C reader in these builds (it needs a frames context), so the suite compares it with the enumerators. That is environment-dependent, as the translator reported. |
| internal | No defect. `constant_name` covers all 15 enumerators and returns `None` otherwise. Layout, ordering, `Debug`, conversion round-trips and plain-element arrays are all unit-tested. |
| conventions | No deviation. Each item has exactly one `Wraps:` anchor plus `Handwritten`, matching the enum precedents (`HmacType`, `SideDataType`, `EscapeMode`). Naming drops the `AVHW`/`AVHWFrame` prefixes. The private `constant_name` follows the formats wave-1 rename. Tests are homed in `tests/{equiv,ub_safe}/hwcontext.rs` and `mod unit_tests`. |
| misc | One documentation defect, fixed. `DeviceType`'s docs justified the open newtype by calling `AVHWDeviceContext.type` (and libavcodec's const `AVCodecHWConfig.device_type`) user-writable. That steers hwcontext wave 1, which wraps `AVHWDeviceContext` with a `type` anchor, toward an unsound setter. The docs now give the real reasons (newer runtime enumerators, C accepting any input) and state that the field is read-only. |

## Validation

These runs used the fixed tree. The fix only changes rustdoc and a
wavefront note.

- `cargo check --workspace`, `cargo clippy --workspace --all-targets -- -D
  warnings`, `cargo test --workspace` (plain) and `cargo doc -p ffmpeg
  --no-deps` with `-D warnings`: all clean.
- The whole `ffmpeg` crate under ASan/UBSan and under TSan: lib 893, equiv
  681, ub_safe 794 (+2 trybuild, skipped under `-Zbuild-std`), ub_unsafe
  141. No sanitizer report and no `runtime error:`.
- Miri on the 8 FFI-free `hwcontext::unit_tests`: all pass.
  `device_type_arrays_are_plain_elements` calls `av_malloc` and was
  skipped.
- BSan: not run (no reproducer; not the last wave).

## Coverage

Details in `coverage.json`. Rust: `libavutil/hwcontext.rs` covers 100% of
its regions (163), functions (21) and lines (117), on the wave tip and on
the fixed tree alike. C: this workset is two enums, so it has no C path of
its own. The C readers the equivalence suite calls
(`av_hwdevice_get_type_name`, `_find_type_by_name`, `_iterate_types`) are
wave-1 symbols, and the translator found them fully executed under gcov.
In these builds `hw_table` is empty, so `av_hwdevice_iterate_types`'s
loop body and every backend path are environment-dependent.
