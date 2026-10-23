//! UB tests for `libavutil/file.h`'s safe surface: the `FileMap` owner and
//! its `FileUnmap` policy, without a mapping (`map` is `unsafe`; see
//! `tests/ub_unsafe/file.rs`).
#![expect(
    trivial_casts,
    reason = "test inputs are chosen within range, and the casts mirror the C conversions under comparison"
)]

use std::thread;

use ffmpeg_bindings::libavutil::libavutil::file::{FileMap, FileUnmap};

#[test]
fn empty_map_owns_nothing_and_crosses_threads() {
    let m = FileMap::default();
    assert!(m.is_empty() && m.as_ptr().is_null());
    assert_eq!(m.as_slice(), &[] as &[u8]);
    let m = thread::spawn(move || m).join().unwrap();
    drop(m);
    assert_eq!(format!("{FileUnmap:?}"), "FileUnmap");
}
