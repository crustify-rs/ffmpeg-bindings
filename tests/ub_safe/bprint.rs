//! UB tests of `libavutil/bprint.h`'s safe [`PaddedStruct`] constants: they
//! evaluate at compile time, without overflow, and describe the C structure's
//! storage.

use core::mem::{align_of, size_of};

use ffmpeg::libavutil::bprint::PaddedStruct;
use ffmpeg::libavutil::ffi;

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

    use ffmpeg::libavutil::bprint::{BPrintFields, BPrintFieldsMut, PaddedStruct};
    use ffmpeg::libavutil::ffi;

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
        #[allow(clippy::clone_on_copy)] // the `Clone` impl itself
        let c = a.clone();
        b.as_mut().set_len(4);
        b.as_mut().set_str(Some(c"other"));
        assert_eq!((a.as_ref().len(), b.as_ref().len(), c.as_ref().len()), (3, 4, 3));
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

    use ffmpeg::libavutil::bprint::{BPrint, BPrintBox, BPrintOnBuffer, BPrintRef};

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
                let grows =
                    size_init > inline && inline != max && size_init.min(max) <= i32::MAX as u32;
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

        use ffmpeg::libavutil::mem::set_max_alloc;
        for limit in [0, 1, 1023] {
            set_max_alloc(limit);
            assert!(BPrint::new(0, BPrint::SIZE_AUTOMATIC).is_none());
            assert!(BPrint::for_buffer(&mut []).is_none());
            let mut out = [0xa5; 8];
            assert!(BPrint::for_buffer(&mut out).is_none());
            assert_eq!(out, [0xa5; 8]);
        }

        // At exactly the descriptor size construction succeeds; failure to
        // allocate the requested string leaves valid inline storage.
        set_max_alloc(1024);
        let buf = BPrint::new(4096, BPrint::SIZE_UNLIMITED).unwrap();
        assert!(buf.as_ref().is_inline());
        check_empty(buf.as_ref());
        drop(buf);
        let mut out = [0xa5; 8];
        drop(BPrint::for_buffer(&mut out).unwrap());
        assert_eq!(out, [0, 0xa5, 0xa5, 0xa5, 0xa5, 0xa5, 0xa5, 0xa5]);
    }

    #[test]
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
    fn caller_buffers_of_every_length_hold_an_empty_string() {
        for len in [0, 1, 2, 3, 1024, 70_000] {
            let mut out = vec![0xa5_u8; len];
            let mut buf = BPrint::for_buffer(&mut out).unwrap();
            check_empty(buf.as_ref());
            assert_eq!(buf.as_ref().size() as usize, len);
            assert_eq!(buf.as_ref().is_inline(), len == 0);
            buf.as_mut().truncate(0);
            drop(buf);
            if len > 0 {
                assert_eq!(out[0], 0);
                // One slice comparison: BSan retags per element visited.
                assert_eq!(out[1..], vec![0xa5; len - 1][..]);
            }
        }
    }

    /// A buffer past `u32::MAX` bytes is used up to that many. Zeroed pages
    /// cost nothing until touched; skipped under BSan, whose shadow does not
    /// scale to them.
    #[test]
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

        let mut out = [0xa5_u8; 32];
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
