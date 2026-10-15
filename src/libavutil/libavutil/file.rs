//! Safe wrappers over `libavutil/file.h`.
//!
//! [`map`] maps a file's contents (`av_file_map`) into a [`FileMap`], a byte
//! buffer that `av_file_unmap` releases on drop through the [`FileUnmap`]
//! policy.

use core::ffi::{CStr, c_int};

use ffibox::{CVec, impl_clendrop};

use super::ffi;
use super::opt::ObjectRef;

/// The release policy of a [`FileMap`]: `av_file_unmap`, given the mapping's
/// size back.
///
/// Thread-agnostic: a mapping may be released on any thread.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct FileUnmap;

/// Wraps: av_file_unmap
///
/// # Safety
///
/// `ptr` and `byte_len` must be a buffer and its size exactly as
/// `av_file_map` returned them, owned by the caller.
unsafe fn unmap(ptr: *mut u8, byte_len: usize) {
    // SAFETY: the caller hands over a live `av_file_map` buffer with its
    // exact size, which `av_file_unmap` unmaps (or frees) once.
    unsafe { ffi::av_file_unmap(ptr, byte_len) }
}
impl_clendrop!(FileUnmap, unmap);

/// A file's contents as `av_file_map` maps them: a private, writable copy-on
/// write mapping (or a read copy where the platform has no `mmap`), released
/// by `av_file_unmap` on drop. An empty file maps to an empty buffer that owns
/// nothing.
pub type FileMap = CVec<u8, FileUnmap>;

/// `AVERROR(EINVAL)`.
const AVERROR_EINVAL: c_int = -(ffi::EINVAL as c_int);

/// Wraps: av_file_map
///
/// Maps the file `filename`; on failure, the `AVERROR` code C returns
/// (`AVERROR(ENOENT)` for a missing file, say), after logging it at
/// `AV_LOG_ERROR + log_offset` with `log_ctx` as the parent log context.
///
/// A `log_offset` above `INT_MAX - AV_LOG_ERROR`, which would overflow the
/// level C logs at, is rejected with `AVERROR(EINVAL)` before C runs.
///
/// `log_ctx` is the parent log context, `None` for none.
///
/// # Safety
///
/// No process may write to or truncate the file while the returned
/// [`FileMap`] lives: its bytes are pages of the file, read through `&[u8]`,
/// which other writers would change behind the borrow, and truncation
/// unmaps them.
pub unsafe fn map(
    filename: &CStr,
    log_offset: c_int,
    log_ctx: Option<ObjectRef<'_>>,
) -> Result<FileMap, c_int> {
    if log_offset > c_int::MAX - ffi::AV_LOG_ERROR as c_int {
        return Err(AVERROR_EINVAL);
    }
    let mut buf = core::ptr::null_mut();
    let mut size = 0;
    let log_ctx = log_ctx.map_or(core::ptr::null_mut(), |o| o.as_ptr());
    // SAFETY: `filename` is a live NUL-terminated string, only read; `buf`
    // and `size` are writable out-parameters; `log_ctx` is NULL or a logging
    // context, as its handle guarantees, which C only reads while logging;
    // and `log_offset` cannot overflow the error level C adds it to.
    let ret =
        unsafe { ffi::av_file_map(filename.as_ptr(), &mut buf, &mut size, log_offset, log_ctx) };
    if ret < 0 {
        return Err(ret);
    }
    // SAFETY: on success `buf` is NULL with `size == 0` (an empty file), or a
    // mapping of `size` initialised bytes the caller now owns, which
    // `av_file_unmap(buf, size)` releases; the caller keeps the file
    // unchanged while it lives.
    Ok(unsafe { FileMap::from_raw_parts_or_empty(buf, size) }
        .expect("av_file_map returns NULL only for an empty file"))
}

#[cfg(test)]
mod unit_tests {
    use super::*;
    use ffibox::CLenDrop;

    /// A file under the temporary directory, removed on drop.
    struct TempFile(std::path::PathBuf, std::ffi::CString);

    impl TempFile {
        fn new(name: &str, contents: &[u8]) -> Self {
            let path = std::env::temp_dir()
                .join(format!("crustify-unit-file-{}-{name}", std::process::id()));
            std::fs::write(&path, contents).unwrap();
            let c = std::ffi::CString::new(path.to_str().unwrap()).unwrap();
            Self(path, c)
        }
    }

    impl Drop for TempFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn overflowing_log_offsets_are_rejected_before_c() {
        let f = TempFile::new("offsets", b"data");
        for offset in [c_int::MAX, c_int::MAX - 15] {
            // SAFETY: nothing is mapped (the offset is rejected first), and
            // No log context.
            let r = unsafe { map(&f.1, offset, None) };
            assert_eq!(r.unwrap_err(), AVERROR_EINVAL);
        }
        // SAFETY: as above; this one maps, and the file is not written while
        // the mapping lives.
        let m = unsafe { map(&f.1, c_int::MAX - 16, None) }.unwrap();
        assert_eq!(m.as_slice(), b"data");
        assert_eq!(AVERROR_EINVAL, -22);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn unmap_policy_releases_a_surrendered_mapping() {
        let f = TempFile::new("surrendered", &[3; 5000]);
        // SAFETY: the file is not written while the mapping lives.
        let m = unsafe { map(&f.1, 0, None) }.unwrap();
        let (ptr, len) = m.into_raw_parts();
        assert_eq!(len, 5000);
        // SAFETY: `ptr` and `len` are the mapping `map` returned, now ours.
        unsafe { FileUnmap.c_drop_len(ptr, len) };

        // SAFETY: as above.
        let m = unsafe { map(&f.1, 0, None) }.unwrap();
        let (ptr, len) = m.into_raw_parts();
        // SAFETY: as above, through the adapter itself.
        unsafe { unmap(ptr, len) };
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn empty_files_map_to_a_null_buffer() {
        let f = TempFile::new("empty", b"");
        // SAFETY: the file is not written while the mapping lives.
        let m = unsafe { map(&f.1, 0, None) }.unwrap();
        assert!(m.is_empty() && m.as_ptr().is_null());
        fn send_sync<T: Send + Sync>() {}
        send_sync::<FileMap>();
    }
}
