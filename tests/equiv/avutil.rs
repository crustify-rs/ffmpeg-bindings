//! Equivalence of `libavutil/avutil.h`'s [`MediaType`] and [`PictureType`]
//! with the C enums, as C observes them: through `av_get_media_type_string`
//! and `av_get_picture_type_char`, and through C-allocated storage holding
//! the values.

use core::ffi::{CStr, c_int, c_void};
use core::mem::size_of;

use ffmpeg::libavutil::avutil::{MediaType, PictureType};
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

/// Every named picture type beside its C enumerator and C letter.
const PICTURE_NAMED: [(PictureType, ffi::AVPictureType, u8); 8] = [
    (
        PictureType::NONE,
        ffi::AVPictureType::AV_PICTURE_TYPE_NONE,
        b'?',
    ),
    (PictureType::I, ffi::AVPictureType::AV_PICTURE_TYPE_I, b'I'),
    (PictureType::P, ffi::AVPictureType::AV_PICTURE_TYPE_P, b'P'),
    (PictureType::B, ffi::AVPictureType::AV_PICTURE_TYPE_B, b'B'),
    (PictureType::S, ffi::AVPictureType::AV_PICTURE_TYPE_S, b'S'),
    (
        PictureType::SI,
        ffi::AVPictureType::AV_PICTURE_TYPE_SI,
        b'i',
    ),
    (
        PictureType::SP,
        ffi::AVPictureType::AV_PICTURE_TYPE_SP,
        b'p',
    ),
    (
        PictureType::BI,
        ffi::AVPictureType::AV_PICTURE_TYPE_BI,
        b'b',
    ),
];

/// Values C may store in an `enum AVPictureType` though it names none of
/// them, negative `int`s included as their unsigned encoding.
const PICTURE_UNNAMED: [u32; 6] = [8, 9, 1 << 20, 1 << 31, u32::MAX - 1, u32::MAX];

/// The letter C gives `t`: `av_get_picture_type_char`.
fn c_letter(t: ffi::AVPictureType) -> u8 {
    // SAFETY: `av_get_picture_type_char` takes the enum by value and maps
    // every value, `'?'` for those it does not name.
    unsafe { ffi::av_get_picture_type_char(t) as u8 }
}

/// Every picture type the tests compare, named first.
fn picture_values() -> Vec<ffi::AVPictureType> {
    PICTURE_NAMED
        .iter()
        .map(|&(_, c, _)| c)
        .chain(PICTURE_UNNAMED.map(ffi::AVPictureType))
        .collect()
}

#[test]
fn picture_type_constants_are_the_c_enumerators() {
    for (t, c, letter) in PICTURE_NAMED {
        assert_eq!(t.into_raw(), c);
        assert_eq!(PictureType::from_raw(c), t);
        assert_eq!(c_letter(t.into_raw()), c_letter(c));
        assert_eq!(c_letter(t.into_raw()), letter);
    }
    for raw in PICTURE_UNNAMED {
        let t = PictureType::from_raw(ffi::AVPictureType(raw));
        assert!(PICTURE_NAMED.iter().all(|&(n, _, _)| n != t));
        assert_eq!(c_letter(t.into_raw()), b'?', "raw {raw}");
        assert_eq!(t.into_raw().0, raw);
    }
}

#[test]
fn picture_type_zeroed_storage_reads_as_c_zeroed_storage() {
    const LEN: usize = 8;
    let rust = alloc_array_zeroed::<PictureType>(LEN).unwrap();
    // SAFETY: `av_calloc` takes its sizes by value.
    let c = unsafe { ffi::av_calloc(LEN, size_of::<ffi::AVPictureType>()) }
        .cast::<ffi::AVPictureType>();
    assert!(!c.is_null());
    for (i, t) in rust.as_slice().iter().enumerate() {
        // SAFETY: `i` is in bounds of the live, zero-filled C block.
        let ci = unsafe { c.add(i).read() };
        assert_eq!(t.into_raw(), ci);
        assert_eq!(c_letter(t.into_raw()), c_letter(ci));
    }
    // A zero-filled C struct's picture type is `AV_PICTURE_TYPE_NONE`.
    assert!(rust.as_slice().iter().all(|&t| t == PictureType::NONE));
    // SAFETY: `c` is the live `av_calloc` block allocated above.
    unsafe { ffi::av_free(c.cast::<c_void>()) };
}

