//! UB tests for `libavutil/avutil.h`'s [`MediaType`] and [`PictureType`],
//! through the safe API only: as values, and as the plain elements of
//! `av_malloc`-family arrays.

use std::thread;

use ffmpeg::libavutil::avutil::{MediaType, PictureType};
use ffmpeg::libavutil::ffi;
use ffmpeg::libavutil::mem::{HeapVec, alloc_array, alloc_array_from_fn, alloc_array_zeroed};

/// Named constants, `AVMEDIA_TYPE_NB` and the `int` extremes.
fn sample() -> Vec<MediaType> {
    [i32::MIN, -2, -1, 0, 1, 2, 3, 4, 5, 6, i32::MAX]
        .map(|raw| MediaType::from_raw(ffi::AVMediaType(raw)))
        .into()
}

#[test]
fn index_and_debug_accept_every_value() {
    // An array sized by `COUNT` takes every index, once per counted type.
    let mut hits = [0; MediaType::COUNT];
    for t in sample() {
        if let Some(i) = t.index() {
            hits[i] += 1;
        }
        assert!(!format!("{t:?}").is_empty());
    }
    assert_eq!(hits, [1; MediaType::COUNT]);
}

#[test]
fn zeroed_array_reads_video_and_takes_every_value() {
    let values = sample();
    let mut a = alloc_array_zeroed::<MediaType>(values.len()).unwrap();
    assert!(a.as_slice().iter().all(|&t| t == MediaType::VIDEO));
    a.as_mut_slice().copy_from_slice(&values);
    assert_eq!(a.as_slice(), &values[..]);
    assert!(a.as_slice().get(values.len()).is_none());
}

#[test]
fn uninit_and_from_fn_arrays_fill_end_to_end() {
    let values = sample();
    let mut u = alloc_array::<MediaType>(values.len()).unwrap();
    for (slot, &t) in u.as_mut_slice().iter_mut().zip(&values) {
        slot.write(t);
    }
    let f = alloc_array_from_fn(values.len(), |i| values[i]).unwrap();
    assert_eq!(f.as_slice(), &values[..]);
    drop(u);
}

#[test]
fn clone_is_independent_of_its_source() {
    let values = sample();
    let mut a = HeapVec::from_slice(&values).unwrap();
    let b = a.clone();
    a.as_mut_slice().fill(MediaType::UNKNOWN);
    drop(a);
    assert_eq!(b.as_slice(), &values[..]);
}

#[test]
fn empty_arrays_allocate_and_free() {
    drop(alloc_array_zeroed::<MediaType>(0).unwrap());
    drop(HeapVec::<MediaType>::from_slice(&[]).unwrap());
}

#[test]
fn arrays_cross_and_are_shared_between_threads() {
    let values = sample();
    let a = HeapVec::from_slice(&values).unwrap();
    // Shared reads from several threads at once.
    thread::scope(|s| {
        for _ in 0..4 {
            s.spawn(|| assert_eq!(a.as_slice(), &values[..]));
        }
    });
    // Moved to another thread, written and freed there.
    let back = thread::spawn(move || {
        let mut a = a;
        a.as_mut_slice().reverse();
        a.as_slice().to_vec()
    })
    .join()
    .unwrap();
    assert_eq!(back.iter().rev().copied().collect::<Vec<_>>(), values);
}

/// Named picture types and values C may store unnamed, `u32::MAX` included.
fn picture_sample() -> Vec<PictureType> {
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 1 << 31, u32::MAX]
        .map(|raw| PictureType::from_raw(ffi::AVPictureType(raw)))
        .into()
}

#[test]
fn picture_type_debug_and_conversions_accept_every_value() {
    for t in picture_sample() {
        assert!(!format!("{t:?}").is_empty());
        let raw: ffi::AVPictureType = t.into();
        assert_eq!(PictureType::from(raw), t);
    }
}

