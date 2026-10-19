# `TmRef::zone` documented a NULL `tm_zone` as always naming the local zone

Affected base: `06a7548c5c439c49f36f434a7251d23939b53462` (core wave 2, batch 0).
Home: `crustify/rust/ffmpeg/libavutil/bprint.rs`, `TmRef::zone`
(`Handwritten` `Tm`, the `struct tm` `BPrintMut::append_strftime` formats)
and the `CZeroable for Tm` safety comment.

## Finding

The getter's doc said that for a NULL `tm_zone` "`strftime` then names the
local zone". glibc 2.39's `%Z` conversion substitutes `tzname[tm_isdst != 0]`
only when `tm_isdst >= 0`, and otherwise formats nothing; an empty
`tm_zone` string is treated like NULL. Checked with a C harness on the host
(glibc 2.39):

```
NULL zone isdst=-1: n=2 '[]'
NULL zone isdst=0: n=5 '[UTC]'
empty zone: n=5 '[UTC]'
NULL zone TZ=EST5EDT isdst=1: n=5 '[EDT]'
NULL zone TZ=EST5EDT isdst=0: n=5 '[EST]'
```

`Tm::new()` zeroes `tm_isdst`, so the common case did name the local zone,
and the equivalence suite (`strftime_matches_c_on_every_shape_and_path`, the
`%Z` steps with `isdst = -1` and no zone) already drove both sides through
the empty expansion alike. No behavioural divergence; the doc described
only one of the two outcomes.

## Fix

The doc now states both outcomes and that an empty name counts as absent;
the `CZeroable` comment says "the local zone's name (or nothing)". The
regression evidence is a doctest on `zone` formatting `[%Z]` with no zone
and `tm_isdst = -1` (`[]`), then with a named zone (`[XST]`).
