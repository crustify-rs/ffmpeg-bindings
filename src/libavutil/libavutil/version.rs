//! Safe wrappers over `libavutil/version.c`: the library's version, configuration and licence.

use core::ffi::CStr;

use super::ffi;

/// Wraps: `avutil_version`
///
/// The `LIBAVUTIL_VERSION_INT` the library was built as:
/// `major << 16 | minor << 8 | micro`.
#[must_use]
pub fn version() -> u32 {
    // SAFETY: `avutil_version` takes no arguments and returns a constant.
    unsafe { ffi::avutil_version() }
}

/// Wraps: `av_version_info`
///
/// An informative version string, usually the release number or a git
/// commit description. It has no fixed format and must not be parsed.
#[must_use]
pub fn version_info() -> &'static CStr {
    // SAFETY: as the comment below states, for this part of the operation.
    let version_info = unsafe { ffi::av_version_info() };
    // SAFETY: `av_version_info` returns the address of a string literal,
    // NUL-terminated and immutable for the life of the process.
    unsafe { CStr::from_ptr(version_info) }
}

/// Wraps: `avutil_configuration`
///
/// The `configure` command line libavutil was built with.
#[must_use]
pub fn configuration() -> &'static CStr {
    // SAFETY: as the comment below states, for this part of the operation.
    let avutil_configuration_2 = unsafe { ffi::avutil_configuration() };
    // SAFETY: `avutil_configuration` returns the address of a string literal,
    // NUL-terminated and immutable for the life of the process.
    unsafe { CStr::from_ptr(avutil_configuration_2) }
}

/// Wraps: `avutil_license`
///
/// The license libavutil was built under, such as `LGPL version 2.1 or
/// later`.
#[must_use]
pub fn license() -> &'static CStr {
    // SAFETY: as the comment below states, for this part of the operation.
    let avutil_license_2 = unsafe { ffi::avutil_license() };
    // SAFETY: `avutil_license` returns an address inside a string literal,
    // past its prefix and before its NUL, immutable for the life of the
    // process.
    unsafe { CStr::from_ptr(avutil_license_2) }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn version_packs_the_built_parts() {
        let v = version();
        assert_eq!(v >> 16, ffi::LIBAVUTIL_VERSION_MAJOR);
        assert_eq!(v >> 8 & 0xFF, ffi::LIBAVUTIL_VERSION_MINOR);
        assert_eq!(v & 0xFF, ffi::LIBAVUTIL_VERSION_MICRO);
    }
}
