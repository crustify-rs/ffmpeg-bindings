//! Shared by the `frame` suites of `equiv`, `ub_safe` and `ub_unsafe`, which
//! include it with `#[path]`: side data arrays built through the safe API
//! ([`SideDataArrayMut::add`], [`SideDataMut::set_metadata`]) from specs the
//! suites also hand to C.
//!
//! [`SideDataArrayMut::add`]: ffmpeg_bindings::libavutil::libavutil::frame::SideDataArrayMut::add
//! [`SideDataMut::set_metadata`]: ffmpeg_bindings::libavutil::libavutil::frame::SideDataMut::set_metadata
#![allow(dead_code)]

use core::ffi::CStr;

use ffmpeg_bindings::libavutil::libavutil::buffer::{BufferReference, BufferReferenceBox};
use ffmpeg_bindings::libavutil::libavutil::dict::Dict;
use ffmpeg_bindings::libavutil::libavutil::ffi;
use ffmpeg_bindings::libavutil::libavutil::frame::{SideDataArray, SideDataFlags, SideDataType};

/// One entry to build: its kind, its payload, and its metadata pairs, in
/// insertion order, keys distinct.
#[derive(Clone, Copy, Debug)]
pub struct Spec<'a> {
    pub kind: SideDataType,
    pub payload: &'a [u8],
    pub metadata: &'a [(&'a CStr, &'a CStr)],
}

impl<'a> Spec<'a> {
    pub const fn new(kind: SideDataType, payload: &'a [u8]) -> Self {
        Self {
            kind,
            payload,
            metadata: &[],
        }
    }

    pub const fn with_metadata(mut self, metadata: &'a [(&'a CStr, &'a CStr)]) -> Self {
        self.metadata = metadata;
        self
    }

    /// A new buffer holding exactly the payload's bytes.
    pub fn buffer(&self) -> BufferReferenceBox {
        BufferReference::alloc(self.payload.len())
            .unwrap()
            .copy_from_slice(self.payload)
    }

    /// The metadata as a dictionary, empty when there is none.
    pub fn dict(&self) -> Dict {
        Dict::from_entries(self.metadata.iter().copied()).unwrap()
    }
}

/// The array of `specs`, in order, each added with no flag: the specs
/// repeat only `MULTI` kinds.
pub fn array(specs: &[Spec<'_>]) -> SideDataArray {
    let mut a = SideDataArray::new();
    let mut entries = a.as_mut();
    for spec in specs {
        let mut e = entries
            .add(spec.kind, spec.buffer(), SideDataFlags::NONE)
            .unwrap();
        e.set_metadata(spec.dict());
    }
    a
}

/// [`SAMPLE`]'s array.
pub fn sample() -> SideDataArray {
    array(&SAMPLE)
}

const PAGE: [u8; 4096] = {
    let mut p = [0u8; 4096];
    let mut i = 0;
    while i < p.len() {
        p[i] = (i * 7 + 3) as u8;
        i += 1;
    }
    p
};

/// A few entries of distinct kinds (and a repeated `MULTI` one), empty to
/// page-sized payloads, with and without metadata, one of a kind C does not
/// name.
pub const SAMPLE: [Spec<'static>; 6] = [
    Spec::new(SideDataType::AFD, &[0x0a]),
    Spec::new(SideDataType::A53_CC, &[]).with_metadata(&[(c"k", c"v")]),
    Spec::new(SideDataType::SEI_UNREGISTERED, b"0123456789abcdef-payload"),
    Spec::new(SideDataType::SEI_UNREGISTERED, &PAGE).with_metadata(&[
        (c"a", c"1"),
        (c"b", c"2"),
        (c"c", c"3"),
    ]),
    Spec::new(SideDataType::ICC_PROFILE, &[1, 2, 3, 4, 5, 6, 7]),
    Spec::new(
        SideDataType::from_raw(ffi::AVFrameSideDataType(u32::MAX)),
        &[0xff; 3],
    ),
];
