//! Raw bindgen bindings to FFmpeg's `libavutil`.
//!
//! Generated from `bindgen/wrapper.h` under `bindgen/allowlist.txt`, and shipped as
//! `src/bindings.rs`, which the `bindgen` feature regenerates against the headers found; items from
//! the link units it depends on are re-exported from their own `-sys` crates, and the out-of-tree
//! types of `bindgen/external.txt` from their libraries' wrappers.
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

#[cfg(not(all(target_arch = "x86_64", target_os = "linux", target_env = "gnu")))]
compile_error!(
    "libavutil-sys supports x86_64-unknown-linux-gnu only, the target its bindings are generated for"
);

/// The version of libavutil the bindings were generated from, as its `pkg-config` file records it.
pub const C_VERSION: &str = env!("C_VERSION");

#[cfg(not(crustify_regenerated))]
include!("bindings.rs");
#[cfg(crustify_regenerated)]
include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
