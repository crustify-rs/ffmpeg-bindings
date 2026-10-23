//! Safe wrappers over `libswresample`, one module per public header.

/// Raw bindings to `libswresample` and the link units it depends on.
pub use libswresample_sys as ffi;
