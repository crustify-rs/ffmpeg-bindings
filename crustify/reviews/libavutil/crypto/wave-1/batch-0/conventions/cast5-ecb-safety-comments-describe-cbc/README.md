# Conventions: CAST5's ECB-only `crypt` SAFETY comments described an IV and CBC

- File: `crustify/rust/ffmpeg/libavutil/cast5.rs`
- Affected commit: `5f76ee7dca` (crypto wave-1 batch 0)
- Items: `Cast5Ref::crypt`, `Cast5Ref::crypt_in_place` (`Wraps: av_cast5_crypt`)

## Finding

`av_cast5_crypt(cs, dst, src, count, decrypt)` is ECB only: it has no IV
parameter and calls `decipher(cs, dst, src, NULL)`. The SAFETY comments of
both wrappers were copied from the CBC variant (`av_cast5_crypt2`): `crypt`'s
said "any IV is a separate writable 8-byte array", and `crypt_in_place`'s
relied on C "preserving ciphertext before CBC decryption writes it" and on an
"optional IV" being disjoint. The conventions require a specific, falsifiable
`// SAFETY:` comment; these named obligations the call does not have and
omitted the one it does (C reads each block before writing it). `crypt`'s
doc also did not say it is ECB.

## Fix

Both comments now state the actual argument: equal-length disjoint (or one
exclusive, exactly aliased) block buffers, no IV, C reads each block before
writing it, no retained pointer, schedule only read. `crypt`'s doc says
"in ECB mode". No code change.
