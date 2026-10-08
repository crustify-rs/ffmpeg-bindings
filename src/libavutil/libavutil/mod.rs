//! Safe wrappers over `libavutil`, one module per public header.

/// Raw bindings to `libavutil` and the link units it depends on.
pub use libavutil_sys as ffi;

pub mod mem;
