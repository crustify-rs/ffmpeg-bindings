//! Equivalence of `libavutil/utils.c`'s [`MediaType`] and [`PictureType`]
//! with the C enums, as C observes them: through `av_get_media_type_string`
//! and `av_get_picture_type_char`, and through C-allocated storage holding
//! the values.
#![expect(
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "test inputs are chosen within range, and the casts mirror the C conversions under comparison"
)]

use core::ffi::{CStr, c_int, c_void};
use core::mem::size_of;

use ffmpeg_bindings::libavutil::libavutil::ffi;
use ffmpeg_bindings::libavutil::libavutil::mem::{HeapVec, alloc_array_zeroed};
use ffmpeg_bindings::libavutil::libavutil::utils::{MediaType, PictureType};

/// Every named constant beside its C enumerator and C name.
const NAMED: [(MediaType, ffi::AVMediaType, Option<&str>); 6] = [
    (MediaType::UNKNOWN, ffi::AVMEDIA_TYPE_UNKNOWN, None),
    (MediaType::VIDEO, ffi::AVMEDIA_TYPE_VIDEO, Some("video")),
    (MediaType::AUDIO, ffi::AVMEDIA_TYPE_AUDIO, Some("audio")),
    (MediaType::DATA, ffi::AVMEDIA_TYPE_DATA, Some("data")),
    (
        MediaType::SUBTITLE,
        ffi::AVMEDIA_TYPE_SUBTITLE,
        Some("subtitle"),
    ),
    (
        MediaType::ATTACHMENT,
        ffi::AVMEDIA_TYPE_ATTACHMENT,
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
        // SAFETY: as the comment below states, for this part of the operation.
        let v_0_add_2 = unsafe { self.0.add(i) };
        // SAFETY: `i` is in bounds of the live, initialised block.
        unsafe { v_0_add_2.read() }
    }

    fn set(&mut self, i: usize, t: ffi::AVMediaType) {
        assert!(i < self.1);
        // SAFETY: as the comment below states, for this part of the operation.
        let v_0_add = unsafe { self.0.add(i) };
        // SAFETY: `i` is in bounds of the live block, which we own.
        unsafe { v_0_add.write(t) }
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
        .map(|i| {
            // SAFETY: `i` is in bounds of the array `ptr` points into.
            let at = unsafe { ptr.add(i) };
            // SAFETY: `MediaType` is `#[repr(transparent)]` over `ffi::AVMediaType`,
            // and `i` is in bounds of the live, initialised array `a` borrows.
            unsafe { at.read() }
        })
        .collect()
}

fn c_values(c: &CTypes) -> Vec<ffi::AVMediaType> {
    (0..c.1).map(|i| c.get(i)).collect()
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn constants_are_the_c_enumerators() {
    for (t, c, name) in NAMED {
        assert_eq!(t.into_raw(), c);
        assert_eq!(MediaType::from_raw(c), t);
        assert_eq!(c_name(t.into_raw()), c_name(c));
        assert_eq!(c_name(t.into_raw()), name);
    }
    assert_eq!(MediaType::COUNT, ffi::AVMEDIA_TYPE_NB as usize);
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn index_is_some_exactly_where_c_names_the_type() {
    let all = NAMED.iter().map(|&(_, c, _)| c).chain(UNNAMED);
    for raw in all.chain(-3..=8) {
        let t = MediaType::from_raw(raw);
        assert_eq!(
            t.index().is_some(),
            c_name(t.into_raw()).is_some(),
            "raw {raw}"
        );
        assert_eq!(t.into_raw(), raw);
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn zeroed_storage_reads_as_c_zeroed_storage() {
    const LEN: usize = 8;
    let rust = alloc_array_zeroed::<MediaType>(LEN).unwrap();
    let c = CTypes::zeroed(LEN);
    assert_eq!(rust_as_c(&rust), c_values(&c));
    // A zero-filled C struct's media type is `AVMEDIA_TYPE_VIDEO`.
    assert!(rust.as_slice().iter().all(|&t| t == MediaType::VIDEO));
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn writes_through_each_side_agree_after_every_step() {
    let values: Vec<ffi::AVMediaType> = NAMED.iter().map(|&(_, c, _)| c).chain(UNNAMED).collect();
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
#[cfg_attr(miri, ignore = "calls C")]
fn values_c_writes_are_read_back_unchanged() {
    let values: Vec<_> = NAMED
        .map(|(_, c, _)| c)
        .into_iter()
        .chain(UNNAMED)
        .collect();
    let mut c = CTypes::zeroed(values.len());
    for (i, &v) in values.iter().enumerate() {
        c.set(i, v);
    }
    // The path a field accessor over a C array of `enum AVMediaType` takes:
    // adopt the C block and read it as `MediaType`s.
    let len = c.1;
    let ptr = c.0;
    let _ = core::mem::ManuallyDrop::new(c);
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
#[cfg_attr(miri, ignore = "calls C")]
fn clone_copies_the_c_encoding() {
    let src: Vec<MediaType> = NAMED
        .map(|(t, _, _)| t)
        .into_iter()
        .chain(UNNAMED.map(Into::into))
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
    (PictureType::NONE, ffi::AV_PICTURE_TYPE_NONE, b'?'),
    (PictureType::I, ffi::AV_PICTURE_TYPE_I, b'I'),
    (PictureType::P, ffi::AV_PICTURE_TYPE_P, b'P'),
    (PictureType::B, ffi::AV_PICTURE_TYPE_B, b'B'),
    (PictureType::S, ffi::AV_PICTURE_TYPE_S, b'S'),
    (PictureType::SI, ffi::AV_PICTURE_TYPE_SI, b'i'),
    (PictureType::SP, ffi::AV_PICTURE_TYPE_SP, b'p'),
    (PictureType::BI, ffi::AV_PICTURE_TYPE_BI, b'b'),
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
        .chain(PICTURE_UNNAMED)
        .collect()
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn picture_type_constants_are_the_c_enumerators() {
    for (t, c, letter) in PICTURE_NAMED {
        assert_eq!(t.into_raw(), c);
        assert_eq!(PictureType::from_raw(c), t);
        assert_eq!(c_letter(t.into_raw()), c_letter(c));
        assert_eq!(c_letter(t.into_raw()), letter);
    }
    for raw in PICTURE_UNNAMED {
        let t = PictureType::from_raw(raw);
        assert!(PICTURE_NAMED.iter().all(|&(n, _, _)| n != t));
        assert_eq!(c_letter(t.into_raw()), b'?', "raw {raw}");
        assert_eq!(t.into_raw(), raw);
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn picture_type_zeroed_storage_reads_as_c_zeroed_storage() {
    const LEN: usize = 8;
    let rust = alloc_array_zeroed::<PictureType>(LEN).unwrap();
    // SAFETY: `av_calloc` takes its sizes by value.
    let c = unsafe { ffi::av_calloc(LEN, size_of::<ffi::AVPictureType>()) }
        .cast::<ffi::AVPictureType>();
    assert!(!c.is_null());
    for (i, t) in rust.as_slice().iter().enumerate() {
        // SAFETY: as the comment below states, for this part of the operation.
        let c_add_3 = unsafe { c.add(i) };
        // SAFETY: `i` is in bounds of the live, zero-filled C block.
        let ci = unsafe { c_add_3.read() };
        assert_eq!(t.into_raw(), ci);
        assert_eq!(c_letter(t.into_raw()), c_letter(ci));
    }
    // A zero-filled C struct's picture type is `AV_PICTURE_TYPE_NONE`.
    assert!(rust.as_slice().iter().all(|&t| t == PictureType::NONE));
    // SAFETY: `c` is the live `av_calloc` block allocated above.
    unsafe { ffi::av_free(c.cast::<c_void>()) };
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
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
        // For the unsafe operations below: `step` and `back` are in bounds of the live C block,
        // owned here.
        {
            // SAFETY: as the comment below states, for this part of the operation.
            let c_add_4 = unsafe { c.add(step) };
            // SAFETY: as the comment above this block states.
            unsafe { c_add_4.write(v) };
            // SAFETY: as the comment below states, for this part of the operation.
            let c_add_3 = unsafe { c.add(back) };
            // SAFETY: as the comment above this block states.
            unsafe { c_add_3.write(w) };
        }
        for i in 0..len {
            // SAFETY: as the comment below states, for this part of the operation.
            let c_add_2 = unsafe { c.add(i) };
            // SAFETY: `i` is in bounds of the live, initialised C block.
            let ci = unsafe { c_add_2.read() };
            let ri = rust.as_slice()[i];
            assert_eq!(ri.into_raw(), ci, "step {step} slot {i}");
            assert_eq!(c_letter(ri.into_raw()), c_letter(ci));
        }
    }
    // A clone is `av_memdup`'s copy of the same encodings.
    let copy = rust.clone();
    for i in 0..len {
        // SAFETY: as the comment below states, for this part of the operation.
        let c_add = unsafe { c.add(i) };
        // SAFETY: as above.
        let ci = unsafe { c_add.read() };
        assert_eq!(copy.as_slice()[i].into_raw(), ci);
    }
    // SAFETY: `c` is the live `av_calloc` block allocated above.
    unsafe { ffi::av_free(c.cast::<c_void>()) };
}

/// `av_fourcc_make_string` against the safe `fourcc_string`.
mod fourcc {
    use core::ffi::CStr;

    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::utils::fourcc_string;

    /// `av_fourcc_make_string` into `buf`: the string, which C returns as `buf`.
    fn c_fourcc(buf: &mut [u8; 32], fourcc: u32) -> Vec<u8> {
        // SAFETY: `buf` is `AV_FOURCC_MAX_STRING_SIZE` writable bytes, which is
        // all `av_fourcc_make_string` writes.
        let ret = unsafe { ffi::av_fourcc_make_string(buf.as_mut_ptr().cast(), fourcc) };
        assert_eq!(ret.cast::<u8>(), buf.as_mut_ptr());
        CStr::from_bytes_until_nul(buf).unwrap().to_bytes().to_vec()
    }

    /// `FourCCs` whose bytes cover every class `av_fourcc_make_string` prints
    /// differently, in each position.
    fn fourccs() -> impl Iterator<Item = u32> {
        let singles = (0..=255u32)
            .flat_map(|b| (0..4).map(move |i| b << (8 * i) | 0x4141_4141 & !(0xFF << (8 * i))));
        let words = [
            0,
            u32::MAX,
            u32::from_le_bytes(*b"H264"),
            u32::from_le_bytes(*b"av01"),
            0x2E2D_205F,
            0x0102_0304,
        ];
        singles.chain(words)
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn fourcc_strings_and_buffers_match_c() {
        for f in fourccs() {
            let (mut rb, mut cb) = ([0u8; 32], [0u8; 32]);
            let r = fourcc_string(&mut rb, f).to_bytes().to_vec();
            assert_eq!(r, c_fourcc(&mut cb, f), "{f:#010x}");
            assert_eq!(rb, cb, "{f:#010x}: whole buffer");
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn reused_buffers_agree_after_every_step() {
        let (mut rb, mut cb) = ([0xA5u8; 32], [0xA5u8; 32]);
        for f in [
            0xFFFF_FFFF,
            u32::from_le_bytes(*b"mp4a"),
            0,
            0x0000_2E41,
            0x7F7F_7F7F,
        ] {
            let r = fourcc_string(&mut rb, f).to_bytes().to_vec();
            assert_eq!(r, c_fourcc(&mut cb, f), "{f:#010x}");
            // Bytes past the NUL keep what earlier, longer strings left, alike.
            assert_eq!(rb, cb, "{f:#010x}: whole buffer");
        }
    }
}

/// The names, letters and time base C gives (core wave 1, batch 2): each safe
/// accessor against the C function, over named and unnamed values.
mod names {
    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::utils::{MediaType, PictureType, time_base};

    use super::{NAMED, PICTURE_NAMED, UNNAMED, c_letter, c_name, picture_values};

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn as_str_is_av_get_media_type_string() {
        let values = NAMED.iter().map(|&(_, raw, _)| raw).chain(UNNAMED);
        for raw in values {
            assert_eq!(MediaType::from_raw(raw).as_str(), c_name(raw), "{raw:?}");
        }
        for (t, _, name) in NAMED {
            assert_eq!(t.as_str(), name);
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn to_char_is_av_get_picture_type_char() {
        for raw in picture_values() {
            assert_eq!(
                PictureType::from_raw(raw).to_char(),
                char::from(c_letter(raw)),
                "{raw:?}"
            );
        }
        for (t, raw, letter) in PICTURE_NAMED {
            assert_eq!(t.to_char(), char::from(letter));
            assert_eq!(t.to_char(), char::from(c_letter(raw)));
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn time_base_is_av_get_time_base_q() {
        // SAFETY: `av_get_time_base_q` takes no arguments and returns a
        // struct by value.
        let c = unsafe { ffi::av_get_time_base_q() };
        let tb = time_base();
        assert_eq!((tb.as_ref().num(), tb.as_ref().den()), (c.num, c.den));
        assert_eq!(c.den, ffi::AV_TIME_BASE as i32);
    }
}
