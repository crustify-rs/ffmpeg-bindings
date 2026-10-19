# Equivalence: `base64::encode`/`decode` refused outputs longer than `INT_MAX`

- File: `crustify/rust/ffmpeg/libavutil/base64.rs`
- Affected commit: `00728268f2` ("wrap crypto checksums, encodings, UUIDs and
  immutable size globals"); wave tip at review time `c45fd5b3d1`
- Items: `encode` (`Wraps: av_base64_encode`), `decode` (`Wraps: av_base64_decode`)
- Kind: functional divergence from C on well-defined input (no UB on either side)

## Finding

`av_base64_encode(out, out_size, in, in_size)` and
`av_base64_decode(out, in, out_size)` take the output size as an `int`, and
neither ever writes more than the data needs: the encoder fails unless
`out_size >= AV_BASE64_SIZE(in_size)` and then writes exactly that many bytes,
and the decoder stops at `out + out_size` or at the end of the data. A C
caller holding a buffer longer than `INT_MAX` bytes passes `INT_MAX` (any
smaller size that covers the data gives the same result).

Both wrappers instead converted the slice length with
`i32::try_from(output.len())?` and returned `AVERROR(EINVAL)` when it did not
fit, so a valid call with a large destination (a reused multi-GiB scratch
buffer, say) failed where C succeeds. Their docs called this "an
unrepresentable C length", but the length C sees is the caller's choice; only
the *data* lengths are bounded by C (`in_size < UINT_MAX / 4` for the encoder,
and the decoded length must fit the `int` C returns).

Trace of `cargo run` in this directory on the affected commit (exit code 1):

```text
encode b"crustify" into 2^31 bytes: C "Y3J1c3RpZnk=", Rust Err(-22): DIFFER
decode c"Y3J1c3RpZnk=" into 2^31 bytes: C Ok(8) [99, 114, 117, 115, 116, 105, 102, 121], Rust Err(-22): DIFFER
```

With the fix both lines read `agree` and the program exits 0.

## Reproduce

The 2 GiB buffers are zeroed allocations, so only the touched pages are
mapped; the run takes well under a second. `Cargo.lock` is seeded from
`crustify/rust/Cargo.lock`, so `--offline` resolves. The libraries are not on
the binary's rpath:

```console
$ B=$(git rev-parse --show-toplevel)/crustify/builds/plain
$ cargo build --offline
$ LD_LIBRARY_PATH=$(printf '%s:' $B/lib*) target/debug/base64-rejects-outputs-beyond-int-max
```

## Fix

`encode` offers C `min(output.len(), INT_MAX)` bytes: the encoded size stays
below `INT_MAX` (C rejects `in_size >= UINT_MAX / 4` first, which the wrapper
mirrors), so any buffer of at least that size offers C enough. `decode` does
the same, except that when the output is longer than `INT_MAX` and the input
could decode to more than `INT_MAX` bytes (`AV_BASE64_DECODE_SIZE(strlen)`,
an input of at least 2 863 311 531 characters) it returns `AVERROR(EINVAL)`
without calling C, since C could neither write nor report that length; an
output of at most `INT_MAX` bytes truncates as before.

Regression tests: `tests/equiv/base64.rs`
(`beyond_int_max::encodes_and_decodes_into_outputs_beyond_int_max_as_c`,
skipped under BSan for its 2 GiB allocations, and
`beyond_int_max::exact_and_short_outputs_still_follow_c`) and the inline
`base64::unit_tests::outputs_beyond_int_max_offer_c_int_max`, which checks the
capacity rule at the `INT_MAX` decode boundary without allocating.
