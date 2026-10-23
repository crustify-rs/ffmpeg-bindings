//! UB tests of `libavutil/bprint.h`'s safe [`PaddedStruct`] constants: they
//! evaluate at compile time, without overflow, and describe the C structure's
//! storage.
#![expect(
    clippy::cast_possible_truncation,
    reason = "test inputs are chosen within range, and the casts mirror the C conversions under comparison"
)]
#![expect(
    clippy::items_after_statements,
    reason = "each test is one scenario, its helpers and steps kept where they are used"
)]

use core::mem::{align_of, size_of};

use ffmpeg_bindings::libavutil::libavutil::bprint::PaddedStruct;
use ffmpeg_bindings::libavutil::libavutil::ffi;

type BPrint = ffi::AVBPrint;

// Evaluated in const context: an underflow here fails the build.
const SIZE: usize = <BPrint as PaddedStruct>::SIZE;
const FIELDS_SIZE: usize = <BPrint as PaddedStruct>::FIELDS_SIZE;
const PADDING_LEN: usize = <BPrint as PaddedStruct>::PADDING_LEN;

#[test]
fn constants_describe_the_storage() {
    assert_eq!(FIELDS_SIZE + PADDING_LEN, SIZE);
    assert_eq!(size_of::<BPrint>(), SIZE);
    assert_eq!(SIZE % align_of::<BPrint>(), 0);
    assert_eq!(FIELDS_SIZE % align_of::<BPrint>(), 0);
}

/// UB tests of [`BPrintFields`], `struct ff_pad_helper_AVBPrint`: a value
/// Rust owns, its copies, and its `'static` string, read and written from one
/// thread and from several.
mod fields {
    use core::ffi::CStr;
    use std::sync::Barrier;
    use std::thread;

    use ffmpeg_bindings::libavutil::libavutil::bprint::{
        BPrintFields, BPrintFieldsMut, PaddedStruct,
    };
    use ffmpeg_bindings::libavutil::libavutil::ffi;

    static GREETING: &CStr = c"Hello, AVBPrint";

    /// Every field set through one exclusive handle, which `fill` reborrows.
    fn fill(mut m: BPrintFieldsMut<'_>, s: Option<&'static CStr>, n: u32) {
        m.set_str(s);
        m.set_len(n);
        m.set_size(n.wrapping_add(1));
        m.set_size_max(n.wrapping_add(2));
        m.set_internal_buffer([n as u8]);
    }

    #[test]
    fn fields_size_is_the_values_size() {
        assert_eq!(
            <ffi::AVBPrint as PaddedStruct>::FIELDS_SIZE,
            size_of::<BPrintFields>()
        );
    }

    #[test]
    fn every_field_reads_back_at_its_extremes() {
        let mut f = BPrintFields::default();
        for (s, n) in [
            (None, 0),
            (Some(c""), 1),
            (Some(GREETING), u32::MAX),
            (Some(c"\x7f\xff"), u32::MAX - 1),
        ] {
            fill(f.as_mut(), s, n);
            let r = f.as_ref();
            assert_eq!(r.str(), s);
            assert_eq!(r.len(), n);
            assert_eq!(r.size(), n.wrapping_add(1));
            assert_eq!(r.size_max(), n.wrapping_add(2));
            assert_eq!(r.internal_buffer(), [n as u8]);
        }
    }

    #[test]
    fn the_exclusive_handle_reborrows_and_reads() {
        let mut f = BPrintFields::new();
        let mut m = f.as_mut();
        fill(m.as_mut(), Some(GREETING), 7);
        // The handle is still usable after the reborrow ended.
        m.set_len(8);
        assert_eq!(m.as_ref().len(), 8);
        assert_eq!(m.as_ref().str(), Some(GREETING));
        // The exclusive handle's last use ends its borrow.
        assert_eq!(f.as_ref().size(), 8);
    }

    #[test]
    fn a_string_read_outlives_the_handle() {
        let mut f = BPrintFields::new();
        f.as_mut().set_str(Some(GREETING));
        let s = {
            let r = f.as_ref();
            r.str()
        };
        assert_eq!(s.map(CStr::to_bytes), Some(&b"Hello, AVBPrint"[..]));
        // Once the string read is done, the field may be overwritten.
        f.as_mut().set_str(None);
        assert_eq!(f.as_ref().str(), None);
    }

