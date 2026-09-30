//! Safe wrappers over `libavformat`, one module per public header.

/// Raw bindings to `libavformat` and the link units it depends on.
pub use libavformat_sys as ffi;
