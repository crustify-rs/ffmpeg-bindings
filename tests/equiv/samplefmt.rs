//! Equivalence of `libavutil/samplefmt.h`'s [`SampleFormat`] with the C enum,
//! as C observes it: through `av_get_sample_fmt_name`, `av_get_sample_fmt`,
//! `av_get_bytes_per_sample` and `av_sample_fmt_is_planar`, and through
//! C-allocated storage holding the values.
#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "test inputs are chosen within range, and the casts mirror the C conversions under comparison"
)]
#![expect(
    clippy::many_single_char_names,
    reason = "names follow the C fields, formulas and fixtures they mirror"
)]

use core::ffi::{CStr, c_int, c_void};
use core::mem::size_of;

use ffmpeg_bindings::libavutil::libavutil::ffi;
use ffmpeg_bindings::libavutil::libavutil::mem::{HeapVec, alloc_array_zeroed};
use ffmpeg_bindings::libavutil::libavutil::samplefmt::SampleFormat;

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
    (unsafe { ffi::av_get_bytes_per_sample(f) }, unsafe {
        ffi::av_sample_fmt_is_planar(f)
    })
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
        // SAFETY: as the comment below states, for this part of the operation.
        let v_0_add_2 = unsafe { self.0.add(i) };
        // SAFETY: `i` is in bounds of the live, initialised block.
        unsafe { v_0_add_2.read() }
    }

    fn set(&mut self, i: usize, f: ffi::AVSampleFormat) {
        assert!(i < self.1);
        // SAFETY: as the comment below states, for this part of the operation.
        let v_0_add = unsafe { self.0.add(i) };
        // SAFETY: `i` is in bounds of the live block, which we own.
        unsafe { v_0_add.write(f) }
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
        .map(|i| {
            // SAFETY: `i` is in bounds of the array `ptr` points into.
            let at = unsafe { ptr.add(i) };
            // SAFETY: `SampleFormat` is `#[repr(transparent)]` over
            // `ffi::AVSampleFormat`, and `i` is in bounds of the live, initialised
            // array `a` borrows.
            unsafe { at.read() }
        })
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
#[cfg_attr(miri, ignore = "calls C")]
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
#[cfg_attr(miri, ignore = "calls C")]
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
#[cfg_attr(miri, ignore = "calls C")]
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
#[cfg_attr(miri, ignore = "calls C")]
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
#[cfg_attr(miri, ignore = "calls C")]
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
#[cfg_attr(miri, ignore = "calls C")]
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
#[cfg_attr(miri, ignore = "calls C")]
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
    let _ = core::mem::ManuallyDrop::new(c);
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
#[cfg_attr(miri, ignore = "calls C")]
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
#[cfg_attr(miri, ignore = "calls C")]
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

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn default_is_the_format_c_gives_a_fresh_frame() {
    // `av_frame_alloc` writes `-1` into the `format` an audio frame reads
    // as its sample format: `AV_SAMPLE_FMT_NONE`.
    // SAFETY: no arguments; NULL on failure, checked below.
    let mut f = unsafe { ffi::av_frame_alloc() };
    assert!(!f.is_null());
    // For the unsafe operations below: `f` is a live frame `av_frame_alloc` initialised; `format`
    // is read in place, and `av_frame_free` then releases the frame and NULLs
    // the slot.
    let format = {
        // SAFETY: as the comment above this block states.
        let format = unsafe { (*f).format };
        // SAFETY: as the comment above this block states.
        unsafe { ffi::av_frame_free(&raw mut f) };
        format
    };
    assert_eq!(SampleFormat::default().into_raw().0, format);
    assert_eq!(
        SampleFormat::from_raw(ffi::AVSampleFormat(format)),
        SampleFormat::default()
    );
    assert_eq!(c_name(SampleFormat::default().into_raw()), None);
    // ... and what C parses an unknown name as.
    assert_eq!(
        SampleFormat::from_raw(c_parse(c"no-such")),
        SampleFormat::default()
    );
}

/// The sample buffer API against `av_samples_*`: layouts and errors for
/// every format and a grid of counts and alignments, buffers C allocates and
/// lays out for both sides, and sequences of writes, silences and copies run
/// on a Rust buffer and its C twin, compared after every step.
mod buffers {
    use core::ffi::{c_int, c_void};
    use core::ptr;

    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::samplefmt::{
        BufferSize, SampleBuffer, SampleBufferWithPointers, SampleFormat, SamplesError, SamplesMut,
        SamplesRef, samples_buffer_size,
    };

    use super::all_values;

    const FORMATS: [SampleFormat; 12] = [
        SampleFormat::U8,
        SampleFormat::S16,
        SampleFormat::S32,
        SampleFormat::FLT,
        SampleFormat::DBL,
        SampleFormat::U8P,
        SampleFormat::S16P,
        SampleFormat::S32P,
        SampleFormat::FLTP,
        SampleFormat::DBLP,
        SampleFormat::S64,
        SampleFormat::S64P,
    ];

    /// Counts and alignments small enough to allocate, with C's default
    /// alignment (0) and a non-power-of-two one (3).
    const CHANNELS: [usize; 4] = [1, 2, 3, 9];
    const SAMPLES: [usize; 4] = [1, 5, 32, 33];
    const ALIGNS: [u32; 6] = [0, 1, 2, 3, 16, 64];

    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }

        fn below(&mut self, n: usize) -> usize {
            (self.next() % n as u64) as usize
        }
    }

    /// `AVERROR(EINVAL)`.
    const EINVAL: c_int = -(ffi::EINVAL as c_int);

    fn is_planar(f: SampleFormat) -> bool {
        // SAFETY: C bounds-checks the value.
        unsafe { ffi::av_sample_fmt_is_planar(f.into_raw()) != 0 }
    }

    fn bytes_per_sample(f: SampleFormat) -> usize {
        // SAFETY: C bounds-checks the value.
        unsafe { ffi::av_get_bytes_per_sample(f.into_raw()) as usize }
    }

    /// `av_samples_get_buffer_size`, as its return and linesize.
    fn c_buffer_size(
        channels: c_int,
        samples: c_int,
        f: SampleFormat,
        align: c_int,
    ) -> (c_int, c_int) {
        let mut linesize = -1;
        // SAFETY: C writes only the live local `linesize`.
        let ret = unsafe {
            ffi::av_samples_get_buffer_size(
                &raw mut linesize,
                channels,
                samples,
                f.into_raw(),
                align,
            )
        };
        (ret, linesize)
    }

    /// Planes as C's routines take them: plane pointers C computed, and the
    /// counts C was given.
    struct CPlanes {
        slots: Vec<*mut u8>,
        linesize: usize,
        channels: c_int,
        format: SampleFormat,
        samples: usize,
    }

    impl CPlanes {
        fn plane_len(&self) -> usize {
            let per_plane = if is_planar(self.format) {
                1
            } else {
                self.channels as usize
            };
            self.samples * bytes_per_sample(self.format) * per_plane
        }

        fn plane(&self, i: usize) -> &[u8] {
            // SAFETY: C placed plane `i` inside a live buffer of the layout
            // it computed, and every plane is initialised.
            unsafe { core::slice::from_raw_parts(self.slots[i], self.plane_len()) }
        }

        fn plane_mut(&mut self, i: usize) -> &mut [u8] {
            // SAFETY: as in `plane`, borrowed exclusively through `self`.
            unsafe { core::slice::from_raw_parts_mut(self.slots[i], self.plane_len()) }
        }

        fn set_silence(&mut self, offset: usize, count: usize) {
            // SAFETY: the caller keeps the range inside the planes.
            let ret = unsafe {
                ffi::av_samples_set_silence(
                    self.slots.as_ptr(),
                    offset as c_int,
                    count as c_int,
                    self.channels,
                    self.format.into_raw(),
                )
            };
            assert_eq!(ret, 0);
        }

        fn copy(
            &mut self,
            src: Option<&CPlanes>,
            src_offset: usize,
            dst_offset: usize,
            count: usize,
        ) {
            let src_slots = src.map_or(self.slots.as_ptr(), |s| s.slots.as_ptr());
            // SAFETY: the caller keeps both ranges inside the planes.
            let ret = unsafe {
                ffi::av_samples_copy(
                    self.slots.as_ptr(),
                    src_slots,
                    dst_offset as c_int,
                    src_offset as c_int,
                    count as c_int,
                    self.channels,
                    self.format.into_raw(),
                )
            };
            assert_eq!(ret, 0);
        }
    }

    /// A buffer `av_samples_alloc` allocated, freed with `av_freep` on drop.
    struct CAlloc(CPlanes);

    impl Drop for CAlloc {
        fn drop(&mut self) {
            // SAFETY: slot 0 owns the `av_malloc` block; `av_freep` frees
            // it once and NULLs the slot.
            unsafe { ffi::av_freep((&raw mut self.0.slots[0]).cast::<c_void>()) };
        }
    }

    fn c_alloc(
        channels: usize,
        samples: usize,
        f: SampleFormat,
        align: u32,
    ) -> Result<CAlloc, c_int> {
        let planes = if is_planar(f) { channels } else { 1 };
        let mut slots = vec![ptr::null_mut(); planes.max(1)];
        let mut linesize = -1;
        // SAFETY: `slots` holds the pointers C writes on success.
        let ret = unsafe {
            ffi::av_samples_alloc(
                slots.as_mut_ptr(),
                &raw mut linesize,
                channels as c_int,
                samples as c_int,
                f.into_raw(),
                align as c_int,
            )
        };
        if ret < 0 {
            assert!(slots.iter().all(|p| p.is_null()));
            return Err(ret);
        }
        Ok(CAlloc(CPlanes {
            slots,
            linesize: linesize as usize,
            channels: channels as c_int,
            format: f,
            samples,
        }))
    }

    /// Both blocks `av_samples_alloc_array_and_samples` allocated, freed
    /// with `av_freep` on drop.
    struct CArray(*mut *mut u8, CPlanes);

    impl Drop for CArray {
        fn drop(&mut self) {
            // For the unsafe operations below: slot 0 of the array owns the sample block, and the
            // array is itself an `av_calloc` block; each is freed once.
            {
                // SAFETY: as the comment above this block states.
                unsafe { ffi::av_freep(self.0.cast::<c_void>()) };
                // SAFETY: as the comment above this block states.
                unsafe { ffi::av_freep((&raw mut self.0).cast::<c_void>()) };
            }
        }
    }

    fn c_alloc_array(
        channels: usize,
        samples: usize,
        f: SampleFormat,
        align: u32,
    ) -> Result<CArray, c_int> {
        let mut array = ptr::null_mut();
        let mut linesize = -1;
        // SAFETY: C writes the live locals only.
        let ret = unsafe {
            ffi::av_samples_alloc_array_and_samples(
                &raw mut array,
                &raw mut linesize,
                channels as c_int,
                samples as c_int,
                f.into_raw(),
                align as c_int,
            )
        };
        if ret < 0 {
            assert!(array.is_null());
            return Err(ret);
        }
        let planes = if is_planar(f) { channels } else { 1 };
        // SAFETY: the array holds `planes` pointers on success.
        let slots = unsafe { core::slice::from_raw_parts(array, planes) }.to_vec();
        Ok(CArray(
            array,
            CPlanes {
                slots,
                linesize: linesize as usize,
                channels: channels as c_int,
                format: f,
                samples,
            },
        ))
    }

    /// The Rust view and the C planes hold the same layout and samples.
    fn assert_same(rust: SamplesRef<'_>, c: &CPlanes, what: &str) {
        assert_eq!(rust.plane_count(), c.slots.len(), "{what}");
        assert_eq!(rust.linesize(), c.linesize, "{what}");
        let base = rust.plane(0).unwrap().as_ptr();
        for (i, plane) in rust.planes().enumerate() {
            assert_eq!(
                plane.as_ptr().addr() - base.addr(),
                c.slots[i].addr() - c.slots[0].addr(),
                "{what} plane {i}"
            );
            assert_eq!(plane, c.plane(i), "{what} plane {i}");
        }
    }

    /// `steps` random writes, silences and copies on both sides, each
    /// compared; `src` is a second pair of equal buffers to copy from.
    fn run_ops(
        rng: &mut Rng,
        rust: &mut SamplesMut<'_>,
        c: &mut CPlanes,
        src: (SamplesRef<'_>, &CPlanes),
        steps: usize,
    ) {
        let n = c.samples;
        for step in 0..steps {
            // Offsets and counts up to one past the end, so some run out.
            let (a, b, k) = (rng.below(n + 2), rng.below(n + 2), rng.below(n + 2));
            let fits = |offset: usize, count: usize, len: usize| offset + count <= len;
            match rng.below(4) {
                0 => {
                    let p = rng.below(c.slots.len());
                    let byte = rng.next() as u8;
                    let len = c.plane_len();
                    let (lo, hi) = (rng.below(len), rng.below(len + 1));
                    let (lo, hi) = (lo.min(hi), lo.max(hi));
                    rust.plane_mut(p).unwrap()[lo..hi].fill(byte);
                    c.plane_mut(p)[lo..hi].fill(byte);
                }
                1 => {
                    let r = rust.set_silence(a, k);
                    if fits(a, k, n) {
                        assert_eq!(r, Ok(()));
                        c.set_silence(a, k);
                    } else {
                        assert_eq!(r, Err(SamplesError::OutOfRange));
                    }
                }
                2 => {
                    let r = rust.copy_within(a, b, k);
                    if fits(a, k, n) && fits(b, k, n) {
                        assert_eq!(r, Ok(()));
                        c.copy(None, a, b, k);
                    } else {
                        assert_eq!(r, Err(SamplesError::OutOfRange));
                    }
                }
                _ => {
                    let a = rng.below(src.1.samples + 2);
                    let r = rust.copy_from(src.0, a, b, k);
                    if fits(a, k, src.1.samples) && fits(b, k, n) {
                        assert_eq!(r, Ok(()));
                        c.copy(Some(src.1), a, b, k);
                    } else {
                        assert_eq!(r, Err(SamplesError::OutOfRange));
                    }
                }
            }
            assert_same(rust.as_ref(), c, &format!("step {step}"));
        }
    }

    /// A random source pair of `samples` samples for copies into buffers of
    /// `f` and `channels`.
    fn source(
        rng: &mut Rng,
        channels: usize,
        samples: usize,
        f: SampleFormat,
    ) -> (SampleBuffer, CAlloc) {
        let mut rust = SampleBuffer::new(channels, samples, f, 1).unwrap();
        let mut c = c_alloc(channels, samples, f, 1).unwrap();
        for p in 0..c.0.slots.len() {
            let bytes: Vec<u8> = (0..c.0.plane_len()).map(|_| rng.next() as u8).collect();
            rust.as_mut().plane_mut(p).unwrap().copy_from_slice(&bytes);
            c.0.plane_mut(p).copy_from_slice(&bytes);
        }
        (rust, c)
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn is_planar_matches_c_for_every_value() {
        for raw in all_values() {
            let f = SampleFormat::from_raw(raw);
            assert_eq!(f.is_planar(), is_planar(f), "{f:?}");
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn buffer_size_matches_c_across_the_int_range() {
        let max = c_int::MAX;
        let counts = [
            0,
            1,
            2,
            3,
            31,
            32,
            33,
            1 << 20,
            max / 2,
            max - 31,
            max - 30,
            max,
        ];
        let aligns = [0, 1, 2, 3, 4, 16, 32, 64, 1 << 20, max];
        for raw in all_values() {
            let f = SampleFormat::from_raw(raw);
            for &channels in &counts {
                for &samples in &counts {
                    for &align in &aligns {
                        // C overflows rounding the default alignment's
                        // `INT_MAX - 31` samples, a multiple of 32: its
                        // layout is the one for no alignment (1).
                        let c_align = if align == 0 && samples == max - 31 {
                            1
                        } else {
                            align
                        };
                        let (ret, linesize) = c_buffer_size(channels, samples, f, c_align);
                        let rust = samples_buffer_size(
                            channels as usize,
                            samples as usize,
                            f,
                            align as u32,
                        );
                        let c = if ret < 0 {
                            Err(ret)
                        } else {
                            Ok(BufferSize {
                                size: ret as usize,
                                linesize: linesize as usize,
                            })
                        };
                        assert_eq!(rust, c, "{channels} {samples} {f:?} {align}");
                    }
                }
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn counts_beyond_an_int_fail_as_c_fails_their_negative_twins() {
        for f in FORMATS {
            // C sees a `u32` alignment past `INT_MAX` as negative.
            for align in [c_int::MAX as u32 + 1, u32::MAX] {
                assert_eq!(c_buffer_size(2, 10, f, align as c_int).0, EINVAL);
                assert_eq!(samples_buffer_size(2, 10, f, align), Err(EINVAL));
                assert_eq!(SampleBuffer::new(2, 10, f, align).unwrap_err(), EINVAL);
                assert_eq!(c_alloc(2, 10, f, align).err(), Some(EINVAL));
            }
            for big in [c_int::MAX as usize + 1, usize::MAX] {
                assert_eq!(samples_buffer_size(big, 1, f, 1), Err(EINVAL));
                assert_eq!(samples_buffer_size(1, big, f, 1), Err(EINVAL));
                assert_eq!(c_buffer_size(-1, 1, f, 1).0, EINVAL);
                assert_eq!(c_buffer_size(1, -1, f, 1).0, EINVAL);
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn alloc_lays_out_and_silences_as_c() {
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
        for f in FORMATS {
            for channels in CHANNELS {
                for samples in SAMPLES {
                    for align in ALIGNS {
                        let what = format!("{channels} {samples} {f:?} {align}");
                        let mut rust = SampleBuffer::new(channels, samples, f, align).unwrap();
                        let mut c = c_alloc(channels, samples, f, align).unwrap();
                        assert_eq!(
                            rust.size(),
                            c_buffer_size(channels as c_int, samples as c_int, f, align as c_int).0
                                as usize
                        );
                        assert_same(rust.as_ref(), &c.0, &what);
                        let (src, c_src) = source(&mut rng, channels, samples + 3, f);
                        run_ops(
                            &mut rng,
                            &mut rust.as_mut(),
                            &mut c.0,
                            (src.as_ref(), &c_src.0),
                            12,
                        );
                    }
                }
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn alloc_array_lays_out_and_silences_as_c() {
        let mut rng = Rng(0x2545_F491_4F6C_DD1D);
        for f in FORMATS {
            for channels in CHANNELS {
                for samples in SAMPLES {
                    for align in ALIGNS {
                        let what = format!("{channels} {samples} {f:?} {align}");
                        let mut rust =
                            SampleBufferWithPointers::new(channels, samples, f, align).unwrap();
                        let mut c = c_alloc_array(channels, samples, f, align).unwrap();
                        assert_same(rust.as_ref(), &c.1, &what);
                        let (src, c_src) = source(&mut rng, channels, samples, f);
                        run_ops(
                            &mut rng,
                            &mut rust.as_mut(),
                            &mut c.1,
                            (src.as_ref(), &c_src.0),
                            6,
                        );
                        // The planes outlive the pointer array alone.
                        let rust = rust.into_buffer();
                        assert_same(rust.as_ref(), &c.1, &what);
                    }
                }
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn alloc_failures_return_c_s_errors() {
        for raw in all_values() {
            let f = SampleFormat::from_raw(raw);
            for (channels, samples, align) in
                [(0, 1, 1), (1, 0, 0), (2, 3, 1), (1, c_int::MAX as usize, 1)]
            {
                let c = c_alloc(channels, samples, f, align).err();
                assert_eq!(
                    SampleBuffer::new(channels, samples, f, align).err(),
                    c,
                    "{f:?} {channels} {samples}"
                );
                let c = c_alloc_array(channels, samples, f, align).err();
                assert_eq!(
                    SampleBufferWithPointers::new(channels, samples, f, align).err(),
                    c,
                    "{f:?} {channels} {samples}"
                );
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn fill_arrays_lays_planes_over_the_buffer_as_c() {
        let mut rng = Rng(0x1234_5678_9ABC_DEF1);
        for f in FORMATS {
            for channels in CHANNELS {
                for align in ALIGNS {
                    let samples = 1 + rng.below(40);
                    let what = format!("{channels} {samples} {f:?} {align}");
                    let size = samples_buffer_size(channels, samples, f, align)
                        .unwrap()
                        .size;
                    let bytes: Vec<u8> =
                        (0..size + rng.below(8)).map(|_| rng.next() as u8).collect();
                    let (mut rust_buf, mut c_buf) = (bytes.clone(), bytes);
                    let planes = if is_planar(f) { channels } else { 1 };
                    let mut slots = vec![ptr::null_mut(); planes];
                    let mut linesize = -1;
                    // SAFETY: `slots` holds the pointers C writes, and
                    // `c_buf` holds the `size` bytes they span.
                    let ret = unsafe {
                        ffi::av_samples_fill_arrays(
                            slots.as_mut_ptr(),
                            &raw mut linesize,
                            c_buf.as_mut_ptr(),
                            channels as c_int,
                            samples as c_int,
                            f.into_raw(),
                            align as c_int,
                        )
                    };
                    assert_eq!(ret as usize, size, "{what}");
                    assert_eq!(slots[0], c_buf.as_mut_ptr());
                    let mut c = CPlanes {
                        slots,
                        linesize: linesize as usize,
                        channels: channels as c_int,
                        format: f,
                        samples,
                    };
                    {
                        let shared =
                            SamplesRef::from_buffer(&rust_buf, channels, samples, f, align)
                                .unwrap();
                        assert_eq!(shared.plane(0).unwrap().as_ptr(), rust_buf.as_ptr());
                        assert_same(shared, &c, &what);
                    }
                    let (src, c_src) = source(&mut rng, channels, samples, f);
                    let mut rust =
                        SamplesMut::from_buffer(&mut rust_buf, channels, samples, f, align)
                            .unwrap();
                    assert_same(rust.as_ref(), &c, &what);
                    run_ops(&mut rng, &mut rust, &mut c, (src.as_ref(), &c_src.0), 8);
                    // Bytes outside the planes are the caller's, untouched
                    // on both sides.
                    assert_eq!(rust_buf, c_buf, "{what}");
                    // One byte short of what C lays out is refused.
                    assert_eq!(
                        SamplesRef::from_buffer(&rust_buf[..size - 1], channels, samples, f, align)
                            .unwrap_err(),
                        EINVAL
                    );
                }
            }
        }
        // C's own errors pass through.
        for (channels, samples, f) in [
            (0, 1, SampleFormat::U8),
            (1, 0, SampleFormat::U8),
            (1, 1, SampleFormat::NONE),
        ] {
            let mut slot = ptr::null_mut();
            // SAFETY: C writes at most one slot, and nothing on failure; a
            // NULL buffer is only offset.
            let ret = unsafe {
                ffi::av_samples_fill_arrays(
                    &raw mut slot,
                    ptr::null_mut(),
                    ptr::null(),
                    channels,
                    samples,
                    f.into_raw(),
                    1,
                )
            };
            assert_eq!(
                SamplesRef::from_buffer(&[0; 64], channels as usize, samples as usize, f, 1)
                    .unwrap_err(),
                ret
            );
        }
    }
}

/// `SampleFormat::alt`, `packed` and `bytes_per_sample` against
/// `av_get_alt_sample_fmt`, `av_get_packed_sample_fmt` and
/// `av_get_bytes_per_sample` for every value from below `NONE` to past
/// `AV_SAMPLE_FMT_NB` and the `int` extremes, and chains feeding each answer
/// into the next call.
mod sample_format_forms {
    use core::ffi::c_int;

    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::samplefmt::SampleFormat;

    fn raws() -> impl Iterator<Item = c_int> {
        (-3..16).chain([i32::MIN, i32::MIN + 1, i32::MAX])
    }

    fn fmt(raw: c_int) -> SampleFormat {
        SampleFormat::from_raw(ffi::AVSampleFormat(raw))
    }

    /// A Rust result as C's raw value; `None` is C's `NONE` (-1).
    fn raw(f: Option<SampleFormat>) -> c_int {
        if let Some(f) = f {
            assert_ne!(f, SampleFormat::NONE);
        }
        f.map_or(-1, |f| f.into_raw().0)
    }

    fn c_alt(raw: c_int, planar: c_int) -> c_int {
        // SAFETY: two integers by value; C bounds-checks the table index.
        unsafe { ffi::av_get_alt_sample_fmt(ffi::AVSampleFormat(raw), planar) }.0
    }

    fn c_packed(raw: c_int) -> c_int {
        // SAFETY: as above.
        unsafe { ffi::av_get_packed_sample_fmt(ffi::AVSampleFormat(raw)) }.0
    }

    fn c_bytes(raw: c_int) -> c_int {
        // SAFETY: as above.
        unsafe { ffi::av_get_bytes_per_sample(ffi::AVSampleFormat(raw)) }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn forms_and_sizes_match_c_for_every_value() {
        let mut known = 0;
        for r in raws() {
            let f = fmt(r);
            assert_eq!(raw(f.alt(false)), c_alt(r, 0), "{r} packed");
            assert_eq!(raw(f.alt(true)), c_alt(r, 1), "{r} planar");
            assert_eq!(raw(f.packed()), c_packed(r), "{r}");
            assert_eq!(f.bytes_per_sample() as c_int, c_bytes(r), "{r}");
            known += usize::from(f.bytes_per_sample() != 0);
        }
        assert_eq!(known, SampleFormat::COUNT);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn chained_conversions_match_c_step_by_step() {
        for r in raws() {
            let (mut rust, mut c) = (fmt(r), r);
            for step in 0..6 {
                let planar = step % 2 == 0;
                let next = rust.alt(planar);
                let c_next = c_alt(c, c_int::from(planar));
                assert_eq!(raw(next), c_next, "{r} step {step}");
                assert_eq!(raw(next.and_then(SampleFormat::packed)), c_packed(c_next));
                assert_eq!(
                    next.map_or(0, SampleFormat::bytes_per_sample) as c_int,
                    c_bytes(c_next)
                );
                rust = next.unwrap_or(SampleFormat::NONE);
                c = c_next;
            }
        }
    }
}

/// `SampleFormat`'s C name lookups, planar form and description line
/// against `av_get_sample_fmt_name`, `av_get_sample_fmt`,
/// `av_get_planar_sample_fmt` and `av_get_sample_fmt_string`, over every
/// named value, values C does not name, and buffers of every size up to the
/// longest line, compared byte for byte after each write.
mod lookups {
    use core::ffi::{CStr, c_int};

    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::samplefmt::SampleFormat;

    /// Every value worth asking about: the named ones and their neighbours.
    fn values() -> impl Iterator<Item = c_int> {
        (-3..16).chain([c_int::MIN, c_int::MIN + 1, c_int::MAX])
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn name_matches_c() {
        for raw in values() {
            // SAFETY: by value; C returns NULL or a static string.
            let c = unsafe { ffi::av_get_sample_fmt_name(ffi::AVSampleFormat(raw)) };
            // SAFETY: a non-NULL `c` is a static NUL-terminated string.
            let c = (!c.is_null()).then(|| unsafe { CStr::from_ptr(c) });
            assert_eq!(
                SampleFormat::from_raw(ffi::AVSampleFormat(raw)).name(),
                c,
                "{raw}"
            );
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn from_name_matches_c() {
        let names: [&CStr; 16] = [
            c"u8", c"s16", c"s32", c"flt", c"dbl", c"u8p", c"s16p", c"s32p", c"fltp", c"dblp",
            c"s64", c"s64p", c"", c"none", c"S16", c"s16 ",
        ];
        for name in names {
            // SAFETY: a live NUL-terminated string C only compares.
            let c = unsafe { ffi::av_get_sample_fmt(name.as_ptr()) };
            let c =
                (c != ffi::AVSampleFormat::AV_SAMPLE_FMT_NONE).then_some(SampleFormat::from_raw(c));
            assert_eq!(SampleFormat::from_name(name), c, "{name:?}");
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn planar_matches_c_twice() {
        for raw in values() {
            let mut rust = Some(SampleFormat::from_raw(ffi::AVSampleFormat(raw)));
            let mut c = ffi::AVSampleFormat(raw);
            for _ in 0..2 {
                // SAFETY: by value; C range-checks it.
                c = unsafe { ffi::av_get_planar_sample_fmt(c) };
                rust = rust.and_then(SampleFormat::planar);
                let expect = (c != ffi::AVSampleFormat::AV_SAMPLE_FMT_NONE)
                    .then_some(SampleFormat::from_raw(c));
                assert_eq!(rust, expect, "{raw}");
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn describe_writes_what_c_writes() {
        for raw in values() {
            let mut rust_buf = [b'#'; 24];
            let mut c_buf = [b'#'; 24];
            // Shrinking buffers, each written over the previous text.
            for size in (0..=c_buf.len()).rev() {
                let f = SampleFormat::from_raw(ffi::AVSampleFormat(raw));
                let rust = f.describe(&mut rust_buf[..size]).map(CStr::to_owned);
                // SAFETY: `size` writable bytes; C writes nothing for values
                // past its table and returns `c_buf`.
                let ret = unsafe {
                    ffi::av_get_sample_fmt_string(
                        c_buf.as_mut_ptr().cast(),
                        size as c_int,
                        ffi::AVSampleFormat(raw),
                    )
                };
                assert_eq!(ret.cast::<u8>(), c_buf.as_mut_ptr());
                assert_eq!(rust_buf, c_buf, "{raw} {size}");
                let wrote = size > 0 && raw < SampleFormat::COUNT as c_int;
                let c_text = wrote.then(|| CStr::from_bytes_until_nul(&c_buf).unwrap().to_owned());
                assert_eq!(rust, c_text, "{raw} {size}");
            }
        }
    }
}
