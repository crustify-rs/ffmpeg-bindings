//! Equivalence of `libavutil/samplefmt.h`'s [`SampleFormat`] with the C enum,
//! as C observes it: through `av_get_sample_fmt_name`, `av_get_sample_fmt`,
//! `av_get_bytes_per_sample` and `av_sample_fmt_is_planar`, and through
//! C-allocated storage holding the values.

use core::ffi::{CStr, c_int, c_void};
use core::mem::size_of;

use ffmpeg::libavutil::ffi;
use ffmpeg::libavutil::mem::{HeapVec, alloc_array_zeroed};
use ffmpeg::libavutil::samplefmt::SampleFormat;

/// Every named constant beside its C enumerator and C name.
const NAMED: [(SampleFormat, ffi::AVSampleFormat, Option<&CStr>); 13] = [
    (
        SampleFormat::NONE,
        ffi::AVSampleFormat::AV_SAMPLE_FMT_NONE,
        None,
    ),
    (
        SampleFormat::U8,
        ffi::AVSampleFormat::AV_SAMPLE_FMT_U8,
        Some(c"u8"),
    ),
    (
        SampleFormat::S16,
        ffi::AVSampleFormat::AV_SAMPLE_FMT_S16,
        Some(c"s16"),
    ),
    (
        SampleFormat::S32,
        ffi::AVSampleFormat::AV_SAMPLE_FMT_S32,
        Some(c"s32"),
    ),
    (
        SampleFormat::FLT,
        ffi::AVSampleFormat::AV_SAMPLE_FMT_FLT,
        Some(c"flt"),
    ),
    (
        SampleFormat::DBL,
        ffi::AVSampleFormat::AV_SAMPLE_FMT_DBL,
        Some(c"dbl"),
    ),
    (
        SampleFormat::U8P,
        ffi::AVSampleFormat::AV_SAMPLE_FMT_U8P,
        Some(c"u8p"),
    ),
    (
        SampleFormat::S16P,
        ffi::AVSampleFormat::AV_SAMPLE_FMT_S16P,
        Some(c"s16p"),
    ),
    (
        SampleFormat::S32P,
        ffi::AVSampleFormat::AV_SAMPLE_FMT_S32P,
        Some(c"s32p"),
    ),
    (
        SampleFormat::FLTP,
        ffi::AVSampleFormat::AV_SAMPLE_FMT_FLTP,
        Some(c"fltp"),
    ),
    (
        SampleFormat::DBLP,
        ffi::AVSampleFormat::AV_SAMPLE_FMT_DBLP,
        Some(c"dblp"),
    ),
    (
        SampleFormat::S64,
        ffi::AVSampleFormat::AV_SAMPLE_FMT_S64,
        Some(c"s64"),
    ),
    (
        SampleFormat::S64P,
        ffi::AVSampleFormat::AV_SAMPLE_FMT_S64P,
        Some(c"s64p"),
    ),
];

/// Values C may store though it names none of them, `AV_SAMPLE_FMT_NB` included.
const UNNAMED: [c_int; 7] = [c_int::MIN, -1000, -2, 12, 13, 1 << 20, c_int::MAX];

/// What C calls `f`: `av_get_sample_fmt_name`, NULL as `None`.
fn c_name(f: ffi::AVSampleFormat) -> Option<&'static CStr> {
    // SAFETY: `av_get_sample_fmt_name` takes the enum by value and range-checks
    // it before indexing its table.
    let name = unsafe { ffi::av_get_sample_fmt_name(f) };
    if name.is_null() {
        return None;
    }
    // SAFETY: a non-NULL result is one of the static string literals in
    // `samplefmt.c`'s `sample_fmt_info`.
    Some(unsafe { CStr::from_ptr(name) })
}

/// The format C parses `name` as: `av_get_sample_fmt`.
fn c_parse(name: &CStr) -> ffi::AVSampleFormat {
    // SAFETY: `name` is NUL-terminated and outlives the call, which only
    // compares it against the table's names.
    unsafe { ffi::av_get_sample_fmt(name.as_ptr()) }
}

/// C's `av_get_bytes_per_sample(f)` and `av_sample_fmt_is_planar(f)`.
fn c_properties(f: ffi::AVSampleFormat) -> (c_int, c_int) {
    // SAFETY: both take the enum by value and range-check it, returning 0 for
    // a value outside the table.
    unsafe {
        (
            ffi::av_get_bytes_per_sample(f),
            ffi::av_sample_fmt_is_planar(f),
        )
    }
}

