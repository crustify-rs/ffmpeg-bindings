//! UB tests for `libavutil/file.h`'s unsafe `map`, each call within its
//! `# Safety` contract: no file is written while a mapping of it lives, and
//! the parent log context is NULL or a struct led by a (NULL) class pointer.

use core::ffi::{CStr, c_int, c_void};
use std::ffi::CString;
use std::path::PathBuf;
use std::thread;

use ffmpeg::libavutil::file::{FileMap, map};

/// `AVERROR(EINVAL)` and `AVERROR(ENOENT)`.
const EINVAL: c_int = -22;
const ENOENT: c_int = -2;

/// A file under the temporary directory, removed on drop.
struct TempFile(PathBuf, CString);

impl TempFile {
    fn new(name: &str, contents: &[u8]) -> Self {
        let path =
            std::env::temp_dir().join(format!("crustify-ub-file-{}-{name}", std::process::id()));
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

/// `map` with no parent log context, within the contract: callers never
/// write the file while the result lives.
fn map_plain(name: &CStr) -> Result<FileMap, c_int> {
    // SAFETY: per each caller, nothing writes or truncates the file while the
    // mapping lives; NULL is a valid log context.
    unsafe { map(name, 0, core::ptr::null_mut()) }
}

/// The path of a file that does not exist.
fn missing() -> CString {
    let p = std::env::temp_dir().join("crustify-ub-file-missing/none");
    CString::new(p.to_str().unwrap()).unwrap()
}

#[test]
fn mappings_of_every_size_are_read_and_written_end_to_end() {
    for len in [0usize, 1, 4095, 4096, 4097] {
        let contents: Vec<u8> = (0..len).map(|i| (i % 253) as u8).collect();
        let f = TempFile::new(&format!("size-{len}"), &contents);
        let mut m = map_plain(&f.1).unwrap();
        assert_eq!(m.len(), len);
        assert_eq!(m.as_slice(), &contents[..]);
        // A private mapping: writes stay in memory.
        m.as_mut_slice().fill(0xee);
        assert_eq!(m.as_slice(), &vec![0xee; len][..]);
        drop(m);
        assert_eq!(std::fs::read(&f.0).unwrap(), contents);
    }
}

#[test]
fn mappings_cross_and_share_threads() {
    let contents = vec![7u8; 3000];
    let f = TempFile::new("threads", &contents);
    let (a, b) = (map_plain(&f.1).unwrap(), map_plain(&f.1).unwrap());
    thread::scope(|s| {
        for m in [&a, &b, &a, &b] {
            s.spawn(move || assert_eq!(m.as_slice(), &[7; 3000][..]));
        }
    });
    let a = thread::spawn(move || {
        drop(b);
        a
    })
    .join()
    .unwrap();
    assert_eq!(a.len(), 3000);
}

#[test]
fn failures_leave_nothing_mapped() {
    assert_eq!(map_plain(&missing()).unwrap_err(), ENOENT);
    assert!(map_plain(c"").is_err());
}

#[test]
fn log_offsets_at_every_edge() {
    let name = missing();
    for offset in [c_int::MIN, -16, 0, 8, c_int::MAX - 16] {
        // SAFETY: the file does not exist, so nothing is mapped; NULL is a
        // valid log context. The error is logged at `16 + offset`.
        let r = unsafe { map(&name, offset, core::ptr::null_mut()) };
        assert_eq!(r.unwrap_err(), ENOENT, "{offset}");
    }
    for offset in [c_int::MAX - 15, c_int::MAX] {
        // SAFETY: as above; Rust rejects the offset before C runs.
        let r = unsafe { map(&name, offset, core::ptr::null_mut()) };
        assert_eq!(r.unwrap_err(), EINVAL, "{offset}");
    }
}

#[test]
fn parent_log_context_led_by_a_null_class() {
    /// A log context: its first member is the (NULL) `const AVClass *`.
    #[repr(C)]
    struct Parent {
        class: *const c_void,
        payload: u64,
    }
    let mut parent = Parent {
        class: core::ptr::null(),
        payload: 0,
    };
    let ctx = (&raw mut parent).cast::<c_void>();
    // SAFETY: `ctx` points to a live struct whose first member is a NULL
    // class pointer; the file does not exist, so nothing is mapped.
    assert_eq!(unsafe { map(&missing(), 0, ctx) }.unwrap_err(), ENOENT);
    let f = TempFile::new("with-parent", b"parented");
    // SAFETY: as above, and nothing writes the file while `m` lives.
    let m = unsafe { map(&f.1, -8, ctx) }.unwrap();
    assert_eq!(m.as_slice(), b"parented");
    assert_eq!(parent.payload, 0);
}