    #[test]
    fn copies_are_independent_and_share_the_string() {
        let mut a = BPrintFields::new();
        fill(a.as_mut(), Some(GREETING), 3);
        let mut b = a;
        #[expect(clippy::clone_on_copy, reason = "the `Clone` impl itself")]
        let c = a.clone();
        b.as_mut().set_len(4);
        b.as_mut().set_str(Some(c"other"));
        assert_eq!(
            (a.as_ref().len(), b.as_ref().len(), c.as_ref().len()),
            (3, 4, 3)
        );
        assert_eq!(a.as_ref().str(), Some(GREETING));
        assert_eq!(c.as_ref().str(), Some(GREETING));
        assert_eq!(b.as_ref().str(), Some(c"other"));
        assert_eq!(c.as_ref().str().unwrap().as_ptr(), GREETING.as_ptr());
    }

    #[test]
    fn values_move_through_heap_storage() {
        let mut v: Vec<BPrintFields> = (0..64)
            .map(|i| {
                let mut f = BPrintFields::new();
                fill(f.as_mut(), Some(GREETING), i);
                f
            })
            .collect();
        // Growing reallocates and moves every value.
        v.reserve(1024);
        v.insert(0, BPrintFields::default());
        let boxed = Box::new(v[10]);
        assert_eq!(BPrintFields::as_ref(&boxed).len(), 9);
        for (i, f) in v.iter().enumerate().skip(1) {
            assert_eq!(f.as_ref().len(), i as u32 - 1);
            assert_eq!(f.as_ref().str(), Some(GREETING));
        }
        assert_eq!(v[0].as_ref().str(), None);
    }

    #[test]
    fn shared_values_and_handles_are_read_from_many_threads() {
        let mut f = BPrintFields::new();
        fill(f.as_mut(), Some(GREETING), 42);
        let f = f;
        let r = f.as_ref();
        thread::scope(|s| {
            for _ in 0..4 {
                // `&BPrintFields: Send` and `BPrintFieldsRef: Send`.
                s.spawn(|| {
                    for _ in 0..1000 {
                        assert_eq!(f.as_ref().str(), Some(GREETING));
                        assert_eq!(r.len(), 42);
                    }
                });
            }
        });
    }

    #[test]
    fn values_and_exclusive_handles_move_between_threads() {
        let mut f = BPrintFields::new();
        fill(f.as_mut(), Some(GREETING), 1);
        // `BPrintFields: Send`: a moved copy, written on another thread.
        let moved = thread::spawn(move || {
            f.as_mut().set_len(2);
            f
        })
        .join()
        .unwrap();
        assert_eq!(moved.as_ref().len(), 2);

        // `BPrintFieldsMut: Send`: each thread writes its own value while the
        // others read theirs.
        let mut values = [BPrintFields::new(); 4];
        let barrier = Barrier::new(values.len());
        thread::scope(|s| {
            for (i, f) in values.iter_mut().enumerate() {
                let m = f.as_mut();
                let barrier = &barrier;
                s.spawn(move || {
                    barrier.wait();
                    fill(m, Some(GREETING), i as u32);
                });
            }
        });
        for (i, f) in values.iter().enumerate() {
            assert_eq!(f.as_ref().len(), i as u32);
        }

        // `&BPrintFieldsMut: Send` (`BPrintFieldsMut: Sync`): shared reads
        // through it.
        let mut g = BPrintFields::new();
        fill(g.as_mut(), Some(GREETING), 9);
        let m = g.as_mut();
        thread::scope(|s| {
            for _ in 0..2 {
                s.spawn(|| assert_eq!(m.as_ref().size(), 10));
            }
        });
    }
}

/// UB tests of the print buffer owners, `BPrintBox` and `BPrintOnBuffer`,
/// through the safe API: every `size_init`/`size_max` shape, caller buffers
/// of every length class, each finalizer, truncation, and owners and handles
/// moved and shared across threads.
mod owners {
    use std::thread;

    use ffmpeg_bindings::libavutil::libavutil::bprint::{
        BPrint, BPrintBox, BPrintOnBuffer, BPrintRef,
    };

    fn under_bsan() -> bool {
        std::env::var("CRUSTIFY_FFMPEG_VARIANT").as_deref() == Ok("bsan")
    }

