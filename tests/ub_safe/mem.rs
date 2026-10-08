//! UB tests for `libavutil/mem.h`'s `av_malloc`-family owners, its dynamic
//! arrays and its reusable buffers, through the safe API only.

use core::mem::MaybeUninit;
use std::panic::{AssertUnwindSafe, catch_unwind};

use ffmpeg::libavutil::intfloat::IntFloat64;
use ffmpeg::libavutil::mem::{
    DynArray, FastBuffer, Heap, HeapString, HeapVec, StringHeap, ZeroedFastBuffer, alloc_array,
    alloc_array_from_fn, alloc_array_zeroed, string_from_bytes_until_nul, string_from_c_str,
};

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

/// Every byte value but NUL, then a 4 KiB run: views over C strings stay at or
/// under 4 KiB (BSan's runtime grows without bound freeing a C block after a
/// 32 KiB view).
fn sample_bytes() -> Vec<Vec<u8>> {
    vec![
        Vec::new(),
        b"x".to_vec(),
        b"crustify".to_vec(),
        (1..=255u8).collect(),
        vec![b'z'; 4095],
    ]
}

#[test]
fn strings_are_built_read_and_freed_end_to_end() {
    for bytes in sample_bytes() {
        let s = string_from_bytes_until_nul(&bytes).unwrap();
        assert_eq!(s.as_bytes(), &bytes[..]);
        assert_eq!(s.len(), bytes.len());
        assert_eq!(s.is_empty(), bytes.is_empty());
        assert_eq!(s.as_c_str().to_bytes_with_nul().last(), Some(&0));
        let t = string_from_c_str(s.as_c_str()).unwrap();
        assert_eq!(t.as_bytes(), s.as_bytes());
        assert_ne!(t.as_ptr(), s.as_ptr());
    }
}

#[test]
fn interior_nul_truncates_and_never_reads_past_the_slice() {
    let s = string_from_bytes_until_nul(b"ab\0cd").unwrap();
    assert_eq!(s.as_bytes(), b"ab");
    // No NUL in the slice: the copy stops at its end, not at whatever follows.
    let backing = b"0123456789";
    for n in 0..=backing.len() {
        let s = string_from_bytes_until_nul(&backing[..n]).unwrap();
        assert_eq!(s.as_bytes(), &backing[..n]);
    }
    let s = string_from_bytes_until_nul(&[0; 16]).unwrap();
    assert!(s.is_empty());
}

#[test]
fn non_utf8_is_an_error_not_a_bad_str() {
    let s = string_from_bytes_until_nul(&[b'a', 0xff, 0xfe]).unwrap();
    assert!(s.to_str().is_err());
    assert_eq!(s.as_bytes(), &[b'a', 0xff, 0xfe]);
    let s = string_from_bytes_until_nul("héllo".as_bytes()).unwrap();
    assert_eq!(s.to_str(), Ok("héllo"));
}

#[test]
fn string_copies_are_independent_through_clone_chains() {
    let a = string_from_c_str(c"origin").unwrap();
    let b = a.clone();
    let c = b.try_clone().unwrap();
    drop(a);
    let d = c.clone();
    drop(c);
    assert_eq!(b.to_str(), Ok("origin"));
    assert_eq!(d.to_str(), Ok("origin"));
    assert_ne!(b.as_ptr(), d.as_ptr());
    let empty = string_from_c_str(c"").unwrap();
    assert!(empty.clone().is_empty());
}

#[test]
fn strings_drop_on_other_threads() {
    let s = string_from_c_str(c"travels").unwrap();
    let t = s.clone();
    std::thread::spawn(move || {
        assert_eq!(t.as_bytes(), b"travels");
        drop(s);
    })
    .join()
    .unwrap();
}

#[test]
fn shared_string_is_read_and_cloned_concurrently() {
    let s = string_from_bytes_until_nul(&[b'q'; 4000]).unwrap();
    std::thread::scope(|scope| {
        for _ in 0..4 {
            let s = &s;
            scope.spawn(move || {
                let c = s.clone();
                assert_eq!(c.as_bytes(), s.as_bytes());
                assert_eq!(s.len(), 4000);
            });
        }
    });
}

