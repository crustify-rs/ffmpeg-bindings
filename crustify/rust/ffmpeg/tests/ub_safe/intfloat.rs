//! UB tests for `libavutil/intfloat.h`'s `IntFloat64` and `IntFloat32`,
//! through the safe API only: inline values, `av_malloc`-family owners and
//! arrays of them, every handle form, and the asserted `Send` / `Sync`.

use std::thread;

use ffibox::CZeroable;
use ffmpeg::libavutil::intfloat::{
    IntFloat32, IntFloat32Mut, IntFloat64, IntFloat64Mut, IntFloat64Ref, double_from_bits,
    double_to_bits, float_from_bits, float_to_bits,
};
use ffmpeg::libavutil::mem::{HeapBox, alloc_cell_array_zeroed};

/// Bit patterns at every class edge of a `double`: zeros, subnormals, the
/// finite extremes, infinities, and quiet and signalling NaNs with payloads.
const BITS: [u64; 12] = [
    0,
    1 << 63,
    1,
    0x000f_ffff_ffff_ffff,
    0x0010_0000_0000_0000,
    0x7fef_ffff_ffff_ffff,
    0x7ff0_0000_0000_0000,
    0xfff0_0000_0000_0000,
    0x7ff0_0000_0000_0001,
    0x7ff8_0000_0000_0000,
    0xfff8_dead_beef_0001,
    u64::MAX,
];

#[test]
fn every_bit_pattern_reads_through_both_views() {
    for bits in BITS {
        let v = IntFloat64::from_bits(bits);
        let r = v.as_ref();
        assert_eq!(r.bits(), bits);
        assert_eq!(r.float().to_bits(), bits);
        let w = IntFloat64::from_float(f64::from_bits(bits));
        assert_eq!(w.as_ref().bits(), bits);
    }
}

#[test]
fn shared_handles_copy_and_coexist() {
    let v = IntFloat64::from_float(0.25);
    let a = v.as_ref();
    let b = a;
    let c = v.as_ref();
    assert_eq!(
        (a.bits(), b.float(), c.float()),
        (0.25f64.to_bits(), 0.25, 0.25)
    );
}

fn reset(mut m: IntFloat64Mut<'_>) {
    m.set_bits(0);
}

fn read(r: IntFloat64Ref<'_>) -> u64 {
    r.bits()
}

#[test]
fn exclusive_handle_reborrows_without_aliasing() {
    let mut v = IntFloat64::from_bits(9);
    let mut m = v.as_mut();
    reset(m.as_mut());
    assert_eq!(read(m.as_ref()), 0);
    m.set_float(1.5);
    let shared = m.as_ref();
    assert_eq!(shared.float(), 1.5);
    m.set_bits(2);
    assert_eq!(read(v.as_ref()), 2);
}

#[test]
fn copies_are_independent_values() {
    let a = IntFloat64::from_bits(1);
    let mut b = a;
    b.as_mut().set_float(-1.0);
    assert_eq!(a.as_ref().bits(), 1);
    assert_eq!(b.as_ref().float(), -1.0);
    #[allow(clippy::clone_on_copy)] // exercises the explicit `Clone` impl
    let c = Clone::clone(&b);
    assert_eq!(c.as_ref().bits(), b.as_ref().bits());
}

#[test]
fn heap_box_owner_lives_reads_writes_clones_and_frees() {
    let mut a = HeapBox::<IntFloat64>::new_zeroed().unwrap();
    for bits in BITS {
        a.as_mut().set_bits(bits);
        assert_eq!(a.as_ref().float().to_bits(), bits);
    }
    let b = a.clone();
    let c = b.try_clone().unwrap();
    // The source may go first; the copies own their own blocks.
    drop(a);
    assert_eq!(b.as_ref().bits(), u64::MAX);
    drop(b);
    assert_eq!(c.as_ref().bits(), u64::MAX);
}

