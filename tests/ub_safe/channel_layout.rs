//! UB tests for `libavutil/channel_layout.h`'s [`Channel`], through the safe
//! API only: as a value, its Ambisonic conversions at every boundary, and as
//! the plain element of `av_malloc`-family arrays.

use std::thread;

use ffmpeg::libavutil::channel_layout::Channel;
use ffmpeg::libavutil::ffi;
use ffmpeg::libavutil::mem::{HeapVec, alloc_array, alloc_array_from_fn, alloc_array_zeroed};

/// Named ids, the Ambisonic bounds and their neighbours, user ids and the
/// `int` extremes.
fn channel_sample() -> Vec<Channel> {
    [
        i32::MIN,
        i32::MIN + 1,
        -2,
        -1,
        0,
        17,
        18,
        62,
        63,
        64,
        0x200,
        0x300,
        0x3ff,
        0x400,
        0x7ff,
        0x800,
        i32::MAX - 1,
        i32::MAX,
    ]
    .map(|raw| Channel::from_raw(ffi::AVChannel(raw)))
    .into()
}

#[test]
fn channel_debug_order_and_ambisonic_index_accept_every_value() {
    let values = channel_sample();
    for &ch in &values {
        assert!(!format!("{ch:?}").is_empty());
        let raw: ffi::AVChannel = ch.into();
        assert_eq!(Channel::from(raw), ch);
        if let Some(acn) = ch.ambisonic_index() {
            assert_eq!(Channel::ambisonic(acn), Some(ch));
        }
        assert!(values.iter().all(|&o| o.cmp(&ch).is_eq() == (o == ch)));
    }
    let mut sorted = values.clone();
    sorted.sort();
    assert_eq!(sorted, values);
}

#[test]
fn ambisonic_accepts_every_acn() {
    let mut some = 0;
    for acn in 0..=u16::MAX {
        if let Some(ch) = Channel::ambisonic(acn) {
            assert_eq!(ch.ambisonic_index(), Some(acn));
            some += 1;
        }
    }
    assert_eq!(some, 1024);
}

#[test]
fn channel_arrays_fill_clone_and_free() {
    let values = channel_sample();
    let mut a = alloc_array_zeroed::<Channel>(values.len()).unwrap();
    assert!(a.as_slice().iter().all(|&ch| ch == Channel::FRONT_LEFT));
    a.as_mut_slice().copy_from_slice(&values);
    assert!(a.as_slice().get(values.len()).is_none());
    let mut u = alloc_array::<Channel>(values.len()).unwrap();
    for (slot, &ch) in u.as_mut_slice().iter_mut().zip(&values) {
        slot.write(ch);
    }
    drop(u);
    let f = alloc_array_from_fn(values.len(), |i| values[i]).unwrap();
    let b = a.clone();
    a.as_mut_slice().fill(Channel::NONE);
    a.as_mut_slice().sort_unstable_by(|x, y| y.cmp(x));
    drop(a);
    assert_eq!(b.as_slice(), &values[..]);
    assert_eq!(f.as_slice(), &values[..]);
    drop(alloc_array_zeroed::<Channel>(0).unwrap());
    drop(HeapVec::<Channel>::from_slice(&[]).unwrap());
}

#[test]
fn channel_arrays_cross_and_are_shared_between_threads() {
    let values = channel_sample();
    let a = HeapVec::from_slice(&values).unwrap();
    // Shared reads from several threads at once.
    thread::scope(|s| {
        for _ in 0..4 {
            s.spawn(|| {
                assert_eq!(a.as_slice(), &values[..]);
                assert!(a.as_slice().is_sorted());
            });
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

/// `MatrixEncoding`: every named and unnamed value through each conversion,
/// `av_malloc`-family arrays of it, and moving it across threads.
mod matrix_encoding {
    use std::thread;

    use ffmpeg::libavutil::channel_layout::MatrixEncoding;
    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::mem::{alloc_array_from_fn, alloc_array_zeroed};

    /// Raw values at every edge: each named one, the `NB` sentinel, and the
    /// `unsigned int` limits a C `int` field's negative values map onto.
    fn raws() -> impl Iterator<Item = u32> {
        (0..=8).chain([i32::MAX as u32, i32::MIN as u32, u32::MAX - 1, u32::MAX])
    }

    #[test]
    fn every_raw_value_converts_indexes_and_formats() {
        for raw in raws() {
            let e = MatrixEncoding::from_raw(ffi::AVMatrixEncoding(raw));
            assert_eq!(e.into_raw().0, raw);
            let back: ffi::AVMatrixEncoding = e.into();
            assert_eq!(MatrixEncoding::from(back), e);
            match e.index() {
                Some(i) => assert!(i < MatrixEncoding::COUNT && i as u32 == raw),
                None => assert!(raw as usize >= MatrixEncoding::COUNT),
            }
            assert!(!format!("{e:?}").is_empty());
        }
    }

    #[test]
    fn heap_arrays_read_any_stored_value() {
        let named = alloc_array_from_fn(MatrixEncoding::COUNT, |i| {
            MatrixEncoding::from_raw(ffi::AVMatrixEncoding(i as u32))
        })
        .unwrap();
        let idx: Vec<Option<usize>> = named.as_slice().iter().map(|e| e.index()).collect();
        assert_eq!(
            idx,
            (0..MatrixEncoding::COUNT).map(Some).collect::<Vec<_>>()
        );
        let mut zeroed = alloc_array_zeroed::<MatrixEncoding>(3).unwrap();
        assert!(zeroed.as_slice().iter().all(|&e| e == MatrixEncoding::NONE));
        zeroed.as_mut_slice()[2] = MatrixEncoding::from_raw(ffi::AVMatrixEncoding(u32::MAX));
        let copy = zeroed.clone();
        drop(zeroed);
        assert_eq!(copy.as_slice()[2].index(), None);
    }

    #[test]
    fn values_cross_threads() {
        let all: Vec<MatrixEncoding> = raws()
            .map(|r| MatrixEncoding::from_raw(ffi::AVMatrixEncoding(r)))
            .collect();
        let shared = &all;
        thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(move || {
                    for e in shared {
                        let _ = (e.index(), format!("{e:?}"));
                    }
                });
            }
        });
        let e = thread::spawn(|| MatrixEncoding::DPLIIZ).join().unwrap();
        assert_eq!(e, MatrixEncoding::DPLIIZ);
    }
}