#[test]
fn optional_strings_and_debug_are_sound() {
    let none: Option<HeapString> = None;
    assert!(none.is_none());
    let some = Some(string_from_c_str(c"dbg").unwrap());
    assert_eq!(format!("{:?}", some.as_ref().unwrap()), "CStrBox(\"dbg\")");
    let p = format!("{:p}", some.as_ref().unwrap());
    assert!(p.starts_with("0x"));
    assert_eq!(format!("{StringHeap:?}"), "StringHeap");
}

#[test]
fn dynarray_grows_through_every_doubling() {
    let mut a = DynArray::<u8>::new();
    assert_eq!(a.as_slice(), &[] as &[u8]);
    for i in 0..4096usize {
        assert!(a.push_or_clear(i as u8));
        assert_eq!(a.len(), i + 1);
        // Viewed at each doubling only: BSan retags the whole slice per view.
        if i.is_power_of_two() {
            assert_eq!(a.as_slice()[i], i as u8);
        }
    }
    let want: Vec<u8> = (0..4096).map(|i| i as u8).collect();
    assert_eq!(a.as_slice(), &want[..]);
    a.as_mut_slice().fill(7);
    assert_eq!(a.as_slice(), &[7; 4096][..]);
    let r = catch_unwind(AssertUnwindSafe(|| a.as_mut_slice()[4096] = 1));
    assert!(
        r.is_err(),
        "an out-of-bounds index panics instead of writing"
    );
    let v = a.into_vec();
    assert_eq!(v.len(), 4096);
}

#[test]
fn dynarray_takes_every_element_shape() {
    let mut odd = DynArray::<[u8; 3]>::new();
    let mut wide = DynArray::<f64>::new();
    let mut ptrs = DynArray::<*const u64>::default();
    for i in 0..300 {
        assert!(odd.push_or_clear([i as u8; 3]));
        assert!(wide.push_or_clear(i as f64 / 3.0));
        assert!(ptrs.push_or_clear(core::ptr::null()));
    }
    assert_eq!(odd.as_slice()[299], [43; 3]);
    assert_eq!(wide.as_slice().len(), 300);
    assert!(ptrs.as_slice().iter().all(|p| p.is_null()));
    assert!(!format!("{odd:?}").is_empty());
}

#[test]
fn pointer_arrays_store_dangling_pointers_without_following_them() {
    let mut a = DynArray::<*const String>::new();
    let mut b = DynArray::<*mut u8>::new();
    {
        let s = String::from("gone soon");
        let mut byte = 0u8;
        for _ in 0..100 {
            assert!(a.push_ptr_or_clear(&s));
            assert_eq!(b.try_push_ptr(&mut byte), Ok(()));
        }
    }
    // The pointees are gone; the array still owns only its block.
    assert_eq!((a.len(), b.len()), (100, 100));
    assert!(a.as_slice().windows(2).all(|w| w[0] == w[1]));
    drop(a);
    drop(b);
}

#[test]
fn cell_elements_are_reached_through_handles() {
    let mut a = DynArray::<IntFloat64>::new();
    for i in 0..65u64 {
        assert!(a.push_or_clear(IntFloat64::from_bits(i)));
    }
    assert_eq!(a.as_handles().len(), 65);
    for mut h in a.as_handles_mut().iter_mut() {
        let b = h.as_ref().bits();
        h.set_bits(b * 2);
    }
    let bits: Vec<u64> = a.as_handles().iter().map(|h| h.bits()).collect();
    assert_eq!(bits, (0..65).map(|i| i * 2).collect::<Vec<_>>());
    assert!(a.as_handles().get(65).is_none());
}

