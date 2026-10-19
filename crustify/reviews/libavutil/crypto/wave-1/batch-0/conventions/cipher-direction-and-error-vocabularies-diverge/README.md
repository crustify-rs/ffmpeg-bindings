# Conventions: the wave's cipher wrappers used three direction/error vocabularies

- Files: `crustify/rust/ffmpeg/libavutil/{des,tea,twofish,xtea,rc4}.rs` (and
  their `tests/equiv` and `tests/ub_safe` suites)
- Affected commits: `1ab47283f1` (XTEA, batch 3), `ca48444648` (RC4, TEA,
  Twofish, batch 2), `0896e41211` (DES, batch 1)
- Items: `DesRef::{crypt, crypt_in_place, crypt_zeros, mac}`,
  `TeaRef::{crypt, crypt_in_place}`, `TwofishRef::{crypt, crypt_in_place}`,
  `XteaRef::{crypt, crypt_in_place, crypt_le, crypt_le_in_place}`,
  `Rc4Mut::{crypt, crypt_in_place, keystream}`

## Finding

Batch 0 introduced `aes::Direction` and `aes::CryptError` (with the private
`aes::buffer_count`) and used them for AES, AES-CTR, Blowfish, Camellia and
CAST5. The batches that landed beside it each picked their own vocabulary for
the same two concepts:

| wrappers | direction | rejected buffers |
|---|---|---|
| AES, AES-CTR, Blowfish, Camellia, CAST5 | `aes::Direction` | `aes::CryptError` |
| XTEA | its own `xtea::Direction` | its own `xtea::CryptError` (`PartialBlock`, `TooManyBlocks` for `IncompleteBlock`, `CountOverflow`) |
| DES, TEA, Twofish | `decrypt: bool` | `Err(-22)` |
| RC4 | (none) | `Err(-22)` |

The C functions behind the `-22` wrappers return `void`: the code is not
C's, it was invented by the wrapper to mean "Rust refused the buffer shape",
which is what `CryptError` names. `decrypt: bool` and `-22` transliterate the
C signature where the naming convention asks for the idiomatic Rust term, and
here the subsystem already had it. A consumer driving several ciphers had to
convert between two identical `Direction` enums and match three error types.

## Fix

All five now use the subsystem's types from `aes.rs`:

- XTEA's duplicate `Direction`, `CryptError` and `block_count` are removed in
  favour of `aes::{Direction, CryptError, buffer_count}`
  (`PartialBlock` -> `IncompleteBlock`, `TooManyBlocks` -> `CountOverflow`).
- DES, TEA and Twofish crypt wrappers take `direction: Direction` and return
  `Result<_, CryptError>`; DES's `mac` returns `Result<[u8; 8], CryptError>`.
- RC4's `crypt`, `crypt_in_place` and `keystream` return `CryptError`, like
  AES-CTR's stream wrappers.
- TEA, Twofish and RC4 keep their documented acceptance of an output longer
  than the input (only `src.len()` bytes change); a shorter output is
  `LengthMismatch`, a partial block `IncompleteBlock`, a count beyond C's
  `int` `CountOverflow`. Validation order and the "no mutation on error"
  guarantee are unchanged.
- `Direction`'s and `CryptError`'s docs now say they are shared by the crypto
  cipher wrappers.

Not changed: the initializers (`Des::new(key, decrypt)`, whose flag C
ignores, and the `i32`-coded `init`/`from_key` errors of DES and RC4) are
wave-0 lifecycle API outside this wave's cipher wrappers.

Tests: the unit tests in each home assert the typed errors; the equiv suites
map C's `decrypt` flag with a small `direction(bool)` helper; `ub_safe` and
the three XTEA `compile_fail` cases import `aes::Direction` (their `.stderr`
snapshots are unchanged).
