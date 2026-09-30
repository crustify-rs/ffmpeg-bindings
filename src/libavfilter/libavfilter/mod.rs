//! Safe wrappers over `libavfilter`, one module per public header.

/// Raw bindings to `libavfilter` and the link units it depends on.
pub use libavfilter_sys as ffi;
