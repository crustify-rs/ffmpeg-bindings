//! Equivalence of `libavutil/file.h`'s `map` and `FileMap` with
//! `av_file_map` and `av_file_unmap`, mapping the same files on each side.
#![expect(
    let_underscore_drop,
    reason = "assertions keep the form of the C behaviour they check"
)]

use core::ffi::{CStr, c_int};
use std::ffi::CString;
use std::path::PathBuf;

use ffmpeg_bindings::libavutil::libavutil::ffi;
use ffmpeg_bindings::libavutil::libavutil::file::{FileMap, map};

/// A file under the temporary directory, removed on drop.
struct TempFile(PathBuf, CString);

impl TempFile {
    fn new(name: &str, contents: &[u8]) -> Self {
        let path =
            std::env::temp_dir().join(format!("crustify-equiv-file-{}-{name}", std::process::id()));
        std::fs::write(&path, contents).unwrap();
        let c = CString::new(path.to_str().unwrap()).unwrap();
        Self(path, c)
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// `av_file_map` of `name`: its bytes, or the error code, with the size C
/// reported; `av_file_unmap` releases it.
fn c_map(name: &CStr) -> (Result<Vec<u8>, c_int>, usize) {
    let (mut buf, mut size) = (core::ptr::null_mut(), 0);
    // SAFETY: `name` is a live NUL-terminated string, `buf` and `size`
    // writable out-parameters; no parent log context. Nothing writes the file
    // while it is mapped.
    let ret = unsafe {
        ffi::av_file_map(
            name.as_ptr(),
            &raw mut buf,
            &raw mut size,
            0,
            core::ptr::null_mut(),
        )
    };
    if ret < 0 {
        assert!(buf.is_null());
        return (Err(ret), size);
    }
    let bytes = if buf.is_null() {
        Vec::new()
    } else {
        // SAFETY: on success `buf` maps `size` initialised bytes.
        unsafe { core::slice::from_raw_parts(buf, size) }.to_vec()
    };
    // SAFETY: `buf` and `size` are what `av_file_map` returned, released once.
    unsafe { ffi::av_file_unmap(buf, size) };
    (Ok(bytes), size)
}

/// `map` of `name`, reported as [`c_map`] does.
fn rust_map(name: &CStr) -> (Result<Vec<u8>, c_int>, usize) {
    // SAFETY: nothing writes or truncates the file while it is mapped; no
    // parent log context.
    match unsafe { map(name, 0, None) } {
        Ok(m) => {
            let len = m.len();
            (Ok(m.as_slice().to_vec()), len)
        }
        Err(e) => (Err(e), 0),
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn mapped_contents_match_c() {
    let page: Vec<u8> = (0..4096u32).map(|i| (i * 7 % 251) as u8).collect();
    for (name, contents) in [
        ("empty", &b""[..]),
        ("one", b"x"),
        ("text", b"hello\nworld\n"),
        ("page", &page[..]),
    ] {
        let f = TempFile::new(name, contents);
        let (r, c) = (rust_map(&f.1), c_map(&f.1));
        assert_eq!(r, c, "{name}");
        assert_eq!(r.0.as_deref(), Ok(contents), "{name}");
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn failures_report_the_c_error() {
    let dir = std::env::temp_dir();
    let missing = CString::new(
        dir.join("crustify-equiv-file-missing/none")
            .to_str()
            .unwrap(),
    )
    .unwrap();
    let directory = CString::new(dir.to_str().unwrap()).unwrap();
    for name in [&missing[..], c""] {
        let (r, c) = (rust_map(name), c_map(name));
        assert!(c.0.is_err(), "{name:?}");
        assert_eq!(r.0, c.0, "{name:?}");
        assert_eq!(c.1, 0, "{name:?}: C reports no size");
    }
    // A directory opens, and fails to map unless its reported size is 0;
    // either way, alike.
    assert_eq!(rust_map(&directory), c_map(&directory));
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn remapping_after_each_rewrite_matches_c() {
    let f = TempFile::new("rewritten", b"");
    for (step, contents) in [&b"first"[..], b"", b"a longer second version", b"3"]
        .into_iter()
        .enumerate()
    {
        std::fs::write(&f.0, contents).unwrap();
        let r = rust_map(&f.1);
        let c = c_map(&f.1);
        assert_eq!(r, c, "step {step}");
        // A mapping, then dropped, then a fresh one, sees the new contents.
        let m: FileMap = {
            // SAFETY: the file is not written again until `m` drops.
            unsafe { map(&f.1, 0, None) }.unwrap()
        };
        assert_eq!(m.as_slice(), contents, "step {step}");
    }
}
