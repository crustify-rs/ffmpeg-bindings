# Conventions: symbol wrappers appended after the `unit_tests` module

- Files: `crustify/rust/ffmpeg/libavutil/{aes,cast5,hash,md5,sha,sha512,crc}.rs`
- Affected commit: `00728268f2` (crypto wave-0 batch 10)
- Items: `context_size` (`av_aes_size`, `av_cast5_size`, `av_md5_size`,
  `av_sha_size`, `av_sha512_size`), `hash::name` (`av_hash_names`),
  `md5::sum` (`av_md5_sum`), `crc::Table` (`av_crc_init`, `av_crc`)

## Finding

The type batches of the same wave had already closed these homes with an
inline `#[cfg(test)] mod unit_tests`; the symbol batch appended its wrappers
after it. The convention is a unit-test module *beside* the translated unit,
i.e. after the items it tests, and `cargo clippy --workspace --all-targets`
warns on six of the files:

```text
warning: items after a test module
   --> ffmpeg/libavutil/aes.rs:166:1
    |
166 | mod unit_tests {
    | ^^^^^^^^^^^^^^
(same for cast5.rs:142, hash.rs:142, md5.rs:100, sha512.rs:145, sha.rs:143)
```

`crc.rs` had the same shape (`TableSize` and `Table` after the module that
tests them), though the lint does not fire there.

## Fix

Each wrapper block moved, unchanged, above its file's `unit_tests` module;
in `crc.rs` the module moved to the end. No code changed. `cargo clippy
--workspace --all-targets` no longer reports `items_after_test_module`.
