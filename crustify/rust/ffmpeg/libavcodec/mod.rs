//! Safe wrappers over `libavcodec`, one module per public header.

/// Raw bindings to `libavcodec` and the link units it depends on.
pub use libavcodec_sys as ffi;
