//! UB tests for `libavutil/samplefmt.h`'s [`SampleFormat`], through the safe
//! API only: as values, as table indices, and as the plain elements of
//! `av_malloc`-family arrays.

use std::collections::HashSet;
use std::thread;

use ffmpeg::libavutil::ffi;
use ffmpeg::libavutil::mem::{HeapVec, alloc_array, alloc_array_from_fn, alloc_array_zeroed};
use ffmpeg::libavutil::samplefmt::SampleFormat;

/// Named constants, `AV_SAMPLE_FMT_NB` and the `int` extremes.
fn sample() -> Vec<SampleFormat> {
    [i32::MIN, -2, -1, 0, 1, 5, 10, 11, 12, 13, i32::MAX]
        .map(|raw| SampleFormat::from_raw(ffi::AVSampleFormat(raw)))
        .into()
}

#[test]
fn index_and_debug_accept_every_value() {
    // A table sized by `COUNT` takes every index, once per counted format.
    let mut hits = [0; SampleFormat::COUNT];
    for raw in -3..=SampleFormat::COUNT as i32 + 2 {
        let f = SampleFormat::from_raw(ffi::AVSampleFormat(raw));
        if let Some(i) = f.index() {
            hits[i] += 1;
        }
        assert!(!format!("{f:?}").is_empty());
    }
    assert_eq!(hits, [1; SampleFormat::COUNT]);
    for f in sample() {
        if let Some(i) = f.index() {
            assert!(i < SampleFormat::COUNT);
        }
        assert!(!format!("{f:?}").is_empty());
    }
}

#[test]
fn values_hash_and_compare_by_encoding() {
    let set: HashSet<SampleFormat> = sample().into_iter().chain(sample()).collect();
    assert_eq!(set.len(), sample().len());
    for f in sample() {
        let raw: ffi::AVSampleFormat = f.into();
        assert_eq!(SampleFormat::from(raw), f);
        assert!(set.contains(&f));
    }
}

#[test]
fn none_terminated_list_stops_at_its_terminator() {
    // The shape of `AVCodec.sample_fmts`: formats up to `NONE`, then storage
    // past it that a reader must not reach.
    let list = [
        SampleFormat::FLTP,
        SampleFormat::S16,
        SampleFormat::from_raw(ffi::AVSampleFormat(99)),
        SampleFormat::NONE,
        SampleFormat::U8,
    ];
    let a = HeapVec::from_slice(&list).unwrap();
    let head: Vec<_> = a
        .as_slice()
        .iter()
        .copied()
        .take_while(|&f| f != SampleFormat::NONE)
        .collect();
    assert_eq!(head, list[..3]);
}

#[test]
fn zeroed_array_reads_u8_and_takes_every_value() {
    let values = sample();
    let mut a = alloc_array_zeroed::<SampleFormat>(values.len()).unwrap();
    assert!(a.as_slice().iter().all(|&f| f == SampleFormat::U8));
    a.as_mut_slice().copy_from_slice(&values);
    assert_eq!(a.as_slice(), &values[..]);
    assert!(a.as_slice().get(values.len()).is_none());
}

#[test]
fn uninit_and_from_fn_arrays_fill_end_to_end() {
    let values = sample();
    let mut u = alloc_array::<SampleFormat>(values.len()).unwrap();
    for (slot, &f) in u.as_mut_slice().iter_mut().zip(&values) {
        slot.write(f);
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
    a.as_mut_slice().fill(SampleFormat::NONE);
    drop(a);
    assert_eq!(b.as_slice(), &values[..]);
}

#[test]
fn empty_arrays_allocate_and_free() {
    drop(alloc_array_zeroed::<SampleFormat>(0).unwrap());
    drop(alloc_array::<SampleFormat>(0).unwrap());
    drop(HeapVec::<SampleFormat>::from_slice(&[]).unwrap());
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
