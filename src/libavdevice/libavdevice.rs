//! Safe wrappers over `libavdevice`, one module per public header.

/// Raw bindings to `libavdevice` and the link units it depends on.
pub use libavdevice_sys as ffi;
