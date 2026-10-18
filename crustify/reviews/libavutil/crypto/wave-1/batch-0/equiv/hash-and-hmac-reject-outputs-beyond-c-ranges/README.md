# Equivalence: hash and HMAC finalizers refused outputs longer than C's size type

- Files: `crustify/rust/ffmpeg/libavutil/hash.rs`, `crustify/rust/ffmpeg/libavutil/hmac.rs`
- Affected commit: `e71508dc3c` ("wrap DES, generic hashing and HMAC
  operations safely", crypto wave-1 batch 1), the wave tip at review time
- Items: `HashContextMut::finalize_bin` (`Wraps: av_hash_final_bin`),
  `finalize_hex` (`av_hash_final_hex`), `finalize_b64` (`av_hash_final_b64`),
  `HmacMut::finalize` (`av_hmac_final`), `HmacMut::calculate` (`av_hmac_calc`)
- Kind: functional divergence from C on well-defined input (no UB on either
  side); the same class as wave 0's
  `../../../../wave-0/batch-1/equiv/base64-rejects-outputs-beyond-int-max`

## Finding

The three hash finalizers take the output size as an `int`, the two HMAC
ones as an `unsigned int`, and none ever writes more than the digest needs:

- `av_hash_final_bin(ctx, dst, size)` writes `min(size, digest)` digest bytes
  and zero-pads up to `size`;
- `av_hash_final_hex` writes at most `2 * digest + 1` bytes (`snprintf` pairs
  bounded by `size`), `av_hash_final_b64` at most
  `AV_BASE64_SIZE(digest) <= 89`;
- `av_hmac_final(c, out, outlen)` and `av_hmac_calc` check
  `outlen >= hashlen` and then write exactly `hashlen` (16..64) bytes.

A C caller holding a buffer longer than the parameter can express passes the
largest value it can (`INT_MAX` or `UINT_MAX`) and C succeeds. The wrappers
instead converted the slice length with `i32::try_from` / `u32::try_from` and
returned `AVERROR(EINVAL)` (-22) without calling C, so a valid call into a
large reused buffer failed where C succeeds. Their docs presented this as a
C limit; only the data lengths are bounded by C here (`av_hmac_update`'s
`unsigned int len`, which the wrappers still check), the output size is the
caller's choice.

## Reproduce

`src/main.rs` finalizes SHA-256 with each hash format into a `2^31`-byte
buffer and HMAC-SHA256 (`final` and `calc`) into a `2^32`-byte buffer, once
through raw C with the largest size C accepts and once through the safe
wrapper, and compares the result and the first 160 bytes. The buffers are
zeroed allocations, so only touched pages are mapped (`final_bin` pads its
whole 2 GiB); the run takes about 2 s. It builds on both sides of the fix
(`finalize_bin`'s result is compared as Debug text, since it no longer returns
a `Result`). `Cargo.lock` is seeded from `crustify/rust/Cargo.lock`; the
libraries are not on the binary's rpath:

```console
$ B=$(git rev-parse --show-toplevel)/crustify/builds/plain
$ cargo build --offline
$ LD_LIBRARY_PATH=$(printf '%s:' $B/lib*) target/debug/hash-and-hmac-reject-outputs-beyond-c-ranges
```

On the affected commit it exits 1 (`trace-affected.txt`, lines cut at 160
columns):

```text
SHA256 final_bin into 2^31 bytes:
  C    Ok(()) [a7, af, 94, bf, 93, 8e, 78, 9d, 13, 37, 8d, 57, 13, ba, 39, 6a, ...
  Rust Err(-22) [aa, aa, aa, aa, aa, aa, aa, aa, aa, aa, aa, aa, aa, aa, aa, aa, ...
  DIFFER
SHA256 final_hex into 2^31 bytes:
  C    Ok(()) [61, 37, 61, 66, 39, 34, 62, 66, ...
  Rust Err(-22) [aa, aa, aa, aa, ...
  DIFFER
SHA256 final_b64 into 2^31 bytes:
  C    Ok(()) [70, 36, 2b, 55, 76, 35, 4f, 4f, ...
  Rust Err(-22) [aa, aa, aa, aa, ...
  DIFFER
HMAC-SHA256 final into 2^32 bytes:
  C    Ok(32) [ca, 6f, a6, 40, 2b, 56, 51, 08, ...
  Rust Err(-22) [aa, aa, aa, aa, ...
  DIFFER
HMAC-SHA256 calc into 2^32 bytes:
  C    Ok(32) [ca, 6f, a6, 40, 2b, 56, 51, 08, ...
  Rust Err(-22) [aa, aa, aa, aa, ...
  DIFFER
exit 1
```

With the fix all five comparisons read `agree` and it exits 0
(`trace-fixed.txt`).

## Fix

- `hash.rs`: a private `c_capacity` offers C `min(len, INT_MAX)` bytes, as the
  base64 fix does. `finalize_hex` keeps refusing fewer than 2 bytes and
  `finalize_b64` an empty output (C writes nothing / writes `dst[-1]`);
  neither refuses large outputs any more. `finalize_bin` can no longer fail,
  so it returns `()`: it offers C `min(len, INT_MAX)` bytes and zero-fills any
  bytes beyond them itself, keeping its documented "zero-padding to the output
  length" for every slice.
- `hmac.rs`: `finalize` and `calculate` offer C `min(len, UINT_MAX)` bytes;
  data and key lengths beyond `unsigned int` are still refused with -22.

Regression tests: `tests/equiv/hash.rs`
(`beyond_int_max::text_and_binary_finals_into_outputs_beyond_int_max_follow_c`,
SHA-256, CRC32 and SHA-512 in all three formats, including the padded last
byte) and `tests/equiv/hmac.rs`
(`beyond_uint_max::finals_and_calculations_into_outputs_beyond_uint_max_follow_c`,
MD5 and SHA-512, `final` and `calc`); both return early under BSan for their
multi-GiB allocations, as the base64 test does. The inline unit tests
`hash::unit_tests::output_lengths_and_rejected_text_capacities` and
`hmac::unit_tests::length_conversion_rejects_truncation` pin `c_capacity` at
the boundaries without allocating.
