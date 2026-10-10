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
