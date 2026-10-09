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
