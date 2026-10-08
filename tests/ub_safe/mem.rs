//! UB tests for `libavutil/mem.h`'s `av_malloc`-family owners, through the
//! safe API only.

use core::mem::MaybeUninit;
use std::panic::{AssertUnwindSafe, catch_unwind};

use ffmpeg::libavutil::mem::{Heap, HeapVec, alloc_array, alloc_array_from_fn, alloc_array_zeroed};

#[test]
fn uninit_array_is_filled_end_to_end_and_freed() {
    let mut a = alloc_array::<u64>(257).unwrap();
    assert_eq!(a.len(), 257);
    for (i, slot) in a.as_mut_slice().iter_mut().enumerate() {
        slot.write(i as u64);
    }
    assert_eq!(a.as_slice().len(), 257);
    assert!(a.as_slice().get(257).is_none());
}

#[test]
fn every_element_shape_allocates_and_frees() {
    drop(alloc_array::<u8>(1).unwrap());
    drop(alloc_array::<u128>(3).unwrap());
    drop(alloc_array::<[u8; 16]>(5).unwrap());
    drop(alloc_array::<*const u8>(2).unwrap());
    drop(alloc_array_zeroed::<f64>(9).unwrap());
    drop(alloc_array_zeroed::<*mut u32>(4).unwrap());
    drop(alloc_array_zeroed::<[i16; 3]>(4).unwrap());
    drop(alloc_array_from_fn(6, |i| [i as u8; 7]).unwrap());
}

#[test]
fn zeroed_array_reads_zero_and_writes_stay_in_bounds() {
    let mut a = alloc_array_zeroed::<i32>(64).unwrap();
    assert!(a.as_slice().iter().all(|&x| x == 0));
    a.as_mut_slice()[63] = -1;
    assert_eq!(a.as_slice()[63], -1);
    let r = catch_unwind(AssertUnwindSafe(|| a.as_mut_slice()[64] = 1));
    assert!(
        r.is_err(),
        "an out-of-bounds index panics instead of writing"
    );
    assert_eq!(a.as_slice()[..63].iter().sum::<i32>(), 0);
}

#[test]
fn null_ptr_elements_are_plain_values() {
    let a = alloc_array_zeroed::<*const u8>(3).unwrap();
    assert!(a.as_slice().iter().all(|p| p.is_null()));
}

#[test]
fn empty_and_zero_sized_arrays_are_sound() {
    for a in [
        alloc_array_zeroed::<u8>(0).unwrap(),
        HeapVec::from_slice(&[]).unwrap(),
        HeapVec::default(),
    ] {
        assert!(a.is_empty());
        assert_eq!(a.as_slice(), &[] as &[u8]);
        let c = a.clone();
        assert!(c.is_empty());
    }
    let mut z = alloc_array_zeroed::<()>(1 << 20).unwrap();
    assert_eq!(z.as_mut_slice().len(), 1 << 20);
    let u = alloc_array::<()>(3).unwrap();
    assert_eq!(u.byte_len(), 0);
}

#[test]
fn empty_arrays_never_reach_the_allocator() {
    let mut a = HeapVec::<u64>::default();
    assert!(a.as_ptr().is_null());
    assert_eq!(a.as_mut_slice(), &mut [] as &mut [u64]);
    let b = HeapVec::<u64>::from_slice(&[]).unwrap();
    let c = b.clone();
    assert!(b.as_ptr().is_null() && c.as_ptr().is_null());
    assert_eq!(c.as_slice().iter().count(), 0);
}

#[test]
fn failed_allocations_are_none_not_dangling() {
    assert!(alloc_array::<u32>(usize::MAX).is_none());
    assert!(alloc_array::<u8>(usize::MAX).is_none());
    assert!(alloc_array_zeroed::<u16>(usize::MAX / 2 + 1).is_none());
    assert!(alloc_array_from_fn::<u8>(usize::MAX, |_| 0).is_none());
}

#[test]
fn copies_are_independent_through_clone_chains() {
    let mut a = HeapVec::<u32>::from_slice(&[1, 2, 3, 4]).unwrap();
    let b = a.clone();
    a.as_mut_slice()[0] = 9;
    let c = a.try_clone().unwrap();
    drop(a);
    assert_eq!(b.as_slice(), &[1, 2, 3, 4]);
    assert_eq!(c.as_slice(), &[9, 2, 3, 4]);
    let d = c.clone();
    drop(c);
    assert_eq!(d.as_slice(), &[9, 2, 3, 4]);
}

#[test]
fn uninit_array_clones_without_reading_as_init() {
    let mut a = alloc_array::<u8>(32).unwrap();
    a.as_mut_slice()[0] = MaybeUninit::new(5);
    let b = a.clone();
    assert_eq!(b.len(), 32);
}

#[test]
fn panicking_initializer_leaves_nothing_behind() {
    let r = catch_unwind(|| {
        alloc_array_from_fn(16, |i| if i == 15 { panic!("stop") } else { i as u8 })
    });
    assert!(r.is_err());
    let r =
        catch_unwind(|| alloc_array_from_fn(16, |i| if i == 0 { panic!("stop") } else { i as u8 }));
    assert!(r.is_err());
}

#[test]
fn owners_drop_on_other_threads() {
    let a = alloc_array_from_fn(1024, |i| i as u16).unwrap();
    let b = HeapVec::<u8>::from_slice(b"crustify").unwrap();
    std::thread::spawn(move || {
        assert_eq!(a.as_slice()[1023], 1023);
        drop(b);
    })
    .join()
    .unwrap();
}

#[test]
fn shared_array_is_read_and_cloned_concurrently() {
    let a = alloc_array_from_fn(4096, |i| i as u32).unwrap();
    std::thread::scope(|s| {
        for t in 0..4 {
            let a = &a;
            s.spawn(move || {
                let c = a.clone();
                assert_eq!(c.as_slice(), a.as_slice());
                assert_eq!(a.as_slice()[t * 1000], (t * 1000) as u32);
            });
        }
    });
}

#[test]
fn exclusive_slices_are_written_from_scoped_threads() {
    let mut a = alloc_array_zeroed::<u64>(4000).unwrap();
    std::thread::scope(|s| {
        for (k, chunk) in a.as_mut_slice().chunks_mut(1000).enumerate() {
            s.spawn(move || chunk.fill(k as u64));
        }
    });
    assert_eq!(a.as_slice()[3999], 3);
}

#[test]
fn policy_is_a_plain_value() {
    let h = Heap;
    let copy = h;
    assert_eq!(h, copy);
    assert_eq!(format!("{h:?}"), "Heap");
}