/// A C block of `ffi::AVSampleFormat`, freed with `av_free` on drop.
struct CFormats(*mut ffi::AVSampleFormat, usize);

impl CFormats {
    fn zeroed(len: usize) -> Self {
        // SAFETY: `av_calloc` takes its sizes by value.
        let ptr = unsafe { ffi::av_calloc(len, size_of::<ffi::AVSampleFormat>()) };
        assert!(!ptr.is_null());
        Self(ptr.cast(), len)
    }

    fn get(&self, i: usize) -> ffi::AVSampleFormat {
        assert!(i < self.1);
        // SAFETY: `i` is in bounds of the live, initialised block.
        unsafe { self.0.add(i).read() }
    }

    fn set(&mut self, i: usize, f: ffi::AVSampleFormat) {
        assert!(i < self.1);
        // SAFETY: `i` is in bounds of the live block, which we own.
        unsafe { self.0.add(i).write(f) }
    }
}

impl Drop for CFormats {
    fn drop(&mut self) {
        // SAFETY: `self.0` is a live `av_calloc` block we own.
        unsafe { ffi::av_free(self.0.cast::<c_void>()) }
    }
}

/// The C values a Rust array holds, read back through its C-typed pointer.
fn rust_as_c(a: &HeapVec<SampleFormat>) -> Vec<ffi::AVSampleFormat> {
    let ptr = a.as_ptr().cast::<ffi::AVSampleFormat>();
    (0..a.len())
        // SAFETY: `SampleFormat` is `#[repr(transparent)]` over
        // `ffi::AVSampleFormat`, and `i` is in bounds of the live, initialised
        // array `a` borrows.
        .map(|i| unsafe { ptr.add(i).read() })
        .collect()
}

fn c_values(c: &CFormats) -> Vec<ffi::AVSampleFormat> {
    (0..c.1).map(|i| c.get(i)).collect()
}

fn all_values() -> Vec<ffi::AVSampleFormat> {
    NAMED
        .iter()
        .map(|&(_, c, _)| c)
        .chain(UNNAMED.map(ffi::AVSampleFormat))
        .collect()
}

#[test]
fn constants_are_the_c_enumerators() {
    for (f, c, name) in NAMED {
        assert_eq!(f.into_raw(), c);
        assert_eq!(SampleFormat::from_raw(c), f);
        assert_eq!(c_name(f.into_raw()), c_name(c));
        assert_eq!(c_name(f.into_raw()), name);
        assert_eq!(c_properties(f.into_raw()), c_properties(c));
    }
    assert_eq!(
        SampleFormat::COUNT,
        ffi::AVSampleFormat::AV_SAMPLE_FMT_NB.0 as usize
    );
}

#[test]
fn c_parses_each_name_back_to_its_constant() {
    for (f, _, name) in NAMED {
        let Some(name) = name else { continue };
        assert_eq!(SampleFormat::from_raw(c_parse(name)), f, "{name:?}");
    }
    // An unknown name parses as `NONE`.
    assert_eq!(SampleFormat::from_raw(c_parse(c"s24")), SampleFormat::NONE);
    assert_eq!(SampleFormat::from_raw(c_parse(c"")), SampleFormat::NONE);
}

#[test]
fn index_is_some_exactly_where_c_knows_the_format() {
    let all = all_values().into_iter().map(|c| c.0);
    for raw in all.chain(-3..=16) {
        let f = SampleFormat::from_raw(ffi::AVSampleFormat(raw));
        let known = f.index().is_some();
        assert_eq!(known, c_name(f.into_raw()).is_some(), "raw {raw}");
        // C's per-format table answers 0 bytes for every value it does not hold.
        let (bytes, _) = c_properties(f.into_raw());
        assert_eq!(known, bytes > 0, "raw {raw}");
        assert_eq!(f.into_raw().0, raw);
    }
}

#[test]
fn planar_constants_are_the_ones_c_reports_planar() {
    let planar = [
        SampleFormat::U8P,
        SampleFormat::S16P,
        SampleFormat::S32P,
        SampleFormat::FLTP,
        SampleFormat::DBLP,
        SampleFormat::S64P,
    ];
    for (f, c, _) in NAMED {
        let (_, is_planar) = c_properties(c);
        assert_eq!(planar.contains(&f), is_planar != 0, "{f:?}");
    }
}

