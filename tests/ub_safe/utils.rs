//! UB tests for `libavutil/utils.c`'s [`MediaType`] and [`PictureType`],
//! through the safe API only: as values, and as the plain elements of
//! `av_malloc`-family arrays.
#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "test inputs are chosen within range, and the casts mirror the C conversions under comparison"
)]
#![expect(
    clippy::assert_is_empty,
    reason = "assertions keep the form of the C behaviour they check"
)]

use std::thread;

use ffmpeg_bindings::libavutil::libavutil::ffi;
use ffmpeg_bindings::libavutil::libavutil::mem::{
    HeapVec, alloc_array, alloc_array_from_fn, alloc_array_zeroed,
};
use ffmpeg_bindings::libavutil::libavutil::utils::{MediaType, PictureType};

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
#[cfg_attr(miri, ignore = "calls C")]
fn zeroed_array_reads_video_and_takes_every_value() {
    let values = sample();
    let mut a = alloc_array_zeroed::<MediaType>(values.len()).unwrap();
    assert!(a.as_slice().iter().all(|&t| t == MediaType::VIDEO));
    a.as_mut_slice().copy_from_slice(&values);
    assert_eq!(a.as_slice(), &values[..]);
    assert!(a.as_slice().get(values.len()).is_none());
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
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
#[cfg_attr(miri, ignore = "calls C")]
fn clone_is_independent_of_its_source() {
    let values = sample();
    let mut a = HeapVec::from_slice(&values).unwrap();
    let b = a.clone();
    a.as_mut_slice().fill(MediaType::UNKNOWN);
    drop(a);
    assert_eq!(b.as_slice(), &values[..]);
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn empty_arrays_allocate_and_free() {
    drop(alloc_array_zeroed::<MediaType>(0).unwrap());
    drop(HeapVec::<MediaType>::from_slice(&[]).unwrap());
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
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
#[cfg_attr(miri, ignore = "calls C")]
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
#[cfg_attr(miri, ignore = "calls C")]
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

/// `fourcc_string` on every byte class and from many threads.
mod fourcc {
    use std::thread;

    use ffmpeg_bindings::libavutil::libavutil::utils::fourcc_string;

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn fourcc_strings_fit_their_buffer() {
        let mut buf = [0u8; 32];
        // The longest: four bracketed three-digit bytes.
        assert_eq!(
            fourcc_string(&mut buf, 0xFFFF_FFFF),
            c"[255][255][255][255]"
        );
        for f in [0, 1, 0x8000_0000, 0x2E2D_205F, 0x0A0D_0900, u32::MAX - 1] {
            let s = fourcc_string(&mut buf, f);
            assert!(s.count_bytes() <= 20);
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
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

/// Names, letters and the time base (core wave 1, batch 2) for every value C
/// may store, here and on other threads.
mod names {
    use std::thread;

    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::utils::{MediaType, PictureType, time_base};

    /// `int` and `unsigned` edges, as the enums' encodings.
    const VALUES: [i64; 12] = [
        0,
        1,
        5,
        6,
        7,
        8,
        -1,
        -2,
        i32::MAX as i64,
        i32::MIN as i64,
        u32::MAX as i64,
        1 << 20,
    ];

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn every_value_names_and_letters() {
        for v in VALUES {
            let name = MediaType::from_raw(ffi::AVMediaType(v as i32)).as_str();
            assert!(name.is_none_or(|n| !n.is_empty() && n.is_ascii()));
            let letter = PictureType::from_raw(ffi::AVPictureType(v as u32)).to_char();
            assert!(letter.is_ascii_graphic());
        }
        assert_eq!(time_base().to_f64(), 1e-6);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn names_outlive_any_value_and_cross_threads() {
        let names: Vec<&'static str> = thread::scope(|s| {
            let workers: Vec<_> = (0..4)
                .map(|_| {
                    s.spawn(|| {
                        (-1..7)
                            .filter_map(|v| MediaType::from_raw(ffi::AVMediaType(v)).as_str())
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            workers
                .into_iter()
                .flat_map(|w| w.join().unwrap())
                .collect()
        });
        assert_eq!(names.len(), 4 * 5);
        let letters = thread::spawn(|| {
            (0..9)
                .map(|v| PictureType::from_raw(ffi::AVPictureType(v)).to_char())
                .collect::<String>()
        });
        assert_eq!(letters.join().unwrap(), "?IPBSipb?");
    }
}