#[test]
fn heap_array_handles_stay_in_bounds() {
    let mut a = alloc_cell_array_zeroed::<IntFloat64>(BITS.len()).unwrap();
    {
        let mut handles = a.as_handles_mut();
        for (i, bits) in BITS.into_iter().enumerate() {
            handles.get_mut(i).unwrap().set_bits(bits);
        }
        assert!(handles.get_mut(BITS.len()).is_none());
    }
    let read: Vec<u64> = a.as_handles().iter().map(|h| h.float().to_bits()).collect();
    assert_eq!(read, BITS);
    assert!(a.as_handles().get(BITS.len()).is_none());
    drop(alloc_cell_array_zeroed::<IntFloat64>(0).unwrap());
}

#[test]
fn values_and_owners_move_across_threads() {
    let v = IntFloat64::from_float(2.0);
    let b = HeapBox::<IntFloat64>::new_zeroed().unwrap();
    let (v, b) = thread::spawn(move || {
        let mut b = b;
        b.as_mut().set_bits(v.as_ref().bits());
        (v, b)
    })
    .join()
    .unwrap();
    assert_eq!(b.as_ref().float(), v.as_ref().float());
    // Freed on another thread than the one that allocated it.
    thread::spawn(move || drop(b)).join().unwrap();
}

#[test]
fn shared_handles_read_concurrently() {
    let v = IntFloat64::from_bits(0x4009_21fb_5444_2d18);
    let boxed = HeapBox::<IntFloat64>::new_zeroed().unwrap();
    let (vr, br) = (v.as_ref(), boxed.as_ref());
    thread::scope(|s| {
        for _ in 0..4 {
            s.spawn(move || {
                for _ in 0..1000 {
                    assert_eq!(vr.float(), core::f64::consts::PI);
                    assert_eq!(br.bits(), 0);
                }
            });
        }
    });
}

#[test]
fn exclusive_handles_write_on_other_threads() {
    let mut values = [IntFloat64::zeroed(); 4];
    let mut boxed = HeapBox::<IntFloat64>::new_zeroed().unwrap();
    thread::scope(|s| {
        for (i, v) in values.iter_mut().enumerate() {
            let mut m = v.as_mut();
            s.spawn(move || m.set_bits(i as u64));
        }
        let mut m = boxed.as_mut();
        s.spawn(move || m.set_float(-0.0));
    });
    let bits: Vec<u64> = values.iter().map(|v| v.as_ref().bits()).collect();
    assert_eq!(bits, [0, 1, 2, 3]);
    assert_eq!(boxed.as_ref().bits(), 1 << 63);
}

/// Write each member in turn through `m`, reading both back after each write.
fn cycle_intfloat32_members(mut m: IntFloat32Mut<'_>, bits: u32) {
    m.set_bits(bits);
    assert_eq!(m.as_ref().float().to_bits(), bits);
    m.set_float(f32::from_bits(!bits));
    assert_eq!(m.as_ref().bits(), !bits);
    let copy = IntFloat32::from_bits(m.as_ref().bits());
    m.set_bits(bits);
    assert_eq!((m.as_ref().bits(), copy.as_ref().bits()), (bits, !bits));
}

#[test]
fn intfloat32_value_takes_every_member() {
    let mut a = IntFloat32::default();
    cycle_intfloat32_members(a.as_mut(), 0x7f80_0001);
    let mut m = a.as_mut();
    cycle_intfloat32_members(m.as_mut(), 0);
    cycle_intfloat32_members(m.as_mut(), u32::MAX);
    assert_eq!(a.as_ref().bits(), u32::MAX);
}

#[test]
fn intfloat32_heap_box_takes_every_member_and_clones() {
    let mut b = HeapBox::<IntFloat32>::new_zeroed().unwrap();
    assert_eq!(b.as_ref().float(), 0.0);
    cycle_intfloat32_members(b.as_mut(), 0x3f80_0000);
    let c = b.clone();
    b.as_mut().set_bits(1);
    drop(b);
    assert_eq!(c.as_ref().float(), 1.0);
    drop(HeapBox::<IntFloat32>::new_uninit().unwrap());
}

#[test]
fn intfloat32_heap_array_elements_stay_in_bounds() {
    let mut v = alloc_cell_array_zeroed::<IntFloat32>(3).unwrap();
    {
        let mut h = v.as_handles_mut();
        for i in 0..3 {
            cycle_intfloat32_members(h.get_mut(i).unwrap(), i as u32);
        }
        assert!(h.get_mut(3).is_none());
    }
    let r = v.as_handles();
    assert!(r.get(3).is_none());
    assert_eq!(r.iter().map(|e| e.bits()).collect::<Vec<_>>(), [0, 1, 2]);
}

