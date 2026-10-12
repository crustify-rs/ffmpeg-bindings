//! UB tests for `libavutil/intreadwrite.h`'s `unaligned_N` and `av_alias*`
//! values, through the safe API only. Tests named `heap_*` reach the C
//! allocator; the rest are FFI-free and run under Miri.
//!
//! The `unaligned_N` views are swept over every misaligned address up to the
//! end of exactly sized Rust and C buffers, as inline values embedded at odd
//! offsets, as `av_malloc`-family blocks, and across threads for the asserted
//! `Send` / `Sync`.
//!
//! The `av_alias*` values have every view read after every write, on
//! Rust-owned values, handles and reborrows, on C-allocated owners, and across
//! threads.

use std::thread;

use ffibox::CZeroable;
use ffmpeg::libavutil::intreadwrite::{
    Alias16, Alias16Mut, Alias16Ref, Alias32, Alias32Mut, Alias32Ref, Alias64, Alias64Mut,
    Alias64Ref, Unaligned16, Unaligned16Mut, Unaligned16Ref, Unaligned32, Unaligned32Mut,
    Unaligned32Ref, Unaligned64, Unaligned64Mut, Unaligned64Ref,
};
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
#[cfg_attr(miri, ignore = "calls C")]
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
#[cfg_attr(miri, ignore = "calls C")]
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
#[cfg_attr(miri, ignore = "calls C")]
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

/// Every write of an `Alias16`, each followed by a read of every view.
fn exercise16(mut m: Alias16Mut<'_>) {
    m.set_u16(0xbeef);
    let r = m.as_ref();
    assert_eq!((r.u16(), r.u8x2()), (0xbeef, 0xbeefu16.to_ne_bytes()));
    m.set_u8x2([0xff, 0x00]);
    let r = m.as_ref();
    assert_eq!(r.u16(), u16::from_ne_bytes([0xff, 0x00]));
    assert_eq!(r.u8x2(), [0xff, 0x00]);
}

/// Every write of an `Alias32`, each followed by a read of every view.
fn exercise32(mut m: Alias32Mut<'_>) {
    fn views(m: &Alias32Mut<'_>) -> (u32, [u16; 2], [u8; 4], u32) {
        let r = m.as_ref();
        (r.u32(), r.u16x2(), r.u8x4(), r.f32().to_bits())
    }
    m.set_u32(u32::MAX);
    let v = views(&m);
    assert_eq!(v, (u32::MAX, [u16::MAX; 2], [u8::MAX; 4], u32::MAX));
    m.set_u16x2([0x0102, 0x0304]);
    assert_eq!(views(&m).1, [0x0102, 0x0304]);
    m.set_u8x4([9, 8, 7, 6]);
    assert_eq!(views(&m).0, u32::from_ne_bytes([9, 8, 7, 6]));
    // A signalling NaN, a subnormal and an infinity keep their bits.
    for bits in [0x7f80_0001, 0x0000_0001, 0xff80_0000] {
        m.set_f32(f32::from_bits(bits));
        assert_eq!(views(&m).0, bits);
        assert_eq!(views(&m).3, bits);
    }
}

#[test]
fn values_read_every_view_after_every_write() {
    let mut a = Alias16::zeroed();
    exercise16(a.as_mut());
    let mut b = Alias32::default();
    exercise32(b.as_mut());
}

#[test]
fn reborrows_hand_back_the_exclusive_handle() {
    let mut a = Alias32::zeroed();
    let mut m = a.as_mut();
    exercise32(m.as_mut());
    m.set_u32(3);
    let shared = m.as_ref();
    let copy = shared;
    assert_eq!(shared.u32(), copy.u32());
    assert_eq!(a.as_ref().u32(), 3);

    let mut c = Alias16::from_u16(1);
    let mut n = c.as_mut();
    exercise16(n.as_mut());
    n.set_u16(4);
    assert_eq!(c.as_ref().u16(), 4);
}

