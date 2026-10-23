//! Functional equivalence tests of the public API against the C library.

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
