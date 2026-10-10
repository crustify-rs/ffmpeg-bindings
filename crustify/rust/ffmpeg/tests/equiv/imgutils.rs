//! Equivalence of `libavutil/imgutils.h`'s `check_size`,
//! `check_size_with_context`, `copy_plane` and `copy_plane_from_uncacheable`
//! with `av_image_check_size`, `av_image_copy_plane` and
//! `av_image_copy_plane_uc_from`, on independently owned planes and over
//! multi-call sequences, comparing results, every byte of the destination
//! buffers after each step, and the lines C logs.

use core::cell::RefCell;
use core::ffi::{c_int, c_void};
use core::ptr;

use ffmpeg::libavutil::ffi;
use ffmpeg::libavutil::imgutils::{
    check_size, check_size_with_context, copy_plane, copy_plane_from_uncacheable,
};
use ffmpeg::libavutil::log::{self, Callback, Record};

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
        // SAFETY: both planes lie inside their buffers (`span` from the
        // row-0 offsets), the linesizes cover the width, and the buffers
        // are distinct.
        unsafe {
            ffi::av_image_copy_plane(
                c.0.as_mut_ptr().add(s.doff + origin(s.dl, s.h)),
                s.dl as c_int,
                src.0.as_ptr().add(s.soff + origin(s.sl, s.h)),
                s.sl as c_int,
                s.w as c_int,
                s.h as c_int,
            )
        };
        assert_eq!(rust, c, "after step {i}: {s:?}");
    }
}

/// On planes C handles (its SSE4 path on 16-byte aligned rows covering the
/// rounded-up width, or its plain path), the results match byte for byte,
/// the SSE4 path's copies past the width included.
#[test]
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
            // SAFETY: as in `copy_plane_matches_c_step_by_step`; on the SSE4
            // path the rows are 16-byte aligned and cover the rounded-up
            // width inside the buffers.
            unsafe {
                ffi::av_image_copy_plane_uc_from(
                    c.0.as_mut_ptr().add(s.doff + origin(s.dl, s.h)),
                    s.dl,
                    src.0.as_ptr().add(s.soff + origin(s.sl, s.h)),
                    s.sl,
                    s.w as isize,
                    s.h as c_int,
                )
            };
        }
        assert_eq!(rust, c, "after step {i}: {s:?}");
    }
}

#[test]
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
            // SAFETY: as above.
            let with_context = unsafe { check_size_with_context(w, h, 0, ptr::null_mut()) };
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

#[test]
fn check_size_logs_as_c() {
    let _state = LogState::lock();
    let class = ffi::AVClass {
        class_name: c"equiv-imgutils-parent".as_ptr(),
        item_name: Some(ffi::av_default_item_name),
        version: (ffi::LIBAVUTIL_VERSION_MAJOR << 16
            | ffi::LIBAVUTIL_VERSION_MINOR << 8
            | ffi::LIBAVUTIL_VERSION_MICRO) as c_int,
        ..Default::default()
    };
    let mut parent = Ctx { class: &class };
    let parent: *mut c_void = (&raw mut parent).cast();
    for (w, h, offset, ctx) in [
        (0u32, 7u32, 0, ptr::null_mut()),
        (u32::MAX, 1, 0, ptr::null_mut()),
        (65536, 65536, 4, parent),
        (1, 0, -8, parent),
        (64, 64, 0, parent),
    ] {
        let mut rust_ret = Ok(());
        // SAFETY: `ctx` is NULL or a live context led by a valid class.
        let rust = captured(|| rust_ret = unsafe { check_size_with_context(w, h, offset, ctx) });
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
