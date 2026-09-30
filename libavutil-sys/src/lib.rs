//! Raw bindgen bindings to FFmpeg's `libavutil`.
//!
//! Generated from `wrapper.h` under `allowlist.txt`; items from the link units it
//! depends on are re-exported from their own `-sys` crates.
#![allow(non_upper_case_globals, non_camel_case_types, non_snake_case, dead_code, unused_imports, clippy::all)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
