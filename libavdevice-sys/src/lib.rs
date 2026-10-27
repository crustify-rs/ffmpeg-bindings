//! Raw bindgen bindings to FFmpeg's `libavdevice`.
//!
//! Generated from `bindgen/wrapper.h` under `bindgen/allowlist.txt`, and shipped as
//! `src/bindings.rs`, which the `bindgen` feature regenerates against the headers found; items from
//! the link units it depends on are re-exported from their own `-sys` crates.
#![allow(
    nonstandard_style,
    missing_docs,
    dead_code,
    unused_imports,
    clippy::all,
    clippy::pedantic
)]

pub(crate) use libavcodec_sys::*;
pub use libavfilter_sys::*;
pub(crate) use libavformat_sys::*;
pub(crate) use libavutil_sys::*;
#[cfg(not(all(target_arch = "x86_64", target_os = "linux", target_env = "gnu")))]
compile_error!(
    "libavdevice-sys supports x86_64-unknown-linux-gnu only, the target its bindings are generated for"
);

/// The version of libavdevice the bindings were generated from, as its `pkg-config` file records
/// it.
pub const C_VERSION: &str = env!("C_VERSION");

#[cfg(not(crustify_regenerated))]
include!("bindings.rs");
#[cfg(crustify_regenerated)]
include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