#[test]
fn dynarrays_cross_and_share_threads() {
    let mut a = DynArray::<u32>::new();
    assert!(a.push_or_clear(1));
    let mut a = std::thread::spawn(move || {
        for i in 2..=1000 {
            assert!(a.push_or_clear(i));
        }
        a
    })
    .join()
    .unwrap();
    std::thread::scope(|s| {
        for _ in 0..4 {
            s.spawn(|| assert_eq!(a.as_slice().iter().sum::<u32>(), 500_500));
        }
    });
    std::thread::scope(|s| {
        s.spawn(|| a.as_mut_slice().fill(3));
    });
    std::thread::spawn(move || drop(a)).join().unwrap();
}

/// Requested sizes around every branch, with failures above `UINT_MAX`;
/// successes stay at or under 4 KiB, for BSan.
const FAST_SIZES: [usize; 11] = [
    0,
    1,
    33,
    33,
    34,
    2000,
    7,
    u32::MAX as usize + 1,
    4000,
    usize::MAX,
    0,
];

#[test]
fn fast_buffers_are_writable_end_to_end_after_every_regrowth() {
    let (mut keep, mut discard, mut zeroed) = (
        FastBuffer::new(),
        FastBuffer::default(),
        ZeroedFastBuffer::new(),
    );
    for min in FAST_SIZES {
        let old = keep.capacity();
        let ok = keep.reserve(min);
        assert!(keep.capacity() >= if ok { min } else { old });
        // One slice-wide write: BSan retags per reference.
        keep.as_mut_slice().fill(MaybeUninit::new(0x5a));

        let ok = discard.reserve_discarding(min);
        assert_eq!(ok, discard.capacity() >= min);
        assert!(ok || discard.capacity() == 0);
        discard.as_mut_slice().fill(MaybeUninit::new(1));

        let ok = zeroed.reserve_discarding(min);
        assert_eq!(ok, zeroed.capacity() >= min);
        // Fresh: zero; reused: the last fill. Compared whole, not per byte,
        // which BSan retags.
        let s = zeroed.as_slice();
        let first = s.first().copied().unwrap_or(0);
        assert!(first == 0 || first == 9);
        assert_eq!(s, &vec![first; s.len()][..]);
        zeroed.as_mut_slice().fill(9);
    }
    assert_eq!(zeroed.into_vec().len(), 0);
    assert!(!format!("{keep:?}{discard:?}").is_empty());
    let v = keep.into_vec();
    assert!(v.len() >= 4000);
}

#[test]
fn fast_buffers_cross_and_share_threads() {
    let mut keep = FastBuffer::new();
    assert!(keep.reserve(100));
    let mut zeroed = ZeroedFastBuffer::new();
    assert!(zeroed.reserve_discarding(3000));
    let (keep, zeroed) = std::thread::spawn(move || {
        assert!(keep.capacity() >= 100);
        (keep, zeroed)
    })
    .join()
    .unwrap();
    std::thread::scope(|s| {
        for _ in 0..4 {
            s.spawn(|| assert_eq!(zeroed.as_slice(), &vec![0; zeroed.capacity()][..]));
        }
    });
    let mut zeroed = zeroed;
    std::thread::scope(|s| {
        s.spawn(|| assert!(zeroed.reserve_discarding(4000)));
    });
    std::thread::spawn(move || drop((keep, zeroed)))
        .join()
        .unwrap();
}

// ---- the av_realloc family ----

mod realloc {
    use core::mem::MaybeUninit;
    use std::panic::{AssertUnwindSafe, catch_unwind};

    use ffmpeg::libavutil::mem::{
        HeapVec, alloc_array, alloc_array_from_fn, checked_size_mul, realloc_array,
        realloc_array_in_place, realloc_array_or_free, realloc_bytes, realloc_bytes_in_place,
        resize_array_with,
    };

    /// Writes every element: ASan flags a length longer than the block.
    fn touch<T: Copy>(a: &mut HeapVec<MaybeUninit<T>>, v: T) {
        for slot in a.as_mut_slice() {
            slot.write(v);
        }
    }

