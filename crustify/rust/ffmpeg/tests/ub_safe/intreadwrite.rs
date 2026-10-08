//! UB tests for `libavutil/intreadwrite.h`'s `Unaligned64`, through the safe
//! API only. Tests named `heap_*` reach the C allocator; the rest are FFI-free
//! and run under Miri.

use ffibox::CZeroable;
use ffmpeg::libavutil::intreadwrite::{Unaligned64, Unaligned64Mut, Unaligned64Ref};
use ffmpeg::libavutil::mem::{HeapBox, HeapVec, alloc_array_zeroed, alloc_cell_array_zeroed};

/// The 8-byte window of `buf` at `off`, which may be at any alignment.
fn window(buf: &[u8], off: usize) -> Unaligned64Ref<'_> {
    Unaligned64Ref::from_bytes(buf[off..off + 8].try_into().unwrap())
}

/// The exclusive 8-byte window of `buf` at `off`.
fn window_mut(buf: &mut [u8], off: usize) -> Unaligned64Mut<'_> {
    Unaligned64Mut::from_bytes((&mut buf[off..off + 8]).try_into().unwrap())
}

#[test]
fn values_are_built_copied_and_rewritten() {
    let mut a = Unaligned64::new(u64::MAX);
    let b = a;
    a.as_mut().set_value(1);
    assert_eq!((a.as_ref().value(), b.as_ref().value()), (1, u64::MAX));
    assert_eq!(Unaligned64::zeroed().as_ref().value(), 0);
    assert_eq!(Unaligned64::from(42).as_ref().value(), 42);
}

#[test]
fn windows_at_every_alignment_stay_in_bounds() {
    // Odd lengths put the last window at every residue of a word.
    for len in 8..=24 {
        let mut buf = vec![0u8; len];
        for off in 0..=len - 8 {
            window_mut(&mut buf, off).set_value(off as u64 + 1);
            assert_eq!(window(&buf, off).value(), off as u64 + 1);
        }
        // The last write covers the last 8 bytes exactly.
        assert_eq!(buf[len - 8..], (len as u64 - 7).to_ne_bytes());
    }
}

#[test]
fn overlapping_windows_see_each_others_writes() {
    let mut buf = [0u8; 9];
    window_mut(&mut buf, 0).set_value(u64::from_ne_bytes([1; 8]));
    window_mut(&mut buf, 1).set_value(u64::from_ne_bytes([2; 8]));
    assert_eq!(buf, [1, 2, 2, 2, 2, 2, 2, 2, 2]);
    assert_eq!(
        window(&buf, 0).value(),
        u64::from_ne_bytes([1, 2, 2, 2, 2, 2, 2, 2])
    );
}

#[test]
fn exclusive_handle_reborrows_and_reads() {
    fn bump(mut m: Unaligned64Mut<'_>) {
        let v = m.as_ref().value();
        m.set_value(v + 1);
    }
    let mut v = Unaligned64::new(0);
    let mut m = v.as_mut();
    bump(m.as_mut());
    bump(m.as_mut());
    let shared = m.as_ref();
    assert_eq!((shared.value(), shared.value()), (2, 2));
    m.set_value(10);
    assert_eq!(v.as_ref().value(), 10);

    let mut bytes = [0u8; 8];
    let mut m = Unaligned64Mut::from_bytes(&mut bytes);
    bump(m.as_mut());
    assert_eq!(m.as_ref().value(), 1);
    assert_eq!(bytes, 1u64.to_ne_bytes());
}

#[test]
fn shared_handles_are_copied_and_outlive_nothing() {
    let buf = *b"0123456789";
    let a = window(&buf, 1);
    let b = a;
    let c = Unaligned64Ref::from_bytes(&[7; 8]);
    assert_eq!(a.value(), b.value());
    assert_eq!(a.value(), u64::from_ne_bytes(*b"12345678"));
    assert_eq!(c.value(), u64::from_ne_bytes([7; 8]));
}

#[test]
fn values_and_handles_cross_threads() {
    let v = Unaligned64::new(5);
    let r = std::thread::spawn(move || v.as_ref().value())
        .join()
        .unwrap();
    assert_eq!(r, 5);

    let buf: Vec<u8> = (0..64).collect();
    std::thread::scope(|s| {
        for t in 0..4 {
            let shared = window(&buf, t * 3 + 1);
            let buf = &buf;
            s.spawn(move || {
                assert_eq!(shared.value(), window(buf, t * 3 + 1).value());
            });
        }
    });

    let mut out = [[0u8; 9]; 4];
    std::thread::scope(|s| {
        for (k, chunk) in out.iter_mut().enumerate() {
            let mut m = window_mut(chunk, 1);
            s.spawn(move || m.set_value(k as u64 * 0x0101_0101_0101_0101));
        }
    });
    for (k, chunk) in out.iter().enumerate() {
        assert_eq!(chunk[0], 0);
        assert_eq!(window(chunk, 1).value(), k as u64 * 0x0101_0101_0101_0101);
    }
}

#[test]
fn shared_value_is_read_concurrently() {
    let v = Unaligned64::new(0xdead_beef);
    std::thread::scope(|s| {
        for _ in 0..4 {
            let r = v.as_ref();
            s.spawn(move || assert_eq!(r.value(), 0xdead_beef));
            s.spawn(|| assert_eq!(v.as_ref().value(), 0xdead_beef));
        }
    });
}

#[test]
fn heap_windows_over_c_memory_stay_in_bounds() {
    // An `av_malloc` block, so ASan bounds-checks C's allocation itself.
    let mut a = alloc_array_zeroed::<u8>(17).unwrap();
    for off in 0..=9 {
        window_mut(a.as_mut_slice(), off).set_value(u64::MAX - off as u64);
        assert_eq!(window(a.as_slice(), off).value(), u64::MAX - off as u64);
    }
    let a = HeapVec::from_slice(&a.as_slice()[1..]).unwrap();
    assert_eq!(window(a.as_slice(), 8).value(), u64::MAX - 9);
}

#[test]
fn heap_boxed_values_are_built_cloned_and_freed() {
    let mut a = HeapBox::<Unaligned64>::new_zeroed().unwrap();
    assert_eq!(a.as_ref().value(), 0);
    a.as_mut().set_value(3);
    let b = a.clone();
    a.as_mut().set_value(4);
    drop(a);
    assert_eq!(b.as_ref().value(), 3);
    let c = std::thread::spawn(move || b.try_clone().unwrap())
        .join()
        .unwrap();
    assert_eq!(c.as_ref().value(), 3);
}

#[test]
fn heap_value_arrays_are_written_through_handles() {
    let mut a = alloc_cell_array_zeroed::<Unaligned64>(5).unwrap();
    assert!(a.as_handles().iter().all(|v| v.value() == 0));
    for (i, mut v) in a.as_handles_mut().iter_mut().enumerate() {
        v.set_value(i as u64);
    }
    let read: Vec<u64> = a.as_handles().iter().map(|v| v.value()).collect();
    assert_eq!(read, [0, 1, 2, 3, 4]);
    assert!(alloc_cell_array_zeroed::<Unaligned64>(usize::MAX / 4).is_none());
}
