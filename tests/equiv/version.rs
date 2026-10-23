//! Equivalence of `libavutil/version.c`'s version queries, against their raw C symbols.

use core::ffi::CStr;

use ffmpeg_bindings::libavutil::libavutil::ffi;
use ffmpeg_bindings::libavutil::libavutil::version::{
    configuration, license, version, version_info,
};

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn version_queries_return_the_c_values() {
    // SAFETY: argument-less queries.
    let c = unsafe { ffi::avutil_version() };
    assert_eq!(version(), c);
    // SAFETY: each returns a static NUL-terminated string.
    let strings = [
        unsafe { ffi::av_version_info() },
        unsafe { ffi::avutil_configuration() },
        unsafe { ffi::avutil_license() },
    ];
    let rust = [version_info(), configuration(), license()];
    for (r, c) in rust.into_iter().zip(strings) {
        assert_eq!(r.as_ptr(), c);
        // SAFETY: as above.
        assert_eq!(r, unsafe { CStr::from_ptr(c) });
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn repeated_queries_are_stable() {
    let first = (version(), version_info(), configuration(), license());
    for _ in 0..3 {
        let again = (version(), version_info(), configuration(), license());
        assert_eq!(again, first);
        assert_eq!(again.1.as_ptr(), first.1.as_ptr());
    }
    assert!(license().to_bytes().starts_with(b"LGPL") || license().to_bytes().starts_with(b"GPL"));
    assert!(!license().to_bytes().starts_with(b"libavutil license: "));
}
