//! Raw bindgen bindings to FFmpeg's `libavformat`.
//!
//! Generated from `wrapper.h` under `allowlist.txt`; items from the link units it
//! depends on are re-exported from their own `-sys` crates.
#![allow(
    nonstandard_style,
    missing_docs,
    dead_code,
    unused_imports,
    clippy::all,
    clippy::pedantic
)]

pub use libavcodec_sys::*;
pub(crate) use libavutil_sys::*;
include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