#[test]
fn picture_type_arrays_fill_clone_and_free() {
    let values = picture_sample();
    let mut a = alloc_array_zeroed::<PictureType>(values.len()).unwrap();
    assert!(a.as_slice().iter().all(|&t| t == PictureType::NONE));
    a.as_mut_slice().copy_from_slice(&values);
    assert!(a.as_slice().get(values.len()).is_none());
    let mut u = alloc_array::<PictureType>(values.len()).unwrap();
    for (slot, &t) in u.as_mut_slice().iter_mut().zip(&values) {
        slot.write(t);
    }
    drop(u);
    let f = alloc_array_from_fn(values.len(), |i| values[i]).unwrap();
    let b = a.clone();
    a.as_mut_slice().fill(PictureType::BI);
    drop(a);
    assert_eq!(b.as_slice(), &values[..]);
    assert_eq!(f.as_slice(), &values[..]);
    drop(alloc_array_zeroed::<PictureType>(0).unwrap());
}

#[test]
fn picture_type_arrays_cross_and_are_shared_between_threads() {
    let values = picture_sample();
    let a = HeapVec::from_slice(&values).unwrap();
    thread::scope(|s| {
        for _ in 0..4 {
            s.spawn(|| assert_eq!(a.as_slice(), &values[..]));
        }
    });
    let back = thread::spawn(move || {
        let mut a = a;
        a.as_mut_slice().reverse();
        a.as_slice().to_vec()
    })
    .join()
    .unwrap();
    assert_eq!(back.iter().rev().copied().collect::<Vec<_>>(), values);
}

/// The version strings and `av_x_if_null`'s borrows.
mod version_and_x_if_null {
    use std::thread;

    use ffmpeg::libavutil::avutil::{
        configuration, license, version, version_info, x_if_null, x_if_null_mut,
    };

    #[test]
    fn static_strings_read_whole_and_cross_threads() {
        let lens = |_| {
            (
                version_info().count_bytes(),
                configuration().count_bytes(),
                license().count_bytes(),
            )
        };
        let theirs = thread::spawn(move || (0..100).map(lens).collect::<Vec<_>>())
            .join()
            .unwrap();
        assert!(theirs.iter().all(|&l| l == lens(0)));
        let strings: &'static [u8] = version_info().to_bytes();
        assert!(!strings.is_empty() && version() > 0);
    }

    #[test]
    fn x_if_null_results_borrow_their_sources() {
        let a = Box::new([1u64; 4]);
        let b = Box::new([2u64; 4]);
        let r = x_if_null(None, Some(&*b)).unwrap();
        assert_eq!(r, &[2; 4]);
        let r = x_if_null(Some(&*a), Some(&*b)).unwrap();
        assert_eq!(r, &[1; 4]);
        drop((a, b));
    }

    #[test]
    fn x_if_null_mut_results_write_their_source_only() {
        let mut a = Box::new([0u8; 8]);
        let mut b = Box::new([0u8; 8]);
        x_if_null_mut(None, Some(&mut *b)).unwrap().fill(1);
        x_if_null_mut(Some(&mut *a), Some(&mut *b)).unwrap()[7] = 9;
        assert_eq!((*a, *b), ([0, 0, 0, 0, 0, 0, 0, 9], [1; 8]));
        let mut zst = ();
        assert!(x_if_null_mut(Some(&mut zst), None).is_some());
        assert!(x_if_null::<String>(None, None).is_none());
    }
}

/// `fourcc_string` on every byte class and from many threads.
mod fourcc {
    use std::thread;

    use ffmpeg::libavutil::avutil::fourcc_string;

    #[test]
    fn fourcc_strings_fit_their_buffer() {
        let mut buf = [0u8; 32];
        // The longest: four bracketed three-digit bytes.
        assert_eq!(
            fourcc_string(&mut buf, 0xffff_ffff),
            c"[255][255][255][255]"
        );
        for f in [0, 1, 0x8000_0000, 0x2e2d_205f, 0x0a0d_0900, u32::MAX - 1] {
            let s = fourcc_string(&mut buf, f);
            assert!(s.count_bytes() <= 20);
        }
    }

    #[test]
    fn fourcc_strings_on_many_threads() {
        let handles: Vec<_> = (0..8u32)
            .map(|t| {
                thread::spawn(move || {
                    let mut buf = [0u8; 32];
                    (0..2000u32)
                        .map(|i| {
                            fourcc_string(&mut buf, i.wrapping_mul(0x0101_0101) ^ t).count_bytes()
                        })
                        .sum::<usize>()
                })
            })
            .collect();
        for h in handles {
            assert!(h.join().unwrap() >= 2000 * 4);
        }
    }
}