#[test]
fn intfloat32_handles_and_owners_cross_threads() {
    let a = IntFloat32::from_float(0.5);
    let mut b = HeapBox::<IntFloat32>::new_zeroed().unwrap();
    let ra = a.as_ref();
    thread::scope(|s| {
        for _ in 0..4 {
            s.spawn(move || assert_eq!(ra.float(), 0.5));
            s.spawn(|| assert_eq!(a.as_ref().bits(), 0x3f00_0000));
        }
        let mb = b.as_mut();
        s.spawn(move || cycle_intfloat32_members(mb, 5));
    });
    assert_eq!(b.as_ref().bits(), 5);
    thread::spawn(move || drop(b)).join().unwrap();
}

#[test]
fn conversions_take_every_class_edge() {
    for bits in BITS {
        assert_eq!(double_to_bits(double_from_bits(bits)), bits);
    }
    for bits in [
        0,
        1,
        0x007f_ffff,
        0x0080_0000,
        0x7f7f_ffff,
        0x7f80_0000,
        0x7f80_0001,
        0x7fc0_0000,
        u32::MAX,
    ] {
        assert_eq!(float_to_bits(float_from_bits(bits)), bits);
    }
    let back = thread::spawn(|| double_to_bits(f64::NEG_INFINITY))
        .join()
        .unwrap();
    assert_eq!(back, f64::NEG_INFINITY.to_bits());
}

// Review 20261005 libavutil/core wave-0 batch-7: `av_intfloat64`'s array
// owner and its views, which the suites above never cloned or shared.

/// The array owner's clone is `av_memdup` of the whole block: every word is
/// copied, each copy frees its own block, and they go in any order.
#[test]
fn heap_array_of_words_clones_and_outlives_its_source() {
    let mut a = alloc_cell_array_zeroed::<IntFloat64>(BITS.len()).unwrap();
    for (mut h, bits) in a.as_handles_mut().iter_mut().zip(BITS) {
        h.set_bits(bits);
    }
    let b = a.clone();
    let c = b.try_clone().unwrap();
    a.as_handles_mut().get_mut(0).unwrap().set_float(1.0);
    drop(a);
    assert_eq!(b.len(), BITS.len());
    assert!(b.as_handles().iter().map(|h| h.bits()).eq(BITS));
    drop(b);
    assert!(c.as_handles().iter().map(|h| h.float().to_bits()).eq(BITS));

    let empty = alloc_cell_array_zeroed::<IntFloat64>(0).unwrap();
    let copy = empty.clone();
    drop(empty);
    assert!(copy.is_empty());
    assert!(copy.as_handles().get(0).is_none());
}

/// A shared run over the array is read by several threads at once, and
/// disjoint exclusive halves are written by two threads, while the owner
/// stays on this one.
#[test]
fn array_views_cross_threads() {
    let mut a = alloc_cell_array_zeroed::<IntFloat64>(BITS.len()).unwrap();
    for (mut h, bits) in a.as_handles_mut().iter_mut().zip(BITS) {
        h.set_bits(bits);
    }
    let run = a.as_handles();
    thread::scope(|s| {
        for _ in 0..4 {
            s.spawn(move || {
                for (h, bits) in run.iter().zip(BITS) {
                    assert_eq!(h.float().to_bits(), bits);
                }
            });
        }
    });

    let half = BITS.len() / 2;
    let mut handles = a.as_handles_mut();
    let (lo, hi) = handles.split_at_mut(half).unwrap();
    thread::scope(|s| {
        s.spawn(move || {
            let mut lo = lo;
            for mut h in lo.iter_mut() {
                h.set_bits(1);
            }
        });
        s.spawn(move || {
            let mut hi = hi;
            for mut h in hi.iter_mut() {
                h.set_float(2.0);
            }
        });
    });
    let words: Vec<u64> = a.as_handles().iter().map(|h| h.bits()).collect();
    assert!(words[..half].iter().all(|&w| w == 1));
    assert!(words[half..].iter().all(|&w| w == 2.0f64.to_bits()));
}
