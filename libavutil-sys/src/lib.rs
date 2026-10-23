//! Raw bindgen bindings to FFmpeg's `libavutil`.
//!
//! Generated from `wrapper.h` under `allowlist.txt`; items from the link units it
//! depends on are re-exported from their own `-sys` crates, and the out-of-tree types of
//! `external-types.txt` from their libraries' wrappers.
#![allow(
    nonstandard_style,
    missing_docs,
    dead_code,
    unused_imports,
    clippy::all,
    clippy::pedantic
)]

/// libc's `struct tm`, as crustify-rs/glibc-bindings wraps it (`external-types.txt`).
pub use glibc_bindings::libc::time::bits::types::struct_tm_h::Tm as tm;

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
