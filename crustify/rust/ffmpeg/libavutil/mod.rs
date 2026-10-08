//! Safe wrappers over `libavutil`, one module per public header.

/// Raw bindings to `libavutil` and the link units it depends on.
pub use libavutil_sys as ffi;

pub mod avstring;
pub mod avutil;
pub mod bprint;
pub mod bswap;
pub mod common;
pub mod cpu;
pub mod error;
pub mod file;
pub mod intfloat;
pub mod intreadwrite;
pub mod log;
pub mod mathematics;
pub mod mem;
pub mod opt;
pub mod random_seed;
pub mod rational;
pub mod time;
pub mod timestamp;
