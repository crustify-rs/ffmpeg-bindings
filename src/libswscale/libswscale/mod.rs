//! Safe wrappers over `libswscale`, one module per public header.

/// Raw bindings to `libswscale` and the link units it depends on.
pub use libswscale_sys as ffi;
