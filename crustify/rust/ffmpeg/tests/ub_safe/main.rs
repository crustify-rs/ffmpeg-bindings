//! UB tests against the safe public API.
#![forbid(unsafe_code)]

mod avstring;
mod avutil;
mod bprint;
mod bswap;
mod cast5;
mod crc;
mod channel_layout;
mod common;
mod cpu;
mod csp;
mod dict;
mod error;
mod eval;
mod file;
mod hash;
mod hmac;
mod imgutils;
mod intfloat;
mod intreadwrite;
mod log;
mod mathematics;
mod md5;
mod murmur3;
mod mem;
mod opt;
mod parseutils;
mod pixdesc;
mod pixelutils;
mod pixfmt;
mod random_seed;
mod rational;
mod samplefmt;
mod sha;
mod sha512;
mod time;
mod timestamp;
mod xtea;

/// Programs under `compile_fail/` must be rejected by the compiler.
#[test]
fn compile_fail() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/ub_safe/compile_fail");
    let any = std::fs::read_dir(&dir)
        .map(|d| d.flatten().any(|e| e.path().extension().is_some_and(|x| x == "rs")))
        .unwrap_or(false);
    if any {
        trybuild::TestCases::new().compile_fail(dir.join("*.rs"));
    }
}