#[test]
fn constructed_values_are_fully_initialised() {
    // Each constructor writes one view; every other view reads all the bytes.
    for a in [
        Alias32::from_u32(1),
        Alias32::from_u16x2([1, 2]),
        Alias32::from_u8x4([1, 2, 3, 4]),
        Alias32::from_f32(-0.0),
    ] {
        let r = a.as_ref();
        let _ = (r.u32(), r.u16x2(), r.u8x4(), r.f32());
    }
    for a in [Alias16::from_u16(1), Alias16::from_u8x2([1, 2])] {
        let r = a.as_ref();
        let _ = (r.u16(), r.u8x2());
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn heap_boxes_read_and_write_c_storage() {
    let mut a = HeapBox::<Alias16>::new_zeroed().unwrap();
    assert_eq!(a.as_ref().u16(), 0);
    exercise16(a.as_mut());
    let mut b = HeapBox::<Alias32>::new_zeroed().unwrap();
    assert_eq!(b.as_ref().u32(), 0);
    exercise32(b.as_mut());

    // `av_memdup` copies: the clone is independent and both free once.
    b.as_mut().set_u32(5);
    let mut c = b.clone();
    c.as_mut().set_u32(6);
    assert_eq!((b.as_ref().u32(), c.as_ref().u32()), (5, 6));
    drop(b);
    assert_eq!(c.as_ref().u32(), 6);
    let d = a.clone();
    drop(a);
    assert_eq!(d.as_ref().u8x2(), [0xff, 0x00]);
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn heap_arrays_reach_each_element_through_handles() {
    let mut v = alloc_cell_array_zeroed::<Alias32>(3).unwrap();
    for (i, mut m) in v.as_handles_mut().iter_mut().enumerate() {
        assert_eq!(m.as_ref().u32(), 0);
        m.set_u32(i as u32 + 10);
    }
    let h = v.as_handles();
    assert_eq!(h.get(2).unwrap().u32(), 12);
    assert!(h.get(3).is_none());
    exercise32(v.as_handles_mut().into_mut(1).unwrap());

    let src = [Alias16::from_u16(1), Alias16::from_u16(2)];
    let w = HeapVec::from_slice(&src).unwrap();
    let back: Vec<u16> = w.as_handles().iter().map(|r| r.u16()).collect();
    assert_eq!(back, [1, 2]);
    let w2 = w.clone();
    drop(w);
    assert_eq!(w2.as_handles().get(1).unwrap().u16(), 2);

    let empty = alloc_cell_array_zeroed::<Alias16>(0).unwrap();
    assert!(empty.as_handles().get(0).is_none());
}

#[test]
fn alias_values_cross_threads() {
    let mut a = Alias32::from_u32(1);
    let mut b = Alias16::zeroed();
    let shared = Alias32::from_f32(0.5);
    std::thread::scope(|s| {
        let (ma, mb) = (a.as_mut(), b.as_mut());
        s.spawn(move || exercise32(ma));
        s.spawn(move || exercise16(mb));
        for _ in 0..2 {
            let r = shared.as_ref();
            s.spawn(move || assert_eq!(r.f32(), 0.5));
            s.spawn(|| assert_eq!(shared, Alias32::from_f32(0.5)));
        }
    });
    let moved = std::thread::spawn(move || a).join().unwrap();
    assert_eq!(moved.as_ref().f32().to_bits(), 0xff80_0000);
    assert_eq!(b.as_ref().u8x2(), [0xff, 0x00]);
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn heap_owners_move_across_threads() {
    let a = Alias32::from_u32(7);
    let b = HeapBox::<Alias16>::new_zeroed().unwrap();
    let (a, b) = std::thread::spawn(move || {
        let mut b = b;
        b.as_mut().set_u16(a.as_ref().u32() as u16);
        (a, b)
    })
    .join()
    .unwrap();
    assert_eq!((a.as_ref().u32(), b.as_ref().u16()), (7, 7));
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn heap_shared_handles_read_concurrently() {
    let mut boxed = HeapBox::<Alias32>::new_zeroed().unwrap();
    boxed.as_mut().set_f32(2.5);
    let value = Alias16::from_u16(9);
    let r32 = boxed.as_ref();
    std::thread::scope(|s| {
        for _ in 0..4 {
            s.spawn(|| {
                for _ in 0..100 {
                    assert_eq!(r32.f32(), 2.5);
                    assert_eq!(value.as_ref().u16(), 9);
                    assert_eq!(value, Alias16::from_u16(9));
                }
            });
        }
    });
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn heap_exclusive_handles_write_on_another_thread() {
    let mut a = Alias32::zeroed();
    let mut b = HeapBox::<Alias16>::new_zeroed().unwrap();
    {
        let ma = a.as_mut();
        let mb = b.as_mut();
        std::thread::scope(|s| {
            s.spawn(move || exercise32(ma));
            s.spawn(move || exercise16(mb));
        });
    }
    let mut parts = alloc_cell_array_zeroed::<Alias32>(2).unwrap();
    {
        let mut handles = parts.as_handles_mut();
        let (lo, hi) = handles.split_at_mut(1).unwrap();
        std::thread::scope(|s| {
            s.spawn(move || lo.into_mut(0).unwrap().set_u32(1));
            s.spawn(move || hi.into_mut(0).unwrap().set_u32(2));
        });
    }
    let h = parts.as_handles();
    assert_eq!((h.get(0).unwrap().u32(), h.get(1).unwrap().u32()), (1, 2));
    assert_eq!(a.as_ref().f32().to_bits(), 0xff80_0000);
    assert_eq!(b.as_ref().u8x2(), [0xff, 0x00]);
}

// `Unaligned16` and `Unaligned32`.

/// Writes then reads back through views at every offset of `buf`, the last
/// one ending on its final byte.
fn sweep(buf: &mut [u8]) {
    for off in 0..=buf.len() - 2 {
        let v = 0xa000 | off as u16;
        Unaligned16Mut::from_bytes(buf[off..].first_chunk_mut().unwrap()).set_value(v);
        assert_eq!(
            Unaligned16Ref::from_bytes(buf[off..].first_chunk().unwrap()).value(),
            v
        );
    }
    for off in 0..=buf.len() - 4 {
        let v = 0xb000_0000 | off as u32;
        Unaligned32Mut::from_bytes(buf[off..].first_chunk_mut().unwrap()).set_value(v);
        assert_eq!(
            Unaligned32Ref::from_bytes(buf[off..].first_chunk().unwrap()).value(),
            v
        );
    }
}

#[test]
fn byte_views_stay_in_bounds_of_an_exact_rust_buffer() {
    for len in [4, 5, 7, 8, 13] {
        let mut buf = vec![0u8; len].into_boxed_slice();
        sweep(&mut buf);
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn heap_byte_views_stay_in_bounds_of_an_exact_c_buffer() {
    for len in [4, 5, 7, 8, 13] {
        let mut buf = alloc_array_zeroed::<u8>(len).unwrap();
        sweep(buf.as_mut_slice());
    }
}

#[test]
fn byte_views_cover_exactly_their_array() {
    // Views over the two bytes, or four, that end a 4-byte allocation: the
    // array type fixes the width, so no read or write runs past it.
    let mut buf = vec![0u8; 4].into_boxed_slice();
    let tail = buf.last_chunk_mut::<2>().unwrap();
    Unaligned16Mut::from_bytes(tail).set_value(u16::MAX);
    assert_eq!(
        Unaligned16Ref::from_bytes(buf.last_chunk().unwrap()).value(),
        u16::MAX
    );
    assert_eq!(buf[..2], [0, 0]);
    let whole = buf.first_chunk_mut::<4>().unwrap();
    Unaligned32Mut::from_bytes(whole).set_value(0);
    assert!(buf.iter().all(|&b| b == 0));
}

#[test]
fn inline_values_at_odd_offsets_read_and_write_unaligned() {
    // Align 1, so each field lands on an odd address for its integer.
    #[derive(Default)]
    struct Packed {
        _pad: u8,
        a: Unaligned32,
        _pad2: u8,
        b: Unaligned16,
        c: [Unaligned32; 3],
    }
    let mut p = Box::<Packed>::default();
    p.a.as_mut().set_value(u32::MAX);
    p.b.as_mut().set_value(0x1234);
    for (i, c) in p.c.iter_mut().enumerate() {
        c.as_mut().set_value(i as u32 + 1);
    }
    assert_eq!(p.a.as_ref().value(), u32::MAX);
    assert_eq!(p.b.as_ref().value(), 0x1234);
    assert_eq!(p.c.map(u32::from), [1, 2, 3]);
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn heap_boxes_allocate_clone_and_free() {
    let mut a = HeapBox::<Unaligned16>::new_zeroed().unwrap();
    assert_eq!(a.as_ref().value(), 0);
    a.as_mut().set_value(0xbeef);
    let b = a.clone();
    a.as_mut().set_value(1);
    assert_eq!(b.as_ref().value(), 0xbeef);
    assert_eq!(a.as_ref().value(), 1);

    let mut c = HeapBox::<Unaligned32>::new_zeroed().unwrap();
    c.as_mut().set_value(u32::MAX);
    let d = c.clone();
    drop(c);
    assert_eq!(d.as_ref().value(), u32::MAX);
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn heap_arrays_hand_out_handles_in_bounds() {
    let mut a = alloc_cell_array_zeroed::<Unaligned32>(5).unwrap();
    let mut h = a.as_handles_mut();
    for i in 0..5 {
        h.get_mut(i).unwrap().set_value(i as u32 * 3);
    }
    assert!(h.get_mut(5).is_none());
    let s = a.as_handles();
    assert_eq!(
        s.iter().map(|r| r.value()).collect::<Vec<_>>(),
        [0, 3, 6, 9, 12]
    );
    assert!(s.get(5).is_none());

    let b = alloc_cell_array_zeroed::<Unaligned16>(3).unwrap();
    assert!(b.as_handles().iter().all(|r| r.value() == 0));
}

#[test]
fn shared_views_read_concurrently() {
    let bytes = [0x11u8, 0x22, 0x33, 0x44, 0x55];
    let r32 = Unaligned32Ref::from_bytes(bytes[1..].first_chunk().unwrap());
    let r16 = Unaligned16Ref::from_bytes(bytes[3..].first_chunk().unwrap());
    let want = (r32.value(), r16.value());
    let value = Unaligned32::new(9);
    thread::scope(|s| {
        let readers: Vec<_> = (0..4)
            .map(|_| s.spawn(move || (0..100).all(|_| (r32.value(), r16.value()) == want)))
            .collect();
        for t in readers {
            assert!(t.join().unwrap());
        }
        // `&Unaligned32Ref` and a shared `&Unaligned32` cross too.
        let (rr, vr) = (&r32, &value);
        s.spawn(move || assert_eq!((rr.value(), vr.as_ref().value()), (want.0, 9)));
    });
}

#[test]
fn exclusive_views_write_disjoint_chunks_on_other_threads() {
    let mut buf = vec![0u8; 4 * 8 + 1].into_boxed_slice();
    thread::scope(|s| {
        // Offset 1: every chunk is misaligned for `u32`.
        for (i, chunk) in buf[1..].as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let mut m = Unaligned32Mut::from_bytes(chunk);
            s.spawn(move || {
                for k in 0..100 {
                    m.set_value(i as u32 * 1000 + k);
                }
                // A shared `&Unaligned32Mut` reaches the getter from more
                // threads, once the writes are done.
                let shared = &m;
                thread::scope(|s| {
                    for _ in 0..2 {
                        s.spawn(move || assert_eq!(shared.as_ref().value(), i as u32 * 1000 + 99));
                    }
                });
            });
        }
    });
    for (i, chunk) in buf[1..].as_chunks::<4>().0.iter().enumerate() {
        assert_eq!(
            Unaligned32Ref::from_bytes(chunk).value(),
            i as u32 * 1000 + 99
        );
    }
    assert_eq!(buf[0], 0);
}

#[test]
fn values_and_their_handles_move_between_threads() {
    let v = Unaligned16::new(7);
    let back = thread::spawn(move || {
        let mut v = v;
        v.as_mut().set_value(8);
        v
    })
    .join()
    .unwrap();
    assert_eq!(back.as_ref().value(), 8);

    let mut w = Unaligned32::new(0);
    let mut wm = w.as_mut();
    thread::scope(|s| {
        s.spawn(|| wm.set_value(5));
    });
    assert_eq!(w.as_ref().value(), 5);

    let mut x = Unaligned16::new(0);
    thread::scope(|s| {
        let m = x.as_mut();
        s.spawn(move || {
            let mut m = m;
            m.set_value(3);
        });
    });
    assert_eq!(x.as_ref().value(), 3);
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn heap_boxes_cross_threads() {
    let mut b = HeapBox::<Unaligned32>::new_zeroed().unwrap();
    b.as_mut().set_value(42);
    let b = thread::spawn(move || {
        let c = b.clone();
        drop(b);
        c
    })
    .join()
    .unwrap();
    thread::scope(|s| {
        let r = b.as_ref();
        s.spawn(move || assert_eq!(r.value(), 42));
        s.spawn(move || assert_eq!(r.value(), 42));
    });
}

/// Write every member in turn through `m`, reading every member back after
/// each write.
fn cycle_alias64_members(mut m: Alias64Mut<'_>, seed: u64) {
    fn read_all(r: Alias64Ref<'_>) -> u64 {
        let _ = (r.u32x2(), r.u16x4(), r.u8x8(), r.f64(), r.f32x2());
        r.u64()
    }
    m.set_u64(seed);
    assert_eq!(read_all(m.as_ref()), seed);
    m.set_u32x2([seed as u32, (seed >> 32) as u32]);
    m.set_u16x4([1, 2, 3, 4]);
    assert_eq!(m.as_ref().u16x4(), [1, 2, 3, 4]);
    m.set_u8x8(seed.to_ne_bytes());
    assert_eq!(read_all(m.as_ref()), seed);
    m.set_f64(f64::from_bits(seed));
    m.set_f32x2([f32::from_bits(seed as u32), f32::NAN]);
    m.set_u8x8(Alias64::from_u64(seed).as_ref().u8x8());
    assert_eq!(read_all(m.as_ref()), seed);
}

#[test]
fn alias64_value_takes_every_member() {
    let mut a = Alias64::default();
    cycle_alias64_members(a.as_mut(), 0x0102_0304_0506_0708);
    // Reborrows of one exclusive handle, then a shared one.
    let mut m = a.as_mut();
    cycle_alias64_members(m.as_mut(), u64::MAX);
    cycle_alias64_members(m.as_mut(), 0);
    assert_eq!(a.as_ref().u64(), 0);
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn heap_alias64_box_takes_every_member_and_clones() {
    let mut b = HeapBox::<Alias64>::new_zeroed().unwrap();
    assert_eq!(b.as_ref().u8x8(), [0; 8]);
    cycle_alias64_members(b.as_mut(), 0xdead_beef_cafe_f00d);
    let c = b.clone();
    b.as_mut().set_u64(1);
    drop(b);
    assert_eq!(c.as_ref().u64(), 0xdead_beef_cafe_f00d);

    drop(HeapBox::<Alias64>::new_uninit().unwrap());
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn heap_alias64_array_elements_stay_in_bounds() {
    let mut v = alloc_cell_array_zeroed::<Alias64>(5).unwrap();
    {
        let mut h = v.as_handles_mut();
        for i in 0..5 {
            cycle_alias64_members(h.get_mut(i).unwrap(), i as u64 * 0x1111);
        }
        assert!(h.get_mut(5).is_none());
    }
    let r = v.as_handles();
    assert!(r.get(5).is_none());
    let sums: u64 = r.iter().map(|e| e.u64()).sum();
    assert_eq!(sums, 0x1111 * 10);
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn heap_alias64_handles_read_concurrently() {
    let a = Alias64::from_u64(42);
    let b = HeapBox::<Alias64>::new_zeroed().unwrap();
    let (ra, rb) = (a.as_ref(), b.as_ref());
    thread::scope(|s| {
        for _ in 0..4 {
            s.spawn(move || {
                for _ in 0..100 {
                    assert_eq!(ra.u64(), 42);
                    assert_eq!(rb.u64(), 0);
                }
            });
            s.spawn(|| assert_eq!(a.as_ref().f64().to_bits(), 42));
        }
    });
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn heap_alias64_handles_and_owners_move_across_threads() {
    let mut a = Alias64::default();
    let mut b = HeapBox::<Alias64>::new_zeroed().unwrap();
    let (ma, mb) = (a.as_mut(), b.as_mut());
    thread::scope(|s| {
        s.spawn(move || cycle_alias64_members(ma, 7));
        s.spawn(move || cycle_alias64_members(mb, 9));
    });
    assert_eq!((a.as_ref().u64(), b.as_ref().u64()), (7, 9));

    // The value and its owner move, and the box is freed on another thread.
    let t = thread::spawn(move || (a.as_ref().u64(), b));
    let (x, b) = t.join().unwrap();
    assert_eq!((x, b.as_ref().u64()), (7, 9));
    thread::spawn(move || drop(b)).join().unwrap();
}

// `Alias16` and `Alias32` views over caller bytes (`from_bytes`).

/// A buffer whose first byte is 8-aligned, so every residue of an `Alias16`
/// and `Alias32` address occurs at a known offset.
#[derive(Clone, Copy)]
#[repr(C, align(8))]
struct Aligned<const N: usize>([u8; N]);

/// Writes through an exclusive view at every aligned offset of `buf` (the
/// last one ending on its final byte), reads back through a shared view, and
/// has every misaligned offset refused.
fn sweep_alias_views(buf: &mut [u8]) {
    let len = buf.len();
    for off in 0..=len - 2 {
        let v = 0x0102u16.wrapping_mul(off as u16 + 1);
        match Alias16Mut::from_bytes((&mut buf[off..off + 2]).try_into().unwrap()) {
            Some(mut m) => {
                assert_eq!(off % 2, 0);
                m.set_u16(v);
                assert_eq!(m.as_ref().u8x2(), v.to_ne_bytes());
            }
            None => assert_eq!(off % 2, 1),
        }
        let r = Alias16Ref::from_bytes(buf[off..off + 2].try_into().unwrap());
        assert_eq!(r.map(|r| r.u16()), (off % 2 == 0).then_some(v));
    }
    for off in 0..=len - 4 {
        let v = 0x0a0b_0c0du32.rotate_left(off as u32 * 8);
        match Alias32Mut::from_bytes((&mut buf[off..off + 4]).try_into().unwrap()) {
            Some(mut m) => {
                assert_eq!(off % 4, 0);
                m.set_u32(v);
                m.set_f32(m.as_ref().f32());
                assert_eq!(m.as_ref().u8x4(), v.to_ne_bytes());
            }
            None => assert_ne!(off % 4, 0),
        }
        let r = Alias32Ref::from_bytes(buf[off..off + 4].try_into().unwrap());
        assert_eq!(r.map(|r| r.u32()), (off % 4 == 0).then_some(v));
    }
}

#[test]
fn alias_byte_views_stay_in_bounds_of_exact_rust_buffers() {
    sweep_alias_views(&mut Aligned([0u8; 4]).0);
    sweep_alias_views(&mut Aligned([0u8; 7]).0);
    sweep_alias_views(&mut Aligned([0u8; 24]).0);
    // A view at the very end of the buffer touches only its own bytes.
    let mut tail = Aligned([0xa5u8; 8]);
    Alias16Mut::from_bytes(tail.0[6..].first_chunk_mut().unwrap())
        .unwrap()
        .set_u16(0);
    assert_eq!(tail.0, [0xa5, 0xa5, 0xa5, 0xa5, 0xa5, 0xa5, 0, 0]);
    Alias32Mut::from_bytes(tail.0[..4].first_chunk_mut().unwrap())
        .unwrap()
        .set_u32(u32::MAX);
    assert_eq!(tail.0, [0xff, 0xff, 0xff, 0xff, 0xa5, 0xa5, 0, 0]);
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn heap_alias_byte_views_stay_in_bounds_of_exact_c_buffers() {
    for len in [4usize, 9, 32] {
        let mut bytes = alloc_array_zeroed::<u8>(len).unwrap();
        sweep_alias_views(bytes.as_mut_slice());
        let first = Alias32Ref::from_bytes(bytes.as_slice()[..4].try_into().unwrap()).unwrap();
        assert_eq!(first.u8x4(), bytes.as_slice()[..4]);
    }
}

#[test]
fn alias_views_overlap_consistently() {
    let mut buf = Aligned([0u8; 4]);
    Alias32Mut::from_bytes(&mut buf.0).unwrap().set_u16x2([0x1122, 0x3344]);
    let lo = Alias16Ref::from_bytes(buf.0[..2].try_into().unwrap()).unwrap();
    let hi = Alias16Ref::from_bytes(buf.0[2..].try_into().unwrap()).unwrap();
    let whole = Alias32Ref::from_bytes(&buf.0).unwrap();
    assert_eq!([lo.u16(), hi.u16()], whole.u16x2());
    let copy = whole; // the shared view is `Copy`
    assert_eq!(copy.u8x4(), [lo.u8x2(), hi.u8x2()].concat()[..]);
    assert_eq!(Alias32::from_u8x4(copy.u8x4()), Alias32::from_u16x2(whole.u16x2()));
}

#[test]
fn alias_view_reborrows_hand_back_the_exclusive_handle() {
    let mut buf = Aligned([0u8; 8]);
    let mut m = Alias32Mut::from_bytes(buf.0[4..].first_chunk_mut().unwrap()).unwrap();
    exercise32(m.as_mut());
    m.set_u32(3);
    assert_eq!(m.as_ref().u32(), 3);
    let mut n = Alias16Mut::from_bytes(buf.0[..2].first_chunk_mut().unwrap()).unwrap();
    exercise16(n.as_mut());
    n.set_u8x2([1, 2]);
    assert_eq!(buf.0[..2], [1, 2]);
    assert_eq!(buf.0[4..], 3u32.to_ne_bytes());
}

#[test]
fn alias_views_cross_threads() {
    let mut buf = Aligned([0u8; 16]);
    {
        let (lo, hi) = buf.0.split_at_mut(8);
        let a = Alias32Mut::from_bytes((&mut lo[..4]).try_into().unwrap()).unwrap();
        let b = Alias16Mut::from_bytes((&mut hi[..2]).try_into().unwrap()).unwrap();
        thread::scope(|s| {
            s.spawn(move || exercise32(a));
            s.spawn(move || exercise16(b));
        });
    }
    let r32 = Alias32Ref::from_bytes(buf.0[..4].try_into().unwrap()).unwrap();
    let r16 = Alias16Ref::from_bytes(buf.0[8..10].try_into().unwrap()).unwrap();
    thread::scope(|s| {
        for _ in 0..4 {
            s.spawn(move || {
                for _ in 0..100 {
                    assert_eq!(r32.f32().to_bits(), 0xff80_0000);
                    assert_eq!(r16.u8x2(), [0xff, 0x00]);
                }
            });
            s.spawn(|| assert_eq!(r32.u32(), 0xff80_0000));
        }
    });
    let moved = thread::spawn(move || buf).join().unwrap();
    assert_eq!(moved.0[..4], 0xff80_0000u32.to_ne_bytes());
}
