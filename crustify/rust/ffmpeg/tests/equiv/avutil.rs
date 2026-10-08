//! Equivalence of `libavutil/avutil.h`'s [`MediaType`] with the C enum, as C
//! observes it: through `av_get_media_type_string` and through C-allocated
//! storage holding the values.

use core::ffi::{CStr, c_int, c_void};
use core::mem::size_of;

use ffmpeg::libavutil::avutil::MediaType;
use ffmpeg::libavutil::ffi;
use ffmpeg::libavutil::mem::{HeapVec, alloc_array_zeroed};

/// Every named constant beside its C enumerator and C name.
const NAMED: [(MediaType, ffi::AVMediaType, Option<&str>); 6] = [
    (
        MediaType::UNKNOWN,
        ffi::AVMediaType::AVMEDIA_TYPE_UNKNOWN,
        None,
    ),
    (
        MediaType::VIDEO,
        ffi::AVMediaType::AVMEDIA_TYPE_VIDEO,
        Some("video"),
    ),
    (
        MediaType::AUDIO,
        ffi::AVMediaType::AVMEDIA_TYPE_AUDIO,
        Some("audio"),
    ),
    (
        MediaType::DATA,
        ffi::AVMediaType::AVMEDIA_TYPE_DATA,
        Some("data"),
    ),
    (
        MediaType::SUBTITLE,
        ffi::AVMediaType::AVMEDIA_TYPE_SUBTITLE,
        Some("subtitle"),
    ),
    (
        MediaType::ATTACHMENT,
        ffi::AVMediaType::AVMEDIA_TYPE_ATTACHMENT,
        Some("attachment"),
    ),
];

/// Values C may store though it names none of them, `AVMEDIA_TYPE_NB` included.
const UNNAMED: [c_int; 7] = [c_int::MIN, -1000, -2, 5, 6, 1 << 20, c_int::MAX];

/// What C calls `t`: `av_get_media_type_string`, NULL as `None`.
fn c_name(t: ffi::AVMediaType) -> Option<&'static str> {
    // SAFETY: `av_get_media_type_string` takes the enum by value and accepts
    // any value.
    let name = unsafe { ffi::av_get_media_type_string(t) };
    if name.is_null() {
        return None;
    }
    // SAFETY: a non-NULL result is one of the static string literals in
    // `utils.c`.
    Some(unsafe { CStr::from_ptr(name) }.to_str().unwrap())
}

/// A C block of `ffi::AVMediaType`, freed with `av_free` on drop.
struct CTypes(*mut ffi::AVMediaType, usize);

impl CTypes {
    fn zeroed(len: usize) -> Self {
        // SAFETY: `av_calloc` takes its sizes by value.
        let ptr = unsafe { ffi::av_calloc(len, size_of::<ffi::AVMediaType>()) };
        assert!(!ptr.is_null());
        Self(ptr.cast(), len)
    }

    fn get(&self, i: usize) -> ffi::AVMediaType {
        assert!(i < self.1);
        // SAFETY: `i` is in bounds of the live, initialised block.
        unsafe { self.0.add(i).read() }
    }

    fn set(&mut self, i: usize, t: ffi::AVMediaType) {
        assert!(i < self.1);
        // SAFETY: `i` is in bounds of the live block, which we own.
        unsafe { self.0.add(i).write(t) }
    }
}

impl Drop for CTypes {
    fn drop(&mut self) {
        // SAFETY: `self.0` is a live `av_calloc` block we own.
        unsafe { ffi::av_free(self.0.cast::<c_void>()) }
    }
}

/// The C values a Rust array holds, read back through its C-typed pointer.
fn rust_as_c(a: &HeapVec<MediaType>) -> Vec<ffi::AVMediaType> {
    let ptr = a.as_ptr().cast::<ffi::AVMediaType>();
    (0..a.len())
        // SAFETY: `MediaType` is `#[repr(transparent)]` over `ffi::AVMediaType`,
        // and `i` is in bounds of the live, initialised array `a` borrows.
        .map(|i| unsafe { ptr.add(i).read() })
        .collect()
}

fn c_values(c: &CTypes) -> Vec<ffi::AVMediaType> {
    (0..c.1).map(|i| c.get(i)).collect()
}

