//! Equivalence of `libavutil/imgutils.h`'s `check_size`,
//! `check_size_with_context`, `copy_plane` and `copy_plane_from_uncacheable`
//! with `av_image_check_size`, `av_image_copy_plane` and
//! `av_image_copy_plane_uc_from`, on independently owned planes and over
//! multi-call sequences, comparing results, every byte of the destination
//! buffers after each step, and the lines C logs.
#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    trivial_numeric_casts,
    reason = "test inputs are chosen within range, and the casts mirror the C conversions under comparison"
)]
#![expect(
    clippy::many_single_char_names,
    clippy::similar_names,
    reason = "names follow the C fields, formulas and fixtures they mirror"
)]
#![expect(
    clippy::too_many_lines,
    reason = "each test is one scenario, its helpers and steps kept where they are used"
)]

use core::cell::RefCell;
use core::ffi::{c_int, c_void};
use core::ptr;

use ffmpeg_bindings::libavutil::libavutil::ffi;
use ffmpeg_bindings::libavutil::libavutil::imgutils::{
    check_size, check_size_with_context, copy_plane, copy_plane_from_uncacheable,
};
use ffmpeg_bindings::libavutil::libavutil::log::{self, Callback, Record};
use ffmpeg_bindings::libavutil::libavutil::opt::ObjectRef;

use crate::log::LogState;

/// The bytes a plane of `h` rows of `w` bytes, `linesize` apart, spans.
fn span(linesize: isize, w: usize, h: usize) -> usize {
    if h == 0 {
        0
    } else {
        (h - 1) * linesize.unsigned_abs() + w
    }
}

/// The offset of row 0 in a plane laid out as `copy_plane` documents.
fn origin(linesize: isize, h: usize) -> usize {
    if linesize < 0 {
        (h - 1) * linesize.unsigned_abs()
    } else {
        0
    }
}

/// A 64-byte aligned buffer, so the uncacheable copy's SSE4 path can run.
#[repr(C, align(64))]
#[derive(Clone, PartialEq, Debug)]
struct Buf([u8; 4096]);

fn noise(seed: u32) -> Buf {
    let mut x = seed | 1;
    Buf(core::array::from_fn(|_| {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        x as u8
    }))
}

/// One copy step: linesizes, width, height, and where each plane starts.
#[derive(Clone, Copy, Debug)]
struct Step {
    dl: isize,
    sl: isize,
    w: usize,
    h: usize,
    doff: usize,
    soff: usize,
}

const STEPS: [Step; 9] = [
    Step {
        dl: 64,
        sl: 64,
        w: 64,
        h: 8,
        doff: 0,
        soff: 0,
    },
    Step {
        dl: 128,
        sl: 64,
        w: 10,
        h: 5,
        doff: 64,
        soff: 128,
    },
    Step {
        dl: -96,
        sl: 80,
        w: 70,
        h: 6,
        doff: 3,
        soff: 1,
    },
    Step {
        dl: 40,
        sl: -40,
        w: 40,
        h: 9,
        doff: 1000,
        soff: 7,
    },
    Step {
        dl: 33,
        sl: 17,
        w: 17,
        h: 12,
        doff: 2000,
        soff: 500,
    },
    Step {
        dl: 192,
        sl: 256,
        w: 129,
        h: 3,
        doff: 2048,
        soff: 1024,
    },
    Step {
        dl: 7,
        sl: 7,
        w: 0,
        h: 4,
        doff: 9,
        soff: 9,
    },
    Step {
        dl: 64,
        sl: 64,
        w: 1,
        h: 0,
        doff: 0,
        soff: 0,
    },
    Step {
        dl: -1,
        sl: -2,
        w: 1,
        h: 40,
        doff: 3000,
        soff: 3000,
    },
];

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn copy_plane_matches_c_step_by_step() {
    let src = noise(1);
    let (mut rust, mut c) = (noise(2), noise(2));
    for (i, s) in STEPS.into_iter().enumerate() {
        let (dlen, slen) = (span(s.dl, s.w, s.h), span(s.sl, s.w, s.h));
        let r = copy_plane(
            &mut rust.0[s.doff..s.doff + dlen],
            s.dl as i32,
            &src.0[s.soff..s.soff + slen],
            s.sl as i32,
            s.w,
            s.h,
        );
        assert_eq!(r, Ok(()), "step {i}");
        // For the unsafe operations below: both planes lie inside their buffers (`span` from the
        // row-0 offsets), the linesizes cover the width, and the buffers
        // are distinct.
        {
            // SAFETY: as the comment below states, for this part of the operation.
            let c_3 = unsafe { c.0.as_mut_ptr().add(s.doff + origin(s.dl, s.h)) };
            // SAFETY: as the comment below states, for this part of the operation.
            let src_3 = unsafe { src.0.as_ptr().add(s.soff + origin(s.sl, s.h)) };
            // SAFETY: as the comment above this block states.
            unsafe {
                ffi::av_image_copy_plane(
                    c_3,
                    s.dl as c_int,
                    src_3,
                    s.sl as c_int,
                    s.w as c_int,
                    s.h as c_int,
                );
            };
        };
        assert_eq!(rust, c, "after step {i}: {s:?}");
    }
}

