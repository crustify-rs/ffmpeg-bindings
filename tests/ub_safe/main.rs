//! UB tests against the safe public API.
#![forbid(unsafe_code)]

mod aes;
mod aes_ctr;
mod avstring;
mod avutil_h;
mod blowfish;
mod bprint;
mod bswap_h;
mod buffer;
mod cast5;
mod channel_layout;
mod common_h;
mod cpu;
mod crc;
mod csp;
mod des;
mod dict;
mod error;
mod eval;
mod file;
mod frame;
mod hash;
mod hmac;
mod imgutils;
mod intfloat_h;
mod intmath;
mod intreadwrite_h;
mod log;
mod mathematics;
mod md5;
mod mem;
mod murmur3;
mod opt;
mod parseutils;
mod pixdesc;
mod pixelutils;
mod pixfmt;
mod random_seed;
mod rational;
mod rc4;
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
    let any = std::fs::read_dir(&dir).is_ok_and(|d| {
        d.flatten()
            .any(|e| e.path().extension().is_some_and(|x| x == "rs"))
    });
    if any {
        trybuild::TestCases::new().compile_fail(dir.join("*.rs"));
    }
}

// Crypto wave 0, symbol batch 10.
mod adler32;
mod base64;
mod camellia;
mod lzo;
mod ripemd;
mod tea;
mod twofish;
mod utils;
mod uuid;
mod version;

// Buffer-frame wave 0.
mod refstruct;

// Hwcontext wave 0.
mod hwcontext;