#[test]
fn zeroed_storage_reads_as_c_zeroed_storage() {
    const LEN: usize = 8;
    let rust = alloc_array_zeroed::<SampleFormat>(LEN).unwrap();
    let c = CFormats::zeroed(LEN);
    assert_eq!(rust_as_c(&rust), c_values(&c));
    // A zero-filled C struct's sample format is `AV_SAMPLE_FMT_U8`, not `NONE`.
    assert!(rust.as_slice().iter().all(|&f| f == SampleFormat::U8));
    assert_eq!(c_name(c.get(0)), Some(c"u8"));
}

#[test]
fn writes_through_each_side_agree_after_every_step() {
    let values = all_values();
    let len = values.len();
    let mut rust = alloc_array_zeroed::<SampleFormat>(len).unwrap();
    let mut c = CFormats::zeroed(len);
    for (step, &v) in values.iter().enumerate() {
        // Each step writes slot `step` and rotates an earlier one.
        rust.as_mut_slice()[step] = SampleFormat::from_raw(v);
        c.set(step, v);
        let back = (step * 7) % len;
        let w = values[(step + 3) % len];
        rust.as_mut_slice()[back] = w.into();
        c.set(back, w);
        assert_eq!(rust_as_c(&rust), c_values(&c), "step {step}");
        for i in 0..len {
            let r = rust.as_slice()[i].into_raw();
            assert_eq!(r, c.get(i), "step {step} slot {i}");
            assert_eq!(c_name(r), c_name(c.get(i)));
            assert_eq!(c_properties(r), c_properties(c.get(i)));
        }
    }
}

#[test]
fn values_c_writes_are_read_back_unchanged() {
    let values = all_values();
    let mut c = CFormats::zeroed(values.len());
    for (i, &v) in values.iter().enumerate() {
        c.set(i, v);
    }
    // The path a field accessor over a C array of `enum AVSampleFormat` takes:
    // adopt the C block and read it as `SampleFormat`s.
    let len = c.1;
    let ptr = c.0;
    core::mem::forget(c);
    // SAFETY: `ptr` is a live `av_calloc` block of `len` initialised
    // `ffi::AVSampleFormat`, whose ownership `forget` gave up; `SampleFormat`
    // is layout-compatible with it, and `av_free` releases it.
    let adopted = unsafe { HeapVec::<SampleFormat>::from_raw_parts(ptr.cast(), len) }.unwrap();
    for (i, &v) in values.iter().enumerate() {
        assert_eq!(adopted.as_slice()[i], SampleFormat::from_raw(v));
        assert_eq!(adopted.as_slice()[i].index().is_some(), c_name(v).is_some());
    }
}

#[test]
fn none_terminated_lists_end_where_c_ends_them() {
    // A C-built list in the shape of `AVCodec.sample_fmts`, read through the
    // Rust type up to its terminator, against C's own walk of the same block.
    let list = [
        ffi::AVSampleFormat::AV_SAMPLE_FMT_FLTP,
        ffi::AVSampleFormat::AV_SAMPLE_FMT_S16,
        ffi::AVSampleFormat(42),
        ffi::AVSampleFormat::AV_SAMPLE_FMT_NONE,
        ffi::AVSampleFormat::AV_SAMPLE_FMT_U8,
    ];
    let mut c = CFormats::zeroed(list.len());
    for (i, &v) in list.iter().enumerate() {
        c.set(i, v);
    }
    let c_walk: Vec<_> = (0..c.1)
        .map(|i| c.get(i))
        .take_while(|&v| v != ffi::AVSampleFormat::AV_SAMPLE_FMT_NONE)
        .collect();
    let rust = HeapVec::from_slice(&list.map(SampleFormat::from_raw)).unwrap();
    let rust_walk: Vec<_> = rust
        .as_slice()
        .iter()
        .take_while(|&&f| f != SampleFormat::NONE)
        .map(|f| f.into_raw())
        .collect();
    assert_eq!(rust_walk, c_walk);
}

#[test]
fn clone_copies_the_c_encoding() {
    let src: Vec<SampleFormat> = all_values().into_iter().map(SampleFormat::from).collect();
    let rust = HeapVec::from_slice(&src).unwrap();
    let copy = rust.clone();
    let mut c = CFormats::zeroed(src.len());
    for (i, f) in src.iter().enumerate() {
        c.set(i, f.into_raw());
    }
    assert_eq!(rust_as_c(&copy), c_values(&c));
    assert_eq!(rust_as_c(&rust), rust_as_c(&copy));
}