/// On planes C handles (its SSE4 path on 16-byte aligned rows covering the
/// rounded-up width, or its plain path), the results match byte for byte,
/// the SSE4 path's copies past the width included.
#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn uncacheable_copy_matches_c_step_by_step() {
    let src = noise(3);
    let (mut rust, mut c) = (noise(4), noise(4));
    for (i, s) in STEPS.into_iter().enumerate() {
        let wide = s.w.next_multiple_of(64);
        let (dlen, slen) = (
            span(s.dl, wide.max(s.w), s.h),
            span(s.sl, wide.max(s.w), s.h),
        );
        let (dlen, slen) = (dlen.min(4096 - s.doff), slen.min(4096 - s.soff));
        let r = copy_plane_from_uncacheable(
            &mut rust.0[s.doff..s.doff + dlen],
            s.dl,
            &src.0[s.soff..s.soff + slen],
            s.sl,
            s.w,
            s.h,
        );
        assert_eq!(r, Ok(()), "step {i}");
        let fast = wide <= s.dl as usize && wide <= s.sl as usize && s.dl > 0 && s.sl > 0;
        if s.w > 0 && s.h > 0 {
            assert!(
                !fast || (s.doff % 16 == 0 && s.soff % 16 == 0),
                "step {i} would fault in C"
            );
            // For the unsafe operations below: as in `copy_plane_matches_c_step_by_step`; on the
            // SSE4 path the rows are 16-byte aligned and cover the rounded-up
            // width inside the buffers.
            {
                // SAFETY: as the comment below states, for this part of the operation.
                let c_2 = unsafe { c.0.as_mut_ptr().add(s.doff + origin(s.dl, s.h)) };
                // SAFETY: as the comment below states, for this part of the operation.
                let src_2 = unsafe { src.0.as_ptr().add(s.soff + origin(s.sl, s.h)) };
                // SAFETY: as the comment above this block states.
                unsafe {
                    ffi::av_image_copy_plane_uc_from(
                        c_2,
                        s.dl,
                        src_2,
                        s.sl,
                        s.w as isize,
                        s.h as c_int,
                    );
                };
            };
        }
        assert_eq!(rust, c, "after step {i}: {s:?}");
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn check_size_matches_c() {
    let dims = [
        0u32,
        1,
        2,
        1080,
        16383,
        16384,
        32767,
        65535,
        1 << 20,
        i32::MAX as u32,
        i32::MAX as u32 + 1,
        u32::MAX,
    ];
    for w in dims {
        for h in dims {
            // SAFETY: a NULL log context and no offset.
            let c = unsafe { ffi::av_image_check_size(w, h, 0, ptr::null_mut()) };
            let c = if c < 0 { Err(c) } else { Ok(()) };
            assert_eq!(check_size(w, h), c, "{w}x{h}");
            let with_context = check_size_with_context(w, h, 0, None);
            assert_eq!(with_context, c, "{w}x{h}");
        }
    }
}

/// What the capturing callback saw of one message on this thread: its
/// level, its context's class name and the formatted line, with the
/// context's address (C's stack, which moves between calls) masked.
type Line = (c_int, Option<String>, String);

thread_local! {
    static LINES: RefCell<Option<Vec<Line>>> = const { RefCell::new(None) };
}

/// `line` with every context address (` @ 0x...`) masked.
fn mask_addresses(line: &str) -> String {
    let mut out = String::new();
    let mut rest = line;
    while let Some(at) = rest.find(" @ 0x") {
        out.push_str(&rest[..at]);
        out.push_str(" @ ADDR");
        rest = rest[at + 5..].trim_start_matches(|c: char| c.is_ascii_hexdigit());
    }
    out.push_str(rest);
    out
}

/// Captures this thread's messages while [`captured`] runs.
struct Capture;

impl Callback for Capture {
    fn log(record: Record<'_>) {
        LINES.with(|l| {
            if let Some(lines) = l.borrow_mut().as_mut() {
                let mut prefix = true;
                let line = record.to_line(&mut prefix).unwrap().into_string().unwrap();
                let line = mask_addresses(&line);
                let class = record
                    .class()
                    .and_then(|c| c.name())
                    .map(|n| n.to_string_lossy().into_owned());
                lines.push((record.level().raw(), class, line));
            }
        });
    }
}

/// The lines `f` logs on this thread. The caller holds the [`LogState`]
/// lock.
fn captured(f: impl FnOnce()) -> Vec<Line> {
    LINES.with(|l| *l.borrow_mut() = Some(Vec::new()));
    log::set_callback::<Capture>();
    f();
    log::set_default_callback();
    LINES.with(|l| l.borrow_mut().take().unwrap())
}

/// A logging context named by its class, as the parent of C's `IMGUTILS`
/// context.
#[repr(C)]
struct Ctx {
    class: *const ffi::AVClass,
}

/// Runs the test `name` alone in a child process, so that no other test
/// logs through the callback it installs; true in that child.
///
/// The process-wide callback is sound for other threads' messages (they are
/// dropped), but `BSan` reports a false use-after-free in `av_strtod` on a
/// `cpu::parse_caps` thread logging through it concurrently: the pointer
/// libc's `strtod` hands back carries stale provenance, while the byte it
/// reads is inside a live block (formats wave-0 review, batch 1).
fn isolated(name: &str) -> bool {
    const KEY: &str = "CRUSTIFY_EQUIV_IMGUTILS_ISOLATED";
    if std::env::var_os(KEY).is_some() {
        return true;
    }
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--test-threads=1"])
        .env(KEY, "1")
        .status()
        .unwrap();
    assert!(status.success(), "isolated run failed: {status}");
    false
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn check_size_logs_as_c() {
    if !isolated("imgutils::check_size_logs_as_c") {
        return;
    }
    let _state = LogState::lock();
    let class = ffi::AVClass {
        class_name: c"equiv-imgutils-parent".as_ptr(),
        item_name: Some(ffi::av_default_item_name),
        version: (ffi::LIBAVUTIL_VERSION_MAJOR << 16
            | ffi::LIBAVUTIL_VERSION_MINOR << 8
            | ffi::LIBAVUTIL_VERSION_MICRO) as c_int,
        ..Default::default()
    };
    let mut parent = Ctx {
        class: &raw const class,
    };
    let parent: *mut c_void = (&raw mut parent).cast();
    for (w, h, offset, ctx) in [
        (0u32, 7u32, 0, ptr::null_mut()),
        (u32::MAX, 1, 0, ptr::null_mut()),
        (65536, 65536, 4, parent),
        (1, 0, -8, parent),
        (64, 64, 0, parent),
    ] {
        let mut rust_ret = Ok(());
        // SAFETY: `ctx` is NULL or a live context led by a valid class with
        // no options, callbacks or offsets, unwritten during the borrow.
        let obj = unsafe { ObjectRef::from_ptr(ctx) };
        let rust = captured(|| rust_ret = check_size_with_context(w, h, offset, obj));
        let mut c_ret = 0;
        // SAFETY: as above.
        let c = captured(|| c_ret = unsafe { ffi::av_image_check_size(w, h, offset, ctx) });
        assert_eq!(rust_ret, if c_ret < 0 { Err(c_ret) } else { Ok(()) });
        assert_eq!(rust, c, "{w}x{h} {offset}");
        assert_eq!(rust.len(), usize::from(c_ret < 0));
        if offset == 0 && ctx.is_null() {
            let mut plain_ret = Ok(());
            let plain = captured(|| plain_ret = check_size(w, h));
            assert_eq!((plain_ret, plain), (rust_ret, rust));
        }
    }
}

/// The image layout, allocation, copy and fill wrappers (`linesize`,
/// `fill_linesizes`, `fill_plane_sizes`, `fill_pointers`, `fill_arrays`,
/// `buffer_size`, `check_size_max_pixels`, `check_sar`, `Image::alloc`,
/// `copy`, `copy_from_mut`, `copy_from_uncacheable`, `copy_to_buffer`,
/// `fill_color`, `fill_black`) against their C functions, for every pixel
/// format C knows: results, out-parameters and plane layouts, and every byte
/// of independently allocated C and Rust images after each step of a copy
/// and fill sequence.
mod images {
    use core::ffi::{c_int, c_void};
    use core::ptr;

    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::imgutils::{
        Image, Planes, PlanesMut, buffer_size, buffer_size_for_linesizes, check_fill_black,
        check_fill_color, check_sar, check_size_max_pixels, check_size_max_pixels_with_context,
        copy, copy_from_mut, copy_from_uncacheable, copy_to_buffer, fill_arrays, fill_arrays_mut,
        fill_black, fill_color, fill_linesizes, fill_plane_sizes, fill_pointers, fill_pointers_mut,
        linesize,
    };
    use ffmpeg_bindings::libavutil::libavutil::opt::ObjectRef;
    use ffmpeg_bindings::libavutil::libavutil::pixfmt::{ColorRange, PixelFormat};
    use ffmpeg_bindings::libavutil::libavutil::rational::Rational;

    use super::{Ctx, captured, isolated};
    use crate::log::LogState;

    const NB: c_int = ffi::AVPixelFormat::AV_PIX_FMT_NB.0;

    fn format(raw: c_int) -> PixelFormat {
        PixelFormat::from_raw(ffi::AVPixelFormat(raw))
    }

    /// Every pixel format value worth asking about.
    fn formats() -> impl Iterator<Item = c_int> {
        -2..NB
    }

    /// Whether the tests run under `BorrowSanitizer`, whose cost per retag and
    /// per allocation makes every-format sweeps at real sizes run for
    /// hours: the image sweeps below then keep a few formats of each layout
    /// class and fewer sizes.
    fn under_bsan() -> bool {
        std::env::var("CRUSTIFY_FFMPEG_VARIANT").as_deref() == Ok("bsan")
    }

    /// One format of each layout class the image routines distinguish, for
    /// the sweeps under `BSan`: packed and planar YUV, subsampled chroma in
    /// both directions, semi-planar, RGB in both byte orders, a palette, a
    /// bitstream, float, 16-bit and alpha planes, Bayer, XYZ and a hardware
    /// format.
    const BSAN_FORMATS: [ffi::AVPixelFormat; 18] = [
        ffi::AVPixelFormat::AV_PIX_FMT_YUV420P,
        ffi::AVPixelFormat::AV_PIX_FMT_YUYV422,
        ffi::AVPixelFormat::AV_PIX_FMT_RGB24,
        ffi::AVPixelFormat::AV_PIX_FMT_YUV410P,
        ffi::AVPixelFormat::AV_PIX_FMT_GRAY8,
        ffi::AVPixelFormat::AV_PIX_FMT_MONOWHITE,
        ffi::AVPixelFormat::AV_PIX_FMT_PAL8,
        ffi::AVPixelFormat::AV_PIX_FMT_UYVY422,
        ffi::AVPixelFormat::AV_PIX_FMT_NV12,
        ffi::AVPixelFormat::AV_PIX_FMT_RGB48BE,
        ffi::AVPixelFormat::AV_PIX_FMT_YUVA444P16LE,
        ffi::AVPixelFormat::AV_PIX_FMT_GBRAPF32LE,
        ffi::AVPixelFormat::AV_PIX_FMT_BAYER_RGGB16BE,
        ffi::AVPixelFormat::AV_PIX_FMT_XYZ12LE,
        ffi::AVPixelFormat::AV_PIX_FMT_P010LE,
        ffi::AVPixelFormat::AV_PIX_FMT_VUYA,
        ffi::AVPixelFormat::AV_PIX_FMT_GBRP,
        ffi::AVPixelFormat::AV_PIX_FMT_VAAPI,
    ];

    /// The formats a sweep runs over: all of them, or [`BSAN_FORMATS`] and
    /// the out-of-range values under `BSan`.
    fn sweep_formats() -> Vec<c_int> {
        if under_bsan() {
            [-2, -1, NB]
                .into_iter()
                .chain(BSAN_FORMATS.map(|f| f.0))
                .collect()
        } else {
            formats().collect()
        }
    }

    fn result(ret: c_int) -> Result<c_int, i32> {
        if ret < 0 { Err(ret) } else { Ok(ret) }
    }

    /// Whether C's linesize arithmetic overflows for `raw` at `width`: the
    /// inputs on which the wrappers refuse to call C. Found as the wrappers
    /// cannot see it, from C's `av_image_fill_linesizes` on a 64-bit
    /// re-computation: bitstream formats whose bit count is near `INT_MAX`.
    fn c_overflows(raw: c_int, width: c_int) -> bool {
        // SAFETY: by value; NULL or C's static table.
        let desc = unsafe { ffi::av_pix_fmt_desc_get(ffi::AVPixelFormat(raw)) };
        if desc.is_null() || width < 0 {
            return false;
        }
        // SAFETY: a live, immutable static descriptor.
        let desc = unsafe { &*desc };
        if desc.flags & u64::from(ffi::AV_PIX_FMT_FLAG_BITSTREAM) == 0 {
            return false;
        }
        let step = desc.comp.iter().map(|c| i64::from(c.step)).max().unwrap();
        let bits = step * i64::from(width);
        bits <= i64::from(c_int::MAX) && bits > i64::from(c_int::MAX - 7)
    }

    const WIDTHS: [c_int; 10] = [
        -1,
        0,
        1,
        2,
        7,
        33,
        4096,
        1 << 20,
        c_int::MAX - 7,
        c_int::MAX,
    ];

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn linesizes_match_c() {
        for raw in formats() {
            for w in WIDTHS {
                if c_overflows(raw, w) {
                    assert!(linesize(format(raw), w, 0).is_err());
                    assert!(fill_linesizes(format(raw), w).is_err());
                    continue;
                }
                for plane in 0..4 {
                    let fmt = ffi::AVPixelFormat(raw);
                    // SAFETY: by value, a plane C indexes in bounds, and a
                    // width C's arithmetic does not overflow for.
                    let c = unsafe { ffi::av_image_get_linesize(fmt, w, plane) };
                    assert_eq!(
                        linesize(format(raw), w, plane as usize),
                        result(c),
                        "{raw} {w}"
                    );
                }
                let mut c = [-9; 4];
                // SAFETY: the `int[4]` C writes; no overflow, as above.
                let ret = unsafe {
                    ffi::av_image_fill_linesizes(c.as_mut_ptr(), ffi::AVPixelFormat(raw), w)
                };
                assert_eq!(
                    fill_linesizes(format(raw), w),
                    result(ret).map(|_| c),
                    "{raw} {w}"
                );
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn plane_sizes_and_pointer_layouts_match_c() {
        let heights = [1, 2, 3, 17, 1 << 20];
        let linesizes: [[c_int; 4]; 4] = [
            [64, 32, 32, 0],
            [1, 1, 1, 1],
            [100, 100, 0, 7],
            [1 << 30, 0, 0, 0],
        ];
        for raw in formats() {
            for h in heights {
                for ls in linesizes {
                    let wide = ls.map(|l| l as isize);
                    let mut c = [7usize; 4];
                    // SAFETY: the arrays C writes and reads; a positive
                    // height.
                    let ret = unsafe {
                        ffi::av_image_fill_plane_sizes(
                            c.as_mut_ptr(),
                            ffi::AVPixelFormat(raw),
                            h,
                            wide.as_ptr(),
                        )
                    };
                    let rust = fill_plane_sizes(format(raw), h, wide);
                    assert_eq!(rust, result(ret).map(|_| c), "{raw} {h} {ls:?}");
                    let mut data = [ptr::null_mut(); 4];
                    // SAFETY: a NULL buffer asks only for the size.
                    let size = unsafe {
                        ffi::av_image_fill_pointers(
                            data.as_mut_ptr(),
                            ffi::AVPixelFormat(raw),
                            h,
                            ptr::null_mut(),
                            ls.as_ptr(),
                        )
                    };
                    let rust = buffer_size_for_linesizes(format(raw), h, ls);
                    assert_eq!(rust, result(size).map(|s| s as usize), "{raw} {h} {ls:?}");
                    if size <= 0 || size > 1 << 16 {
                        continue;
                    }
                    let buf = vec![0u8; size as usize];
                    let c = c_offsets(&buf, |p, data| {
                        // SAFETY: `buf` holds the size C returned.
                        unsafe {
                            ffi::av_image_fill_pointers(
                                data,
                                ffi::AVPixelFormat(raw),
                                h,
                                p.cast_mut(),
                                ls.as_ptr(),
                            )
                        }
                    });
                    let planes = fill_pointers(&buf, format(raw), h, ls).unwrap();
                    assert_eq!(offsets(&buf, planes), c, "{raw} {h} {ls:?}");
                    let mut buf2 = vec![0u8; size as usize];
                    let base = buf2.as_ptr();
                    let planes = fill_pointers_mut(&mut buf2, format(raw), h, ls).unwrap();
                    assert_eq!(offsets_mut(base, &planes), c);
                }
            }
        }
    }

    /// C's plane layout in `buf`: each non-NULL plane's offset and length
    /// (to the next plane, or the size C returns), after `fill` writes the
    /// plane pointers for a buffer at `buf`'s start.
    fn c_offsets(
        buf: &[u8],
        fill: impl FnOnce(*const u8, *mut *mut u8) -> c_int,
    ) -> [(usize, usize); 4] {
        let mut data = [ptr::null_mut::<u8>(); 4];
        let size = fill(buf.as_ptr(), data.as_mut_ptr());
        assert!(size >= 0);
        let starts = data.map(|p| (!p.is_null()).then(|| p as usize - buf.as_ptr() as usize));
        core::array::from_fn(|i| match starts[i] {
            Some(s) => {
                let end = starts[i + 1..].iter().flatten().next().copied();
                (s, end.unwrap_or(size as usize) - s)
            }
            None => (0, 0),
        })
    }

    /// The layout of `planes` in `buf`, as `c_offsets` gives C's.
    fn offsets(buf: &[u8], planes: Planes<'_>) -> [(usize, usize); 4] {
        planes.data.map(|d| {
            if d.is_empty() {
                (0, 0)
            } else {
                (d.as_ptr() as usize - buf.as_ptr() as usize, d.len())
            }
        })
    }

    fn offsets_mut(base: *const u8, planes: &PlanesMut<'_>) -> [(usize, usize); 4] {
        planes.data.each_ref().map(|d| {
            if d.is_empty() {
                (0, 0)
            } else {
                (d.as_ptr() as usize - base as usize, d.len())
            }
        })
    }

    const SIZES: [(c_int, c_int); 4] = [(1, 1), (7, 5), (33, 17), (64, 3)];

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn array_layouts_and_buffer_sizes_match_c() {
        for raw in formats() {
            for (w, h) in SIZES
                .into_iter()
                .chain([(0, 4), (4, 0), (-1, 4), (1 << 16, 1 << 16)])
            {
                for align in [1usize, 4, 32, 64, 1 << 30] {
                    // SAFETY: by value; C checks the size first, which keeps
                    // its aligned linesizes in `int`.
                    let c = unsafe {
                        ffi::av_image_get_buffer_size(ffi::AVPixelFormat(raw), w, h, align as c_int)
                    };
                    let rust = buffer_size(format(raw), w, h, align);
                    assert_eq!(rust, result(c).map(|s| s as usize), "{raw} {w}x{h} {align}");
                    let mut c_ls = [-9; 4];
                    let mut data = [ptr::null_mut(); 4];
                    // SAFETY: the arrays C writes; a NULL buffer.
                    let ret = unsafe {
                        ffi::av_image_fill_arrays(
                            data.as_mut_ptr(),
                            c_ls.as_mut_ptr(),
                            ptr::null(),
                            ffi::AVPixelFormat(raw),
                            w,
                            h,
                            align as c_int,
                        )
                    };
                    assert_eq!(ret, c, "{raw} {w}x{h} {align}");
                    if c <= 0 || c > 1 << 20 {
                        assert!(fill_arrays(&[], format(raw), w, h, align).is_err());
                        continue;
                    }
                    let buf = vec![0u8; c as usize];
                    let c_layout = c_offsets(&buf, |p, data| {
                        let mut ls = [0; 4];
                        // SAFETY: `buf` holds the size C returned.
                        unsafe {
                            ffi::av_image_fill_arrays(
                                data,
                                ls.as_mut_ptr(),
                                p,
                                ffi::AVPixelFormat(raw),
                                w,
                                h,
                                align as c_int,
                            )
                        }
                    });
                    let planes = fill_arrays(&buf, format(raw), w, h, align).unwrap();
                    assert_eq!(planes.linesizes, c_ls.map(|l| l as isize));
                    assert_eq!(offsets(&buf, planes), c_layout, "{raw} {w}x{h} {align}");
                    let mut buf2 = vec![0u8; c as usize];
                    let base = buf2.as_ptr();
                    let planes = fill_arrays_mut(&mut buf2, format(raw), w, h, align).unwrap();
                    assert_eq!(offsets_mut(base, &planes), c_layout);
                    // One byte short is refused.
                    assert!(fill_arrays(&buf[1..], format(raw), w, h, align).is_err());
                }
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn size_and_sar_checks_match_c() {
        let dims = [
            0u32,
            1,
            1080,
            16384,
            65535,
            1 << 20,
            i32::MAX as u32 - 7,
            i32::MAX as u32,
            u32::MAX,
        ];
        let fmts = [
            -1,
            ffi::AVPixelFormat::AV_PIX_FMT_YUV420P.0,
            ffi::AVPixelFormat::AV_PIX_FMT_RGBAF32LE.0,
            ffi::AVPixelFormat::AV_PIX_FMT_MONOBLACK.0,
            ffi::AVPixelFormat::AV_PIX_FMT_BGR4.0,
            NB,
        ];
        for w in dims {
            for h in dims {
                for raw in fmts {
                    for max in [i64::MIN, 0, 1 << 20, i64::MAX - 1, i64::MAX] {
                        let rust = check_size_max_pixels(w, h, max, format(raw));
                        if c_overflows(raw, w as c_int) {
                            assert_eq!(rust, Err(-22));
                            continue;
                        }
                        // SAFETY: a NULL log context; no overflow, as above.
                        let c = unsafe {
                            ffi::av_image_check_size2(
                                w,
                                h,
                                max,
                                ffi::AVPixelFormat(raw),
                                0,
                                ptr::null_mut(),
                            )
                        };
                        assert_eq!(rust, result(c).map(drop), "{w}x{h} {raw} {max}");
                    }
                }
                for (num, den) in [
                    (0, 1),
                    (1, 1),
                    (1, 2),
                    (2, 1),
                    (1, 0),
                    (-1, 1),
                    (1, -1),
                    (1, i32::MAX),
                    (i32::MAX, 1),
                    (0, 0),
                ] {
                    let sar = ffi::AVRational { num, den };
                    // SAFETY: by value.
                    let c = unsafe { ffi::av_image_check_sar(w, h, sar) };
                    let rust = check_sar(w, h, Rational::new(num, den));
                    assert_eq!(rust, result(c).map(drop), "{w}x{h} {num}/{den}");
                }
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn check_size_max_pixels_logs_as_c() {
        if !isolated("imgutils::images::check_size_max_pixels_logs_as_c") {
            return;
        }
        let _state = LogState::lock();
        let class = ffi::AVClass {
            class_name: c"equiv-imgutils-parent".as_ptr(),
            item_name: Some(ffi::av_default_item_name),
            version: (ffi::LIBAVUTIL_VERSION_MAJOR << 16
                | ffi::LIBAVUTIL_VERSION_MINOR << 8
                | ffi::LIBAVUTIL_VERSION_MICRO) as c_int,
            ..Default::default()
        };
        let mut parent = Ctx {
            class: &raw const class,
        };
        let parent: *mut c_void = (&raw mut parent).cast();
        let yuv = PixelFormat::YUV420P;
        for (w, h, max, offset, ctx) in [
            (0u32, 7u32, i64::MAX, 0, ptr::null_mut()),
            (64, 64, 1000, 0, ptr::null_mut()),
            (64, 64, 4096, 0, ptr::null_mut()),
            (65536, 65536, i64::MAX, 4, parent),
            (640, 480, 1, -8, parent),
        ] {
            let mut rust_ret = Ok(());
            // SAFETY: `ctx` is NULL or a live context led by a valid class
            // with no options, callbacks or offsets, unwritten meanwhile.
            let obj = unsafe { ObjectRef::from_ptr(ctx) };
            let rust = captured(|| {
                rust_ret = check_size_max_pixels_with_context(w, h, max, yuv, offset, obj);
            });
            let mut c_ret = 0;
            let c = captured(|| {
                // SAFETY: as above.
                c_ret = unsafe { ffi::av_image_check_size2(w, h, max, yuv.into_raw(), offset, ctx) }
            });
            assert_eq!(rust_ret, result(c_ret).map(drop));
            assert_eq!(rust, c, "{w}x{h} {max} {offset}");
            assert_eq!(rust.len(), usize::from(c_ret < 0));
            if ctx.is_null() {
                let mut plain_ret = Ok(());
                let plain = captured(|| plain_ret = check_size_max_pixels(w, h, max, yuv));
                assert_eq!((plain_ret, plain), (rust_ret, rust));
            }
        }
        // The monochrome width C's linesize overflows for logs as an
        // invalid size, as C's fallback does.
        let w = i32::MAX as u32 - 3;
        let mut rust_ret = Ok(());
        let rust =
            captured(|| rust_ret = check_size_max_pixels(w, 1, i64::MAX, PixelFormat::MONOBLACK));
        let mut c_ret = 0;
        let c = captured(|| {
            // SAFETY: a NULL context, and a format C has no linesize for.
            c_ret = unsafe {
                ffi::av_image_check_size2(
                    w,
                    1,
                    i64::MAX,
                    ffi::AVPixelFormat(-1),
                    0,
                    ptr::null_mut(),
                )
            }
        });
        assert_eq!((rust_ret, rust), (result(c_ret).map(drop), c));
    }

    /// A C-allocated image: `av_image_alloc`'s pointers, linesizes and
    /// size, freed with `av_free`.
    struct CImage {
        data: [*mut u8; 4],
        linesizes: [c_int; 4],
        layout: [(usize, usize); 4],
    }

    impl CImage {
        fn alloc(raw: c_int, w: c_int, h: c_int, align: usize) -> Result<(Self, c_int), c_int> {
            let mut data = [ptr::null_mut(); 4];
            let mut linesizes = [0; 4];
            // SAFETY: the arrays C writes; C checks the size first.
            let size = unsafe {
                ffi::av_image_alloc(
                    data.as_mut_ptr(),
                    linesizes.as_mut_ptr(),
                    w,
                    h,
                    ffi::AVPixelFormat(raw),
                    align as c_int,
                )
            };
            if size < 0 {
                return Err(size);
            }
            let starts = data.map(|p| (!p.is_null()).then(|| p as usize - data[0] as usize));
            let layout = core::array::from_fn(|i| match starts[i] {
                Some(s) => {
                    let end = starts[i + 1..].iter().flatten().next().copied();
                    (s, end.unwrap_or(size as usize) - s)
                }
                None => (0, 0),
            });
            let img = Self {
                data,
                linesizes,
                layout,
            };
            // C leaves the block uninitialised; zero it, as `Image::alloc`.
            // SAFETY: C allocated `size + align` bytes at `data[0]`.
            unsafe { ptr::write_bytes(img.data[0], 0, size as usize + align) };
            Ok((img, size))
        }

        fn plane(&self, i: usize) -> &[u8] {
            let (_, len) = self.layout[i];
            if len == 0 {
                return &[];
            }
            // SAFETY: plane `i` of the live, zero-initialised block.
            unsafe { core::slice::from_raw_parts(self.data[i], len) }
        }

        fn plane_mut(&mut self, i: usize) -> &mut [u8] {
            let (_, len) = self.layout[i];
            if len == 0 {
                return &mut [];
            }
            // SAFETY: as in `plane`, exclusively borrowed.
            unsafe { core::slice::from_raw_parts_mut(self.data[i], len) }
        }

        fn wide_linesizes(&self) -> [isize; 4] {
            self.linesizes.map(|l| l as isize)
        }
    }

    impl Drop for CImage {
        fn drop(&mut self) {
            // SAFETY: `data[0]` is the block `av_image_alloc` returned.
            unsafe { ffi::av_free(self.data[0].cast()) };
        }
    }

    fn rust_layout(img: &Image) -> [(usize, usize); 4] {
        let planes = img.planes();
        let base = planes.data[0].as_ptr() as usize;
        planes.data.map(|d| {
            if d.is_empty() {
                (0, 0)
            } else {
                (d.as_ptr() as usize - base, d.len())
            }
        })
    }

    fn assert_same(c: &CImage, rust: &Image, what: &str) {
        for i in 0..4 {
            assert_eq!(c.plane(i), rust.planes().data[i], "{what}: plane {i}");
        }
    }

    /// Fills both images' planes with the same noise, each plane in one
    /// slice write (a write per byte is a retag per byte under `BSan`).
    fn noise(c: &mut CImage, rust: &mut Image, seed: u32) {
        let mut x = seed | 1;
        let planes = rust.planes_mut();
        for i in 0..4 {
            let bytes: Vec<u8> = (0..planes.data[i].len())
                .map(|_| {
                    x ^= x << 13;
                    x ^= x >> 17;
                    x ^= x << 5;
                    x as u8
                })
                .collect();
            planes.data[i].copy_from_slice(&bytes);
            c.plane_mut(i).copy_from_slice(&bytes);
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn allocations_match_c() {
        let (sizes, aligns): (&[(c_int, c_int)], &[usize]) = if under_bsan() {
            (&[(7, 5), (64, 3), (0, 1), (1 << 16, 1 << 16)], &[1, 64])
        } else {
            (
                &[
                    (1, 1),
                    (7, 5),
                    (33, 17),
                    (64, 3),
                    (0, 1),
                    (1, -1),
                    (1 << 16, 1 << 16),
                ],
                &[1, 2, 4, 16, 64],
            )
        };
        for raw in sweep_formats() {
            for &(w, h) in sizes {
                for &align in aligns {
                    let what = format!("{raw} {w}x{h} {align}");
                    let c = CImage::alloc(raw, w, h, align);
                    let rust = Image::alloc(w, h, format(raw), align);
                    match (c, rust) {
                        (Ok((c, size)), Ok(rust)) => {
                            assert_eq!(rust.linesizes(), c.linesizes, "{what}");
                            assert_eq!(rust_layout(&rust), c.layout, "{what}");
                            let total: usize = rust.planes().data.iter().map(|d| d.len()).sum();
                            assert_eq!(total, size as usize, "{what}");
                            assert_same(&c, &rust, &what);
                        }
                        (Err(c), Err(rust)) => assert_eq!(rust, c, "{what}"),
                        (c, rust) => {
                            panic!("{what}: C {:?}, Rust {:?}", c.map(|r| r.1), rust.map(drop))
                        }
                    }
                }
            }
        }
    }

    /// One copy and fill sequence on a C and a Rust image of the same
    /// format, comparing every byte after each step.
    fn copy_and_fill_sequence(raw: c_int, w: c_int, h: c_int, align: usize, seed: u32) {
        let what = format!("{raw} {w}x{h} {align}");
        let Ok((mut c_src, _)) = CImage::alloc(raw, w, h, align) else {
            return;
        };
        let (mut c_dst, _) = CImage::alloc(raw, w, h, align).unwrap();
        let mut src = Image::alloc(w, h, format(raw), align).unwrap();
        let mut dst = Image::alloc(w, h, format(raw), align).unwrap();
        noise(&mut c_src, &mut src, seed);
        noise(&mut c_dst, &mut dst, seed ^ 0x5555);

        // Whole and partial copies.
        for (cw, ch) in [(w, h), (w / 2, h / 2), (w, 1)] {
            // SAFETY: two live C images of this format at least `cw x ch`.
            unsafe {
                ffi::av_image_copy(
                    c_dst.data.as_ptr(),
                    c_dst.linesizes.as_ptr(),
                    c_src.data.as_ptr().cast(),
                    c_src.linesizes.as_ptr(),
                    ffi::AVPixelFormat(raw),
                    cw,
                    ch,
                );
            };
            copy(&mut dst.planes_mut(), src.planes(), format(raw), cw, ch).unwrap();
            assert_same(&c_dst, &dst, &format!("{what} copy {cw}x{ch}"));
        }
        noise(&mut c_dst, &mut dst, seed ^ 0xAAAA);
        // SAFETY: as above.
        unsafe {
            ffi::av_image_copy2(
                c_dst.data.as_ptr(),
                c_dst.linesizes.as_ptr(),
                c_src.data.as_ptr(),
                c_src.linesizes.as_ptr(),
                ffi::AVPixelFormat(raw),
                w,
                h,
            );
        };
        copy_from_mut(&mut dst.planes_mut(), &src.planes_mut(), format(raw), w, h).unwrap();
        assert_same(&c_dst, &dst, &format!("{what} copy2"));

        // Uncacheable copies, after fresh noise.
        noise(&mut c_dst, &mut dst, seed ^ 0x1234);
        let (c_dl, c_sl) = (c_dst.wide_linesizes(), c_src.wide_linesizes());
        if align == 64 {
            // SAFETY: as above; C's SSE4 path needs 16-byte aligned rows
            // inside the planes, which `av_image_alloc` gives at alignments
            // of 64.
            unsafe {
                ffi::av_image_copy_uc_from(
                    c_dst.data.as_ptr(),
                    c_dl.as_ptr(),
                    c_src.data.as_ptr().cast(),
                    c_sl.as_ptr(),
                    ffi::AVPixelFormat(raw),
                    w,
                    h,
                );
            };
            copy_from_uncacheable(&mut dst.planes_mut(), src.planes(), format(raw), w, h).unwrap();
            assert_same(&c_dst, &dst, &format!("{what} uc"));
        }

        // Copies into a packed buffer.
        for buf_align in [1usize, 4, 32] {
            // SAFETY: by value.
            let size = unsafe {
                ffi::av_image_get_buffer_size(ffi::AVPixelFormat(raw), w, h, buf_align as c_int)
            };
            let len = size.max(0) as usize + 3;
            let (mut c_buf, mut rust_buf) = (vec![0xA5u8; len], vec![0xA5u8; len]);
            // SAFETY: `c_buf` holds `len` bytes, which C checks the size
            // against; the source is a live C image of this format.
            let c = unsafe {
                ffi::av_image_copy_to_buffer(
                    c_buf.as_mut_ptr(),
                    len as c_int,
                    c_src.data.as_ptr().cast(),
                    c_src.linesizes.as_ptr(),
                    ffi::AVPixelFormat(raw),
                    w,
                    h,
                    buf_align as c_int,
                )
            };
            let rust = copy_to_buffer(&mut rust_buf, src.planes(), format(raw), w, h, buf_align);
            assert_eq!(
                rust,
                result(c).map(|n| n as usize),
                "{what} to_buffer {buf_align}"
            );
            assert_eq!(rust_buf, c_buf, "{what} to_buffer {buf_align}");
        }

        // Fills with colors, then black in both ranges.
        let mut x = seed | 1;
        for _ in 0..3 {
            let color: [u32; 4] = core::array::from_fn(|_| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                x
            });
            for (fw, fh) in [(w, h), (w / 2, h / 3)] {
                let dl = c_dst.wide_linesizes();
                // SAFETY: a live C image of this format at least `fw x fh`.
                let c = unsafe {
                    ffi::av_image_fill_color(
                        c_dst.data.as_ptr(),
                        dl.as_ptr(),
                        ffi::AVPixelFormat(raw),
                        color.as_ptr(),
                        fw,
                        fh,
                        0,
                    )
                };
                let rust = fill_color(&mut dst.planes_mut(), format(raw), color, fw, fh);
                assert_eq!(rust, result(c).map(drop), "{what} fill {color:x?}");
                assert_same(&c_dst, &dst, &format!("{what} fill {color:x?} {fw}x{fh}"));
                // SAFETY: NULL planes only check the arguments.
                let c = unsafe {
                    ffi::av_image_fill_color(
                        ptr::null(),
                        ptr::null(),
                        ffi::AVPixelFormat(raw),
                        color.as_ptr(),
                        fw,
                        0,
                        0,
                    )
                };
                assert_eq!(
                    check_fill_color(format(raw), color, fw),
                    result(c).map(drop)
                );
            }
        }
        for range in [ColorRange::MPEG, ColorRange::JPEG, ColorRange::UNSPECIFIED] {
            let dl = c_dst.wide_linesizes();
            // SAFETY: as above.
            let c = unsafe {
                ffi::av_image_fill_black(
                    c_dst.data.as_ptr(),
                    dl.as_ptr(),
                    ffi::AVPixelFormat(raw),
                    range.into_raw(),
                    w,
                    h,
                )
            };
            let rust = fill_black(&mut dst.planes_mut(), format(raw), range, w, h);
            assert_eq!(rust, result(c).map(drop), "{what} black {range:?}");
            assert_same(&c_dst, &dst, &format!("{what} black {range:?}"));
            // SAFETY: NULL planes only check the arguments.
            let c = unsafe {
                ffi::av_image_fill_black(
                    ptr::null(),
                    ptr::null(),
                    ffi::AVPixelFormat(raw),
                    range.into_raw(),
                    w,
                    0,
                )
            };
            assert_eq!(check_fill_black(format(raw), range, w), result(c).map(drop));
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn copies_and_fills_match_c_for_every_format() {
        let sizes: &[(c_int, c_int)] = if under_bsan() {
            &[(7, 5), (33, 17)]
        } else {
            &SIZES
        };
        for raw in sweep_formats()
            .into_iter()
            .filter(|&raw| (0..NB).contains(&raw))
        {
            for (i, &(w, h)) in sizes.iter().enumerate() {
                for align in [1usize, 64] {
                    copy_and_fill_sequence(raw, w, h, align, (raw as u32) << 8 | i as u32);
                }
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn copies_of_bottom_up_planes_match_c() {
        // Row 0 last in each plane: C's negative linesizes from the last
        // row's address.
        let (w, h) = (9, 6);
        let raw = ffi::AVPixelFormat::AV_PIX_FMT_YUV420P.0;
        let (mut c_src, _) = CImage::alloc(raw, w, h, 1).unwrap();
        let mut src = Image::alloc(w, h, format(raw), 1).unwrap();
        noise(&mut c_src, &mut src, 99);
        let mut c_dst = vec![0u8; 512];
        let mut rust_dst = vec![0u8; 512];
        let rows = [6usize, 3, 3];
        let ls = [9isize, 5, 5];
        let starts = [0usize, 100, 200];
        let mut c_ptrs = [ptr::null_mut::<u8>(); 4];
        let mut c_ls = [0; 4];
        for i in 0..3 {
            // SAFETY: the last row of plane `i`, inside `c_dst`.
            c_ptrs[i] = unsafe {
                c_dst
                    .as_mut_ptr()
                    .add(starts[i] + (rows[i] - 1) * ls[i] as usize)
            };
            c_ls[i] = -ls[i] as c_int;
        }
        // SAFETY: C copies each plane's rows upward from its last row, all
        // inside `c_dst`; the source is a live C image.
        unsafe {
            ffi::av_image_copy(
                c_ptrs.as_ptr(),
                c_ls.as_ptr(),
                c_src.data.as_ptr().cast(),
                c_src.linesizes.as_ptr(),
                ffi::AVPixelFormat(raw),
                w,
                h,
            );
        };
        let (p0, rest) = rust_dst.split_at_mut(100);
        let (p1, p2) = rest.split_at_mut(100);
        let mut planes = PlanesMut {
            data: [
                &mut p0[..(rows[0] - 1) * 9 + 9],
                &mut p1[..(rows[1] - 1) * 5 + 5],
                &mut p2[..(rows[2] - 1) * 5 + 5],
                &mut [],
            ],
            linesizes: [-9, -5, -5, 0],
        };
        copy(&mut planes, src.planes(), format(raw), w, h).unwrap();
        assert_eq!(rust_dst, c_dst);
    }

    /// A caller-laid-out image for the random probe: every plane in its own
    /// exactly sized heap buffer, rows a random padding apart, top-down or
    /// bottom-up per plane. Row 0 of plane `i` is at `origin[i]`.
    struct Laid {
        bufs: [Vec<u8>; 4],
        linesizes: [isize; 4],
        origin: [usize; 4],
    }

    impl Laid {
        /// The layout C's own routines give `raw` at `w x h`: `linesize`
        /// for the row widths, and the chroma shift for the rows.
        fn new(raw: c_int, w: c_int, h: c_int, x: &mut u32, seed: u8) -> Self {
            let fmt = format(raw);
            let (_, v) = fmt.chroma_subsampling().unwrap();
            let planes = fmt.plane_count().unwrap();
            let mut linesizes = [0isize; 4];
            let mut origin = [0usize; 4];
            let bufs = core::array::from_fn(|i| {
                if i == 1 && fmt == PixelFormat::PAL8 {
                    return (0..1024u32).map(|j| (j as u8) ^ seed).collect();
                }
                let width = linesize(fmt, w, i).unwrap() as usize;
                if i >= planes || width == 0 {
                    return Vec::new();
                }
                let rows = if i == 1 || i == 2 {
                    (h as usize).div_ceil(1 << v)
                } else {
                    h as usize
                };
                let pad = (xorshift(x) % 9) as usize;
                let ls = (width + pad) as isize;
                let bottom_up = xorshift(x) % 2 == 1;
                linesizes[i] = if bottom_up { -ls } else { ls };
                let len = (rows - 1) * ls as usize + width;
                origin[i] = if bottom_up {
                    (rows - 1) * ls as usize
                } else {
                    0
                };
                (0..len)
                    .map(|j| (j as u8).wrapping_mul(29).wrapping_add(seed))
                    .collect()
            });
            Self {
                bufs,
                linesizes,
                origin,
            }
        }

        fn planes(&self) -> Planes<'_> {
            Planes {
                data: self.bufs.each_ref().map(|b| &b[..]),
                linesizes: self.linesizes,
            }
        }

        fn planes_mut(&mut self) -> PlanesMut<'_> {
            PlanesMut {
                data: self.bufs.each_mut().map(|b| &mut b[..]),
                linesizes: self.linesizes,
            }
        }

        /// Where each plane's slice starts for an operation on the top `ch`
        /// rows of an image of `fmt`, as a caller passes them: a bottom-up
        /// plane's row 0 is the last row of its slice, so the slice starts
        /// at the last of the `ch` rows C reaches from the same row-0
        /// pointer.
        fn top_rows_start(&self, fmt: PixelFormat, ch: c_int) -> [usize; 4] {
            let (_, v) = fmt.chroma_subsampling().unwrap();
            core::array::from_fn(|i| {
                if self.linesizes[i] >= 0 || self.bufs[i].is_empty() {
                    return 0;
                }
                let rows = if i == 1 || i == 2 {
                    (ch as usize).div_ceil(1 << v)
                } else {
                    ch as usize
                };
                self.origin[i] - rows.saturating_sub(1) * self.linesizes[i].unsigned_abs()
            })
        }

        /// The planes for an operation on the top `ch` rows, for reading.
        fn top_rows(&self, fmt: PixelFormat, ch: c_int) -> Planes<'_> {
            let start = self.top_rows_start(fmt, ch);
            Planes {
                data: core::array::from_fn(|i| &self.bufs[i][start[i]..]),
                linesizes: self.linesizes,
            }
        }

        /// The planes for an operation on the top `ch` rows, for writing.
        fn top_rows_mut(&mut self, fmt: PixelFormat, ch: c_int) -> PlanesMut<'_> {
            let start = self.top_rows_start(fmt, ch);
            let mut i = 0;
            let data = self.bufs.each_mut().map(|b| {
                i += 1;
                &mut b[start[i - 1]..]
            });
            PlanesMut {
                data,
                linesizes: self.linesizes,
            }
        }

        /// C's row-0 pointers (NULL for an empty plane) and linesizes.
        fn c_ptrs(&mut self) -> ([*mut u8; 4], [c_int; 4], [isize; 4]) {
            let origin = self.origin;
            let mut ptrs = [ptr::null_mut(); 4];
            for (i, b) in self.bufs.iter_mut().enumerate() {
                if !b.is_empty() {
                    // SAFETY: `origin[i]` is inside the plane's buffer.
                    ptrs[i] = unsafe { b.as_mut_ptr().add(origin[i]) };
                }
            }
            (ptrs, self.linesizes.map(|l| l as c_int), self.linesizes)
        }
    }

    fn xorshift(x: &mut u32) -> u32 {
        *x ^= *x << 13;
        *x ^= *x >> 17;
        *x ^= *x << 5;
        *x
    }

    /// Copies, fills and packings between caller-laid-out planes of random
    /// padding and orientation, on a C and a Rust copy of the same layout,
    /// compared byte for byte after every step: the layouts
    /// `av_image_alloc` never produces, where the row-0 origins, the
    /// per-plane extents and the rejection of rows C would assert on are
    /// the wrappers' own arithmetic.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn random_plane_layouts_match_c() {
        let formats = [
            PixelFormat::YUV420P,
            PixelFormat::YUV410P,
            PixelFormat::NV12,
            PixelFormat::YUYV422,
            PixelFormat::RGB24,
            PixelFormat::PAL8,
            PixelFormat::MONOWHITE,
            PixelFormat::YUVA444P16LE,
            PixelFormat::GBRAPF32LE,
            PixelFormat::P010LE,
            PixelFormat::BAYER_RGGB16BE,
            PixelFormat::GRAY8,
        ];
        let under_bsan = under_bsan();
        let mut x = 0x9E37_79B9u32;
        for (k, fmt) in formats.iter().enumerate() {
            let raw = fmt.into_raw().0;
            for round in 0..if under_bsan { 3 } else { 24 } {
                let w = 1 + (xorshift(&mut x) % 37) as c_int;
                let h = 1 + (xorshift(&mut x) % 23) as c_int;
                let what = format!("{fmt:?} {w}x{h} round {round}");
                let seed = (k * 31 + round) as u8;
                let mut lx = x;
                let mut c_src = Laid::new(raw, w, h, &mut lx, seed);
                let src = Laid::new(raw, w, h, &mut x, seed);
                let mut lx = x;
                let mut c_dst = Laid::new(raw, w, h, &mut lx, seed ^ 0x55);
                let mut dst = Laid::new(raw, w, h, &mut x, seed ^ 0x55);
                assert_eq!(src.bufs, c_src.bufs);
                assert_eq!(dst.linesizes, c_dst.linesizes);

                // Whole, partial and degenerate copies.
                for (cw, ch) in [(w, h), (w / 2, h / 2), (w, 0), (0, h), (w, 1)] {
                    let (dp, dl, _) = c_dst.c_ptrs();
                    let (sp, sl, _) = c_src.c_ptrs();
                    // SAFETY: both images hold a `w x h` image of this
                    // format at these origins and linesizes.
                    unsafe {
                        ffi::av_image_copy(
                            dp.as_ptr(),
                            dl.as_ptr(),
                            sp.as_ptr().cast(),
                            sl.as_ptr(),
                            ffi::AVPixelFormat(raw),
                            cw,
                            ch,
                        );
                    };
                    copy(
                        &mut dst.top_rows_mut(*fmt, ch),
                        src.top_rows(*fmt, ch),
                        *fmt,
                        cw,
                        ch,
                    )
                    .unwrap_or_else(|e| panic!("{what} copy {cw}x{ch}: {e}"));
                    assert_eq!(dst.bufs, c_dst.bufs, "{what} copy {cw}x{ch}");
                }
                // The uncacheable copy on the same layouts: C's SSE4 path
                // is taken only where both linesizes cover the padded row.
                let (dp, _, dl) = c_dst.c_ptrs();
                let (sp, _, sl) = c_src.c_ptrs();
                let sound = (0..4).all(|i| {
                    let ls = [dl[i], sl[i]];
                    let width = linesize(*fmt, w, i).unwrap() as usize;
                    ls.iter()
                        .all(|&l| l < 0 || (l as usize) < width.next_multiple_of(64))
                });
                if sound {
                    // SAFETY: as above; no linesize covers the 64-byte
                    // padded row, so C copies plainly, row by row.
                    unsafe {
                        ffi::av_image_copy_uc_from(
                            dp.as_ptr(),
                            dl.as_ptr(),
                            sp.as_ptr().cast(),
                            sl.as_ptr(),
                            ffi::AVPixelFormat(raw),
                            w,
                            h,
                        );
                    };
                    copy_from_uncacheable(&mut dst.planes_mut(), src.planes(), *fmt, w, h)
                        .unwrap_or_else(|e| panic!("{what} uc: {e}"));
                    assert_eq!(dst.bufs, c_dst.bufs, "{what} uc");
                }

                // Fills of the destination, color then black.
                let color: [u32; 4] = core::array::from_fn(|_| xorshift(&mut x));
                for (fw, fh) in [(w, h), (w / 3, h / 2)] {
                    let (dp, _, dl) = c_dst.c_ptrs();
                    // SAFETY: the destination holds a `w x h` image.
                    let c = unsafe {
                        ffi::av_image_fill_color(
                            dp.as_ptr(),
                            dl.as_ptr(),
                            ffi::AVPixelFormat(raw),
                            color.as_ptr(),
                            fw,
                            fh,
                            0,
                        )
                    };
                    let r = fill_color(&mut dst.top_rows_mut(*fmt, fh), *fmt, color, fw, fh);
                    assert_eq!(r, result(c).map(drop), "{what} fill {fw}x{fh}");
                    assert_eq!(dst.bufs, c_dst.bufs, "{what} fill {fw}x{fh}");
                }
                let (dp, _, dl) = c_dst.c_ptrs();
                // SAFETY: as above.
                let c = unsafe {
                    ffi::av_image_fill_black(
                        dp.as_ptr(),
                        dl.as_ptr(),
                        ffi::AVPixelFormat(raw),
                        ColorRange::MPEG.into_raw(),
                        w,
                        h,
                    )
                };
                let r = fill_black(&mut dst.planes_mut(), *fmt, ColorRange::MPEG, w, h);
                assert_eq!(r, result(c).map(drop), "{what} black");
                assert_eq!(dst.bufs, c_dst.bufs, "{what} black");

                // Packing the source into a buffer at a random alignment.
                let align = 1usize << (xorshift(&mut x) % 7);
                let size = buffer_size(*fmt, w, h, align).unwrap();
                let (mut c_out, mut out) = (vec![0x3Cu8; size + 2], vec![0x3Cu8; size + 2]);
                let (sp, sl, _) = c_src.c_ptrs();
                // SAFETY: `c_out` holds `size + 2` bytes, which C checks the
                // image's size against; the source holds the image.
                let c = unsafe {
                    ffi::av_image_copy_to_buffer(
                        c_out.as_mut_ptr(),
                        (size + 2) as c_int,
                        sp.as_ptr().cast(),
                        sl.as_ptr(),
                        ffi::AVPixelFormat(raw),
                        w,
                        h,
                        align as c_int,
                    )
                };
                let r = copy_to_buffer(&mut out, src.planes(), *fmt, w, h, align);
                assert_eq!(r, result(c).map(|n| n as usize), "{what} pack {align}");
                assert_eq!(out, c_out, "{what} pack {align}");

                // The packed buffer read back as planes, and copied back
                // out into the (now filled) destination.
                let planes = fill_arrays(&out, *fmt, w, h, align).unwrap();
                let (dp, dl, _) = c_dst.c_ptrs();
                let mut cp = [ptr::null_mut::<u8>(); 4];
                let mut cl = [0 as c_int; 4];
                // SAFETY: `c_out` holds the packed image C laid out.
                unsafe {
                    ffi::av_image_fill_arrays(
                        cp.as_mut_ptr(),
                        cl.as_mut_ptr(),
                        c_out.as_ptr(),
                        ffi::AVPixelFormat(raw),
                        w,
                        h,
                        align as c_int,
                    )
                };
                assert_eq!(planes.linesizes.map(|l| l as c_int), cl, "{what} unpack");
                // SAFETY: as above, from the packed planes.
                unsafe {
                    ffi::av_image_copy(
                        dp.as_ptr(),
                        dl.as_ptr(),
                        cp.as_ptr().cast(),
                        cl.as_ptr(),
                        ffi::AVPixelFormat(raw),
                        w,
                        h,
                    );
                };
                copy(&mut dst.planes_mut(), planes, *fmt, w, h)
                    .unwrap_or_else(|e| panic!("{what} unpack copy: {e}"));
                assert_eq!(dst.bufs, c_dst.bufs, "{what} unpack copy");
                drop(dst);
                drop(src);
            }
        }
    }
}

mod pixel_steps {
    use core::ffi::c_int;
    use core::ptr;

    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::imgutils::max_pixel_steps;
    use ffmpeg_bindings::libavutil::libavutil::pixfmt::PixelFormat;

    const NB: c_int = ffi::AVPixelFormat::AV_PIX_FMT_NB.0;

    fn format(raw: c_int) -> PixelFormat {
        PixelFormat::from_raw(ffi::AVPixelFormat(raw))
    }

    fn c_get(raw: c_int) -> *const ffi::AVPixFmtDescriptor {
        // SAFETY: by value; C range-checks it.
        unsafe { ffi::av_pix_fmt_desc_get(ffi::AVPixelFormat(raw)) }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn max_pixel_steps_match_c_with_and_without_components() {
        for raw in 0..NB {
            let p = c_get(raw);
            let (mut steps, mut comps) = ([-9; 4], [-9; 4]);
            let mut steps_only = [-9; 4];
            // For the unsafe operations below: two `int[4]` out-arrays, or one and NULL, and an
            // entry of C's table, whose planes are 0 to 3.
            {
                // SAFETY: as the comment above this block states.
                unsafe {
                    ffi::av_image_fill_max_pixsteps(steps.as_mut_ptr(), comps.as_mut_ptr(), p);
                };
                // SAFETY: as the comment above this block states.
                unsafe {
                    ffi::av_image_fill_max_pixsteps(steps_only.as_mut_ptr(), ptr::null_mut(), p);
                };
            }
            let rust = max_pixel_steps(format(raw).descriptor().unwrap()).unwrap();
            assert_eq!(rust.steps, steps, "{raw}");
            assert_eq!(rust.steps, steps_only, "{raw}");
            assert_eq!(rust.components, comps.map(|c| c as usize), "{raw}");
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn max_pixel_steps_of_rust_built_descriptors_match_c() {
        use ffmpeg_bindings::libavutil::libavutil::pixdesc::{
            ComponentDescriptor, PixelFormatDescriptor,
        };
        let steps = [c_int::MIN, -1, 0, 1, 2, 7, c_int::MAX];
        for planes in [[0, 0, 0, 0], [0, 1, 1, 0], [3, 2, 1, 0], [2, 2, 3, 3]] {
            for (i, &a) in steps.iter().enumerate() {
                for &b in &steps[i..] {
                    let mut d = PixelFormatDescriptor::new();
                    for (c, mut slot) in d.as_mut().components_mut().iter_mut().enumerate() {
                        slot.set(ComponentDescriptor::from_raw(ffi::AVComponentDescriptor {
                            plane: planes[c],
                            step: if c % 2 == 0 { a } else { b },
                            offset: 0,
                            shift: 0,
                            depth: 8,
                        }));
                    }
                    let raw = d.into_raw();
                    let (mut c_steps, mut c_comps) = ([-9; 4], [-9; 4]);
                    // SAFETY: two `int[4]` out-arrays and a descriptor whose
                    // four slots are in planes 0 to 3.
                    unsafe {
                        ffi::av_image_fill_max_pixsteps(
                            c_steps.as_mut_ptr(),
                            c_comps.as_mut_ptr(),
                            &raw const raw,
                        );
                    };
                    let rust = max_pixel_steps(d.as_ref()).unwrap();
                    assert_eq!(rust.steps, c_steps, "{planes:?} {a} {b}");
                    assert_eq!(
                        rust.components,
                        c_comps.map(|c| c as usize),
                        "{planes:?} {a} {b}"
                    );
                }
            }
        }
    }
}