#[test]
fn picture_type_writes_through_each_side_agree_after_every_step() {
    let values = picture_values();
    let len = values.len();
    let mut rust = alloc_array_zeroed::<PictureType>(len).unwrap();
    // SAFETY: `av_calloc` takes its sizes by value.
    let c = unsafe { ffi::av_calloc(len, size_of::<ffi::AVPictureType>()) }
        .cast::<ffi::AVPictureType>();
    assert!(!c.is_null());
    for (step, &v) in values.iter().enumerate() {
        // Each step writes slot `step` and rotates an earlier one.
        let back = (step * 5) % len;
        let w = values[(step + 3) % len];
        rust.as_mut_slice()[step] = PictureType::from_raw(v);
        rust.as_mut_slice()[back] = w.into();
        // SAFETY: `step` and `back` are in bounds of the live C block, owned
        // here.
        unsafe {
            c.add(step).write(v);
            c.add(back).write(w);
        }
        for i in 0..len {
            // SAFETY: `i` is in bounds of the live, initialised C block.
            let ci = unsafe { c.add(i).read() };
            let ri = rust.as_slice()[i];
            assert_eq!(ri.into_raw(), ci, "step {step} slot {i}");
            assert_eq!(c_letter(ri.into_raw()), c_letter(ci));
        }
    }
    // A clone is `av_memdup`'s copy of the same encodings.
    let copy = rust.clone();
    for i in 0..len {
        // SAFETY: as above.
        let ci = unsafe { c.add(i).read() };
        assert_eq!(copy.as_slice()[i].into_raw(), ci);
    }
    // SAFETY: `c` is the live `av_calloc` block allocated above.
    unsafe { ffi::av_free(c.cast::<c_void>()) };
}

/// The version queries and `av_x_if_null`, against their raw C symbols.
mod version_and_x_if_null {
    use core::ffi::{CStr, c_void};
    use core::ptr;

    use ffmpeg::libavutil::avutil::{
        configuration, license, version, version_info, x_if_null, x_if_null_mut,
    };
    use ffmpeg::libavutil::ffi;

    #[test]
    fn version_queries_return_the_c_values() {
        // SAFETY: argument-less queries.
        let c = unsafe { ffi::avutil_version() };
        assert_eq!(version(), c);
        // SAFETY: each returns a static NUL-terminated string.
        let strings = unsafe {
            [
                ffi::av_version_info(),
                ffi::avutil_configuration(),
                ffi::avutil_license(),
            ]
        };
        let rust = [version_info(), configuration(), license()];
        for (r, c) in rust.into_iter().zip(strings) {
            assert_eq!(r.as_ptr(), c);
            // SAFETY: as above.
            assert_eq!(r, unsafe { CStr::from_ptr(c) });
        }
    }

    #[test]
    fn repeated_queries_are_stable() {
        let first = (version(), version_info(), configuration(), license());
        for _ in 0..3 {
            let again = (version(), version_info(), configuration(), license());
            assert_eq!(again, first);
            assert_eq!(again.1.as_ptr(), first.1.as_ptr());
        }
        assert!(
            license().to_bytes().starts_with(b"LGPL") || license().to_bytes().starts_with(b"GPL")
        );
        assert!(!license().to_bytes().starts_with(b"libavutil license: "));
    }

    /// `av_x_if_null(p, x)` on raw pointers.
    fn c_pick<T>(p: Option<&T>, x: Option<&T>) -> *const c_void {
        let raw = |r: Option<&T>| r.map_or(ptr::null(), |r| ptr::from_ref(r).cast::<c_void>());
        // SAFETY: `av_x_if_null` reads neither pointer.
        unsafe { ffi::av_x_if_null(raw(p), raw(x)) }.cast_const()
    }

    #[test]
    fn x_if_null_picks_as_c() {
        let (a, b) = (0x11u32, 0x22u32);
        for (p, x) in [
            (Some(&a), Some(&b)),
            (None, Some(&b)),
            (Some(&a), None),
            (None, None),
        ] {
            let r = x_if_null(p, x).map_or(ptr::null(), |r| ptr::from_ref(r).cast::<c_void>());
            assert_eq!(r, c_pick(p, x));
        }
    }

    #[test]
    fn x_if_null_mut_writes_land_where_c_points() {
        let mut mine = [[0u16; 2]; 2];
        let mut theirs = [[0u16; 2]; 2];
        for (step, (has_p, has_x)) in [(true, true), (false, true), (true, false)]
            .into_iter()
            .enumerate()
        {
            let [a, b] = &mut mine;
            let r = x_if_null_mut(has_p.then_some(a), has_x.then_some(b)).unwrap();
            r[step % 2] += 1;
            let [a, b] = &theirs;
            let c = c_pick(has_p.then_some(a), has_x.then_some(b));
            let i = usize::from(c != ptr::from_ref(a).cast());
            theirs[i][step % 2] += 1;
            assert_eq!(mine, theirs, "step {step}");
        }
    }
}