#[test]
fn constants_are_the_c_enumerators() {
    for (t, c, name) in NAMED {
        assert_eq!(t.into_raw(), c);
        assert_eq!(MediaType::from_raw(c), t);
        assert_eq!(c_name(t.into_raw()), c_name(c));
        assert_eq!(c_name(t.into_raw()), name);
    }
    assert_eq!(
        MediaType::COUNT,
        ffi::AVMediaType::AVMEDIA_TYPE_NB.0 as usize
    );
}

#[test]
fn index_is_some_exactly_where_c_names_the_type() {
    let all = NAMED.iter().map(|&(_, c, _)| c.0).chain(UNNAMED);
    for raw in all.chain(-3..=8) {
        let t = MediaType::from_raw(ffi::AVMediaType(raw));
        assert_eq!(
            t.index().is_some(),
            c_name(t.into_raw()).is_some(),
            "raw {raw}"
        );
        assert_eq!(t.into_raw().0, raw);
    }
}

#[test]
fn zeroed_storage_reads_as_c_zeroed_storage() {
    const LEN: usize = 8;
    let rust = alloc_array_zeroed::<MediaType>(LEN).unwrap();
    let c = CTypes::zeroed(LEN);
    assert_eq!(rust_as_c(&rust), c_values(&c));
    // A zero-filled C struct's media type is `AVMEDIA_TYPE_VIDEO`.
    assert!(rust.as_slice().iter().all(|&t| t == MediaType::VIDEO));
}

#[test]
fn writes_through_each_side_agree_after_every_step() {
    let values: Vec<ffi::AVMediaType> = NAMED
        .iter()
        .map(|&(_, c, _)| c)
        .chain(UNNAMED.map(ffi::AVMediaType))
        .collect();
    let len = values.len();
    let mut rust = alloc_array_zeroed::<MediaType>(len).unwrap();
    let mut c = CTypes::zeroed(len);
    for (step, &v) in values.iter().enumerate() {
        // Each step writes slot `step` and rotates an earlier one.
        rust.as_mut_slice()[step] = MediaType::from_raw(v);
        c.set(step, v);
        let back = (step * 7) % len;
        let w = values[(step + 3) % len];
        rust.as_mut_slice()[back] = w.into();
        c.set(back, w);
        assert_eq!(rust_as_c(&rust), c_values(&c), "step {step}");
        for i in 0..len {
            assert_eq!(
                rust.as_slice()[i].into_raw(),
                c.get(i),
                "step {step} slot {i}"
            );
            assert_eq!(c_name(rust.as_slice()[i].into_raw()), c_name(c.get(i)));
        }
    }
}

#[test]
fn values_c_writes_are_read_back_unchanged() {
    let values: Vec<_> = NAMED
        .map(|(_, c, _)| c)
        .into_iter()
        .chain(UNNAMED.map(ffi::AVMediaType))
        .collect();
    let mut c = CTypes::zeroed(values.len());
    for (i, &v) in values.iter().enumerate() {
        c.set(i, v);
    }
    // The path a field accessor over a C array of `enum AVMediaType` takes:
    // adopt the C block and read it as `MediaType`s.
    let len = c.1;
    let ptr = c.0;
    core::mem::forget(c);
    // SAFETY: `ptr` is a live `av_calloc` block of `len` initialised
    // `ffi::AVMediaType`, whose ownership `forget` gave up; `MediaType` is
    // layout-compatible with it, and `av_free` releases it.
    let adopted = unsafe { HeapVec::<MediaType>::from_raw_parts(ptr.cast(), len) }.unwrap();
    for (i, &v) in values.iter().enumerate() {
        assert_eq!(adopted.as_slice()[i], MediaType::from_raw(v));
        assert_eq!(adopted.as_slice()[i].index().is_some(), c_name(v).is_some());
    }
}

#[test]
fn clone_copies_the_c_encoding() {
    let src: Vec<MediaType> = NAMED
        .map(|(t, _, _)| t)
        .into_iter()
        .chain(UNNAMED.map(|r| ffi::AVMediaType(r).into()))
        .collect();
    let rust = HeapVec::from_slice(&src).unwrap();
    let copy = rust.clone();
    let mut c = CTypes::zeroed(src.len());
    for (i, t) in src.iter().enumerate() {
        c.set(i, t.into_raw());
    }
    assert_eq!(rust_as_c(&copy), c_values(&c));
    assert_eq!(rust_as_c(&rust), rust_as_c(&copy));
}
