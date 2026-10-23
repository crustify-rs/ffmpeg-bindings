//! Safe `SideDataArrayMut::add_clone` with `REPLACE`, over an entry of the
//! same non-`MULTI` kind, leaks the partial metadata copy when copying the
//! source's metadata fails after its first entry.
//!
//! C's in-place replacement path (`av_frame_side_data_clone`,
//! `libavutil/side_data.c:270-277`) copies the source's metadata into a
//! local `AVDictionary *dict` with `av_dict_copy` and, when that fails,
//! returns the error without freeing `dict`. `av_dict_copy` stops at the
//! first `av_dict_set` that fails and keeps every entry it already copied,
//! so a source with two entries whose second cannot be allocated leaves a
//! one-entry dictionary that nothing references. Here the second key is
//! larger than the `av_max_alloc` limit (`mem::set_max_alloc`, safe).
//!
//! `cargo run` exits non-zero on the affected commit, with LeakSanitizer
//! reporting the dictionary allocated under `av_frame_side_data_clone`.

#![forbid(unsafe_code)]

use std::ffi::CString;

use ffmpeg::libavutil::libavutil::dict::Dict;
use ffmpeg::libavutil::libavutil::frame::{SideDataArray, SideDataFlags, SideDataType};
use ffmpeg::libavutil::libavutil::mem::set_max_alloc;

fn main() {
    let long = CString::new(vec![b'k'; 150]).unwrap();
    let mut src = SideDataArray::new();
    let mut entry = src
        .alloc_from_slice(SideDataType::AFD, b"s", SideDataFlags::NONE)
        .expect("a source entry");
    entry.set_metadata(Dict::from_entries([(c"a", c"b"), (long.as_c_str(), c"v")]).unwrap());

    let mut dst = SideDataArray::new();
    dst.alloc_from_slice(SideDataType::AFD, b"d", SideDataFlags::NONE)
        .expect("an entry to replace");

    // The first metadata entry fits under the limit, the 151-byte key of
    // the second does not.
    set_max_alloc(100);
    let ret = dst
        .as_mut()
        .add_clone(src.as_ref().get(0).unwrap(), SideDataFlags::REPLACE);
    set_max_alloc(i32::MAX as usize);
    eprintln!("add_clone: {ret:?}");
    assert!(ret.is_err(), "the metadata copy fails");
    // The entry is untouched; both arrays are released here, so LeakSanitizer
    // sees only what C dropped.
    assert_eq!(dst.as_ref().get(0).unwrap().data(), b"d");
}