    #[test]
    fn grow_shrink_cycles_stay_in_bounds() {
        let mut a = realloc_array::<u32>(HeapVec::empty(), 1).unwrap();
        for len in [1, 7, 64, 3, 0, 1000, 2, 4096, 1] {
            a = realloc_array(a, len).unwrap();
            assert_eq!(a.len(), len);
            touch(&mut a, len as u32);
        }
        let mut b = realloc_bytes(HeapVec::empty(), 0).unwrap();
        for size in [1, 33, 0, 4096, 5] {
            b = realloc_bytes(b, size).unwrap();
            touch(&mut b, 0xEE);
        }
    }

    #[test]
    fn failures_leave_a_usable_array_or_free_it() {
        let mut a = alloc_array::<u64>(4).unwrap();
        touch(&mut a, 1);
        for len in [usize::MAX, usize::MAX / 8, i32::MAX as usize] {
            a = realloc_array(a, len).unwrap_err();
            assert_eq!(a.len(), 4);
            touch(&mut a, 2);
        }
        let b = realloc_bytes(alloc_array::<u8>(3).unwrap(), usize::MAX).unwrap_err();
        assert_eq!(b.len(), 3);
        assert!(realloc_array_or_free(b, usize::MAX).is_none());
        let c = realloc_array_or_free(alloc_array::<u16>(2).unwrap(), 9).unwrap();
        assert_eq!(c.len(), 9);
        let d = alloc_array_from_fn(3, |i| i as u8).unwrap();
        let d = resize_array_with(d, usize::MAX, |_| 0).unwrap_err();
        assert_eq!(d.as_slice(), &[0, 1, 2]);
    }

    #[test]
    fn initialised_resizes_never_expose_uninit_elements() {
        let mut a = alloc_array_from_fn(0, |_| 0u64).unwrap();
        for len in [5, 1, 300, 0, 17] {
            a = resize_array_with(a, len, |i| i as u64 * 3).unwrap();
            let sum: u64 = a.as_slice().iter().sum();
            assert_eq!(a.len(), len);
            assert!(sum < u64::MAX);
        }
    }

    #[test]
    fn panicking_fill_frees_the_storage() {
        // Plain elements: owning ones would leak, as `HeapVec` never drops
        // its elements, which LeakSanitizer reports.
        let a = alloc_array_from_fn(2, |i| [i as u8; 3]).unwrap();
        let r = catch_unwind(AssertUnwindSafe(|| {
            resize_array_with(a, 6, |i| if i == 4 { panic!("stop") } else { [i as u8; 3] })
        }));
        assert!(r.is_err());
    }

    #[test]
    fn zero_sized_and_empty_arrays_are_sound() {
        let z = realloc_array::<()>(HeapVec::empty(), 1_000_000).unwrap();
        assert_eq!(z.len(), 1_000_000);
        let z = realloc_array(z, 0).unwrap();
        assert!(z.is_empty());
        let mut slot = HeapVec::<MaybeUninit<()>>::empty();
        realloc_array_in_place(&mut slot, 77).unwrap();
        assert_eq!(slot.len(), 77);
        let e = resize_array_with(HeapVec::<u8>::empty(), 0, |_| 0).unwrap();
        assert!(e.is_empty());
    }

    #[test]
    fn in_place_slots_survive_every_transition() {
        let mut slot = HeapVec::<MaybeUninit<u8>>::empty();
        for size in [0, 9, usize::MAX, 0, 3, 4096, 0, 1] {
            let _ = realloc_bytes_in_place(&mut slot, size);
            touch(&mut slot, 1);
        }
        let mut slot = HeapVec::<MaybeUninit<u32>>::empty();
        for len in [2, 0, usize::MAX / 2, 5, 1024, 1] {
            let _ = realloc_array_in_place(&mut slot, len);
            touch(&mut slot, 7);
        }
    }

    #[test]
    fn arrays_resize_and_free_on_other_threads() {
        let a = alloc_array_from_fn(8, |i| i as u32).unwrap();
        let a = std::thread::spawn(move || resize_array_with(a, 64, |i| i as u32).unwrap())
            .join()
            .unwrap();
        assert_eq!(a.as_slice()[63], 63);
        std::thread::spawn(move || drop(a)).join().unwrap();
        assert_eq!(checked_size_mul(usize::MAX, 2), None);
    }
}
