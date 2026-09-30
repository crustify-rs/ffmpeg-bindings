//! Safe Rust wrappers over FFmpeg's public C API, one module per link unit.

#[cfg(feature = "libavutil")]
pub mod libavutil;
#[cfg(feature = "libswresample")]
pub mod libswresample;
#[cfg(feature = "libswscale")]
pub mod libswscale;
#[cfg(feature = "libavcodec")]
pub mod libavcodec;
#[cfg(feature = "libavformat")]
pub mod libavformat;
#[cfg(feature = "libavfilter")]
pub mod libavfilter;
#[cfg(feature = "libavdevice")]
pub mod libavdevice;
