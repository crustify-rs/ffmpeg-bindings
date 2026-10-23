# BPrintBox's no-Clone doctest checked the wrong expression

Affected base: `b2cff27cd691af1482bd36e8c2edab9d5e25fcf8`.
Home: `bprint.rs`, BPrintBox compile-fail doctest.

The test requested E0599 (missing clone method), but `b: &BPrintBox` has
Clone through the blanket reference implementation. `b.clone()` cloned
the reference and failed the declared owned return type with E0308 instead.
Stable rustdoc ignores the requested error code; nightly enforces it:

```
error[E0308]: mismatched types
425 |     b.clone()
    |     ^^^^^^^^^ expected `CBox<BPrint, BPrintFinalize>`, found `&CBox<BPrint, BPrintFinalize>`
Some expected error codes were not found: ["E0599"]
```

Change the expression to `(*b).clone()` to test the owner itself. The existing
doctest is the regression test and now checks E0599 under nightly. Analogous
pre-existing doctests for HeapBox and OptionRangesBox are outside this workset;
they remain documented as whole-workspace nightly-doctest limitations.