    /// `size_max` values at every boundary `av_bprint_init` distinguishes.
    const SIZE_MAX: [u32; 9] = [
        BPrint::SIZE_COUNT_ONLY,
        BPrint::SIZE_AUTOMATIC,
        2,
        BPrint::INLINE_CAPACITY - 1,
        BPrint::INLINE_CAPACITY,
        BPrint::INLINE_CAPACITY + 1,
        5000,
        u32::MAX - 1,
        BPrint::SIZE_UNLIMITED,
    ];

    /// `size_init` values around the inline storage, and one no allocator
    /// can satisfy (4 GiB, beyond `av_max_alloc`'s default `INT_MAX`).
    const SIZE_INIT: [u32; 7] = [0, 1, 2, 1004, 1005, 4096, u32::MAX];

    /// Every getter, whose results must be consistent with an empty buffer.
    fn check_empty(r: BPrintRef<'_>) {
        assert_eq!(r.len(), 0);
        assert!(r.is_empty());
        assert_eq!(r.as_bytes(), b"");
        assert_eq!(r.as_c_str(), c"");
        assert!(r.size() <= r.size_max());
        let _ = r.is_inline();
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn every_init_shape_reads_empty_and_drops() {
        for size_max in SIZE_MAX {
            for size_init in SIZE_INIT {
                let mut buf = BPrint::new(size_init, size_max).unwrap();
                check_empty(buf.as_ref());
                // Inline unless `size_init` asked for more than the inline
                // size, the limit allowed growing, and `av_realloc` could
                // (it refuses more than `INT_MAX` bytes).
                let max = if size_max == BPrint::SIZE_AUTOMATIC {
                    BPrint::INLINE_CAPACITY
                } else {
                    size_max
                };
                let inline = max.min(BPrint::INLINE_CAPACITY);
                let grows = size_init > inline
                    && inline != max
                    && i32::try_from(size_init.min(max)).is_ok();
                assert_eq!(buf.as_ref().is_inline(), !grows, "{size_init} {size_max}");
                buf.as_mut().truncate(0);
                buf.as_mut().as_mut().truncate(u32::MAX);
                check_empty(buf.as_mut().as_ref());
            }
        }
    }

    /// Descriptor allocation fails before C sees the caller buffer. The
    /// limit is process-wide, so run this case in an isolated child.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn descriptor_allocation_failure_leaves_caller_storage_untouched() {
        const CHILD: &str = "CRUSTIFY_BPRINT_ALLOCATION_FAILURE_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "bprint::owners::descriptor_allocation_failure_leaves_caller_storage_untouched",
                    "--test-threads=1",
                ])
                .env(CHILD, "1")
                .status()
                .unwrap();
            assert!(status.success());
            return;
        }

        use ffmpeg_bindings::libavutil::libavutil::mem::set_max_alloc;
        for limit in [0, 1, 1023] {
            set_max_alloc(limit);
            assert!(BPrint::new(0, BPrint::SIZE_AUTOMATIC).is_none());
            assert!(BPrint::for_buffer(&mut []).is_none());
            let mut out = [0xA5; 8];
            assert!(BPrint::for_buffer(&mut out).is_none());
            assert_eq!(out, [0xA5; 8]);
        }

        // At exactly the descriptor size construction succeeds; failure to
        // allocate the requested string leaves valid inline storage.
        set_max_alloc(1024);
        let buf = BPrint::new(4096, BPrint::SIZE_UNLIMITED).unwrap();
        assert!(buf.as_ref().is_inline());
        check_empty(buf.as_ref());
        drop(buf);
        let mut out = [0xA5; 8];
        drop(BPrint::for_buffer(&mut out).unwrap());
        assert_eq!(out, [0, 0xA5, 0xA5, 0xA5, 0xA5, 0xA5, 0xA5, 0xA5]);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn each_finalizer_hands_out_the_empty_string() {
        for size_max in SIZE_MAX {
            for size_init in [0, 4096] {
                let s = BPrint::into_string(BPrint::new(size_init, size_max).unwrap()).unwrap();
                assert_eq!(s.as_c_str(), c"");
                let b = BPrint::into_bytes(BPrint::new(size_init, size_max).unwrap()).unwrap();
                assert_eq!(b.as_slice(), b"");
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn caller_buffers_of_every_length_hold_an_empty_string() {
        for len in [0, 1, 2, 3, 1024, 70_000] {
            let mut out = vec![0xA5_u8; len];
            let mut buf = BPrint::for_buffer(&mut out).unwrap();
            check_empty(buf.as_ref());
            assert_eq!(buf.as_ref().size() as usize, len);
            assert_eq!(buf.as_ref().is_inline(), len == 0);
            buf.as_mut().truncate(0);
            drop(buf);
            if len > 0 {
                assert_eq!(out[0], 0);
                // One slice comparison: BSan retags per element visited.
                assert_eq!(out[1..], vec![0xA5; len - 1][..]);
            }
        }
    }

    /// A buffer past `u32::MAX` bytes is used up to that many. Zeroed pages
    /// cost nothing until touched; skipped under `BSan`, whose shadow does not
    /// scale to them.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn a_caller_buffer_beyond_u32_is_clamped() {
        if under_bsan() {
            return;
        }
        let mut out = vec![0_u8; u32::MAX as usize + 2];
        let buf = BPrint::for_buffer(&mut out).unwrap();
        assert_eq!(buf.as_ref().size(), u32::MAX);
        assert_eq!(buf.as_ref().size_max(), u32::MAX);
        assert_eq!(buf.as_ref().as_bytes(), b"");
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn owners_and_handles_cross_threads() {
        let mut boxes: Vec<BPrintBox> = (0..4)
            .map(|i| BPrint::new(i * 2000, BPrint::SIZE_UNLIMITED).unwrap())
            .collect();
        // Shared handles read concurrently.
        thread::scope(|s| {
            for b in &boxes {
                let r = b.as_ref();
                s.spawn(move || {
                    for _ in 0..100 {
                        check_empty(r);
                    }
                });
            }
        });
        // Exclusive handles write on other threads.
        thread::scope(|s| {
            for b in &mut boxes {
                let mut m = b.as_mut();
                s.spawn(move || m.truncate(0));
            }
        });
        // Owners finalize and drop where they land.
        let joins: Vec<_> = boxes
            .into_iter()
            .enumerate()
            .map(|(i, b)| {
                thread::spawn(move || {
                    if i % 2 == 0 {
                        BPrint::into_string(b).unwrap().len()
                    } else {
                        drop(b);
                        0
                    }
                })
            })
            .collect();
        for j in joins {
            assert_eq!(j.join().unwrap(), 0);
        }

        let mut out = [0xA5_u8; 32];
        let on: BPrintOnBuffer<'_> = BPrint::for_buffer(&mut out).unwrap();
        thread::scope(|s| {
            s.spawn(move || {
                check_empty(on.as_ref());
                drop(on);
            });
        });
        assert_eq!(out[0], 0);
    }
}

/// UB tests of the appenders, `av_bprint_is_complete` and [`Tm`] through the
/// safe API: every appender on every owner shape, into growth, limits and
/// truncation; counts at their cap; `strftime` over extreme fields and odd
/// formats; storage handed out by `append_with`, written in full, refused
/// and abandoned by a panic; and buffers, handles and times on other
/// threads.
///
/// [`Tm`]: glibc_bindings::libc::time::bits::types::struct_tm_h::Tm
mod appenders {
    use core::ffi::{CStr, c_int, c_long};
    use core::fmt::Write;
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::thread;

    use ffmpeg_bindings::libavutil::libavutil::avstring::{EscapeFlags, EscapeMode};
    use ffmpeg_bindings::libavutil::libavutil::bprint::{BPrint, BPrintMut, BPrintRef};
    use glibc_bindings::libc::time::bits::types::struct_tm_h::Tm;

    /// `size_max` values at every boundary the appenders distinguish.
    const SIZE_MAX: [u32; 8] = [
        BPrint::SIZE_COUNT_ONLY,
        BPrint::SIZE_AUTOMATIC,
        2,
        30,
        BPrint::INLINE_CAPACITY,
        BPrint::INLINE_CAPACITY + 1,
        3000,
        BPrint::SIZE_UNLIMITED,
    ];

    /// Caller buffer lengths: count only, room for the NUL alone, small, and
    /// past the inline storage.
    const LENS: [usize; 5] = [0, 1, 2, 30, 1100];

    /// The relations every getter must keep, read in full.
    fn check(r: BPrintRef<'_>) {
        let (len, size) = (r.len(), r.size());
        assert!(len <= u32::MAX - 5);
        assert_eq!(r.is_complete(), len < size);
        assert_eq!(r.as_bytes().len() as u32, len.min(size.saturating_sub(1)));
        let c = r.as_c_str().to_bytes();
        assert!(c.len() <= r.as_bytes().len());
        assert_eq!(c, &r.as_bytes()[..c.len()]);
        assert!(size <= r.size_max());
        assert_eq!(r.is_empty(), len == 0);
    }

    /// A time with every field set, in a named zone.
    fn some_time() -> Tm {
        let mut tm = Tm::new();
        let mut m = tm.as_mut();
        m.set_year(124);
        m.set_mon(1);
        m.set_mday(29);
        m.set_hour(23);
        m.set_min(59);
        m.set_sec(60);
        m.set_wday(4);
        m.set_yday(59);
        m.set_isdst(1);
        m.set_gmtoff(-12_600);
        m.set_zone(Some(c"NDT"));
        tm
    }

    /// Every appender once, several large enough to grow or truncate.
    fn append_all(m: &mut BPrintMut<'_>, tm: &Tm) {
        m.append_bytes(b"");
        m.append_bytes(b"bin\0ary\xff");
        m.append_repeated(b'r', 0);
        m.append_repeated(b'r', 700);
        m.append_escaped(
            c" e'sc\\ape: ",
            Some(c":"),
            EscapeMode::Backslash,
            EscapeFlags::empty(),
        );
        m.append_escaped(
            c"<x a='1'/>",
            None,
            EscapeMode::Xml,
            EscapeFlags::XML_SINGLE_QUOTES,
        );
        m.append_escaped(c"'q'", None, EscapeMode::Quote, EscapeFlags::STRICT);
        m.append_escaped(
            c"",
            Some(c""),
            EscapeMode::Auto,
            EscapeFlags::from_bits_retain(-1),
        );
        m.append_fmt(format_args!("{}|{:>8}|{:e}", u64::MAX, "pad", f64::MIN));
        m.append_fmt(format_args!(""));
        m.append_c_str(c"c string");
        m.append_c_str(c"");
        m.append_strftime(c"%c %Z %z %j %G-W%V", tm.as_ref());
        m.append_strftime(c"", tm.as_ref());
        let n = m.append_with(400, |spare| {
            spare.fill(b'w');
            spare.len()
        });
        assert!(n <= 400);
        let nul = '\0';
        write!(m, "{nul}written {n}").unwrap();
        m.append_repeated(b'R', 2500);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn every_appender_on_every_shape_keeps_the_invariants() {
        let tm = some_time();
        for size_max in SIZE_MAX {
            let mut buf = BPrint::new(0, size_max).unwrap();
            for _ in 0..2 {
                append_all(&mut buf.as_mut(), &tm);
                check(buf.as_ref());
                buf.as_mut().truncate(5);
                check(buf.as_ref());
                append_all(&mut buf.as_mut(), &tm);
                check(buf.as_ref());
                buf.as_mut().clear();
                check(buf.as_ref());
            }
            append_all(&mut buf.as_mut(), &tm);
            let bytes = buf.as_ref().as_bytes().to_vec();
            if size_max % 2 == 0 {
                assert_eq!(BPrint::into_bytes(buf).unwrap().as_slice(), bytes);
            } else {
                let s = BPrint::into_string(buf).unwrap();
                assert!(bytes.starts_with(s.as_bytes()));
            }
        }
        for len in LENS {
            let mut out = vec![0x5A_u8; len];
            let mut buf = BPrint::for_buffer(&mut out).unwrap();
            for _ in 0..2 {
                append_all(&mut buf.as_mut(), &tm);
                check(buf.as_ref());
                assert_eq!(buf.as_ref().size() as usize, len);
                buf.as_mut().clear();
                check(buf.as_ref());
            }
            append_all(&mut buf.as_mut(), &tm);
            let stored = buf.as_ref().as_bytes().to_vec();
            drop(buf);
            // What was stored stays in the caller buffer, terminated.
            if len > 0 {
                assert_eq!(out[..stored.len()], stored[..]);
                assert_eq!(out[stored.len()], 0);
            }
        }
    }

    /// `len` stops at `u32::MAX - 5` however much is counted, where C's
    /// unsigned arithmetic would otherwise wrap; nothing is stored or
    /// allocated for it.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn counts_stop_at_their_cap() {
        let mut storage = [0_u8; 4];
        let mut on = BPrint::for_buffer(&mut storage).unwrap();
        let mut count = BPrint::new(0, BPrint::SIZE_COUNT_ONLY).unwrap();
        let mut auto = BPrint::new(0, BPrint::SIZE_AUTOMATIC).unwrap();
        for mut m in [on.as_mut(), count.as_mut(), auto.as_mut()] {
            for _ in 0..3 {
                m.append_repeated(b'n', u32::MAX);
                assert_eq!(m.as_ref().len(), u32::MAX - 5);
                check(m.as_ref());
            }
            m.append_fmt(format_args!("more"));
            m.append_bytes(b"more");
            m.append_c_str(c"more");
            m.append_strftime(c"%Y", Tm::new().as_ref());
            m.append_escaped(c"more", None, EscapeMode::Quote, EscapeFlags::empty());
            assert_eq!(m.append_with(u32::MAX, |spare| spare.len()), 0);
            assert_eq!(m.as_ref().len(), u32::MAX - 5);
            check(m.as_ref());
            m.truncate(u32::MAX - 6);
            check(m.as_ref());
            m.clear();
            check(m.as_ref());
        }
        drop(on);
        // Filled to the brim, then emptied by `clear`'s NUL.
        assert_eq!(storage, *b"\0nn\0");
    }

    /// `strftime` reads only what its conversions name; out-of-range fields
    /// and odd formats (lone `%`, modifiers, unknown conversions) give
    /// unspecified characters, never out-of-bounds access.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn strftime_over_extreme_fields_and_odd_formats() {
        const INTS: [c_int; 7] = [c_int::MIN, -1_000_000, -1, 0, 12, 400, c_int::MAX];
        const OFFSETS: [c_long; 4] = [c_long::MIN, -1, 0, c_long::MAX];
        const FORMATS: [&CStr; 6] = [
            c"%a%A%b%B%c%C%d%D%e%F%g%G%h%H%I%j%k%l%m%M%n%p%P%r%R%s%S%t%T%u%U%V%w%W%x%X%y%Y%z%Z%+%%",
            c"%Ec%EC%Ex%EX%Ey%EY%Od%Oe%OH%OI%Om%OM%OS%Ou%OU%OV%Ow%OW%Oy",
            c"%_10Y%-d%0e%^a%#b%10000c",
            c"%",
            c"%E%O%q%",
            c"%Z%Z%Z%Z%Z%Z%Z%Z%Z%Z%Z%Z%Z%Z%Z%Z",
        ];
        let mut buf = BPrint::new(0, BPrint::SIZE_AUTOMATIC).unwrap();
        for field in 0..9 {
            for v in INTS {
                let mut tm = some_time();
                let mut m = tm.as_mut();
                match field {
                    0 => m.set_sec(v),
                    1 => m.set_min(v),
                    2 => m.set_hour(v),
                    3 => m.set_mday(v),
                    4 => m.set_mon(v),
                    5 => m.set_year(v),
                    6 => m.set_wday(v),
                    7 => m.set_yday(v),
                    _ => m.set_isdst(v),
                }
                for (i, zone) in [None, Some(c""), Some(c"Z")].into_iter().enumerate() {
                    tm.as_mut().set_zone(zone);
                    tm.as_mut().set_gmtoff(OFFSETS[(field + i) % OFFSETS.len()]);
                    for fmt in FORMATS {
                        buf.as_mut().append_strftime(fmt, tm.as_ref());
                        check(buf.as_ref());
                        buf.as_mut().clear();
                    }
                }
            }
        }
        // An unlimited buffer grows for wide output, a limited one falls
        // back or truncates.
        for size_max in [BPrint::SIZE_UNLIMITED, 30, 1500] {
            let mut buf = BPrint::new(0, size_max).unwrap();
            buf.as_mut()
                .append_strftime(c"%3000c", some_time().as_ref());
            check(buf.as_ref());
            buf.as_mut().append_strftime(c"%c", some_time().as_ref());
            check(buf.as_ref());
        }
    }

    /// Storage handed out is the room short of the NUL's byte: written in
    /// full each time it fills the buffer to its last byte, also inside the
    /// structure; a closure may write nothing, refuse with a panic, or panic
    /// itself, and the buffer stays readable.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn storage_handed_out_is_written_in_full_or_abandoned() {
        let mut auto = BPrint::new(0, BPrint::SIZE_AUTOMATIC).unwrap();
        let mut limited = BPrint::new(0, 2500).unwrap();
        let mut out = [0_u8; 64];
        let mut on = BPrint::for_buffer(&mut out).unwrap();
        for mut m in [auto.as_mut(), limited.as_mut(), on.as_mut()] {
            let size_max = m.as_ref().size_max();
            while m.as_ref().is_complete() {
                let before = m.as_ref().len();
                let n = m.append_with(700, |spare| {
                    assert!(spare.iter().all(|&b| b == 0));
                    spare.fill(b'#');
                    spare.len()
                });
                check(m.as_ref());
                if n == 0 {
                    break;
                }
                assert_eq!(m.as_ref().len() as usize, before as usize + n);
            }
            // Full to the last byte: nothing more is handed out.
            assert_eq!(m.as_ref().len(), size_max - 1);
            assert_eq!(m.append_with(1, |spare| spare.len()), 0);
            m.append_repeated(b'!', 1);
            assert!(!m.as_ref().is_complete());
            assert_eq!(m.append_with(1, |spare| spare.len()), 0);

            m.truncate(3);
            assert_eq!(m.append_with(4, |_| 0), 0);
            let refused = catch_unwind(AssertUnwindSafe(|| m.append_with(4, |s| s.len() + 1)));
            assert!(refused.is_err());
            let panicked = catch_unwind(AssertUnwindSafe(|| {
                m.append_with(4, |s| {
                    s.fill(b'?');
                    panic!("abandoned")
                })
            }));
            assert!(panicked.is_err());
            assert_eq!(m.as_ref().as_c_str(), c"###");
            check(m.as_ref());
            // A reborrow appends through the same buffer.
            m.as_mut().append_with(2, |s| {
                s.copy_from_slice(b"ok");
                2
            });
            assert_eq!(m.as_ref().as_bytes(), b"###ok");
        }
        drop(on);
        assert_eq!(&out[..6], b"###ok\0");
    }

    /// Owners append on the threads they move to, exclusive handles on
    /// scoped threads, and times are formatted from many threads at once.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn appenders_run_on_other_threads() {
        let tm = some_time();
        let joins: Vec<_> = SIZE_MAX
            .iter()
            .map(|&size_max| {
                let mut buf = BPrint::new(0, size_max).unwrap();
                thread::spawn(move || {
                    append_all(&mut buf.as_mut(), &some_time());
                    check(buf.as_ref());
                    buf
                })
            })
            .collect();
        let mut boxes: Vec<_> = joins.into_iter().map(|j| j.join().unwrap()).collect();
        thread::scope(|s| {
            for b in &mut boxes {
                let mut m = b.as_mut();
                let tm = &tm;
                s.spawn(move || {
                    m.clear();
                    for _ in 0..20 {
                        m.append_strftime(c"%F %T %Z ", tm.as_ref());
                    }
                    check(m.as_ref());
                });
            }
        });
        thread::scope(|s| {
            for b in &boxes {
                let r = b.as_ref();
                let t = tm.as_ref();
                s.spawn(move || {
                    check(r);
                    assert_eq!(t.zone(), Some(c"NDT"));
                });
            }
        });
        let copies: Vec<Tm> = (0..4).map(|_| tm).collect();
        let joins: Vec<_> = copies
            .into_iter()
            .map(|mut t| {
                thread::spawn(move || {
                    t.as_mut().set_zone(Some(c"UTC"));
                    let mut buf = BPrint::new(0, BPrint::SIZE_UNLIMITED).unwrap();
                    buf.as_mut().append_strftime(c"%Z", t.as_ref());
                    buf.as_ref().as_bytes().to_vec()
                })
            })
            .collect();
        for j in joins {
            assert_eq!(j.join().unwrap(), b"UTC");
        }
    }
}
