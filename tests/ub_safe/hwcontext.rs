//! UB tests for `libavutil/hwcontext.h`'s wrapped types, through the safe API
//! only.

/// [`DeviceType`] as a value and as the plain element of `av_malloc`-family
/// arrays, the storage a C array of `enum AVHWDeviceType` lives in (hwcontext
/// wave 0, batch 0).
///
/// [`DeviceType`]: ffmpeg::libavutil::hwcontext::DeviceType
mod device_type {
    use std::collections::HashSet;
    use std::thread;

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::hwcontext::DeviceType;
    use ffmpeg::libavutil::mem::{HeapVec, alloc_array, alloc_array_from_fn, alloc_array_zeroed};

    /// Named values at both ends, the first values past the enumerators, and
    /// the `unsigned` extremes a negative `int` written into C storage reads
    /// as.
    fn sample() -> Vec<DeviceType> {
        [
            0,
            1,
            13,
            14,
            15,
            16,
            1 << 20,
            i32::MAX as u32,
            i32::MIN as u32,
            u32::MAX - 1,
            u32::MAX,
        ]
        .map(|raw| DeviceType::from_raw(ffi::AVHWDeviceType(raw)))
        .into()
    }

    #[test]
    fn every_value_converts_hashes_and_formats() {
        let values = sample();
        for &t in &values {
            let raw: ffi::AVHWDeviceType = t.into();
            assert_eq!(DeviceType::from(raw), t);
            assert_eq!(DeviceType::from_raw(t.into_raw()), t);
            assert!(!format!("{t:?}").is_empty());
        }
        let set: HashSet<DeviceType> = values.iter().copied().collect();
        assert_eq!(set.len(), values.len());
        assert_eq!(DeviceType::default(), DeviceType::NONE);
    }

    #[test]
    fn zeroed_array_reads_none_and_takes_every_value() {
        let values = sample();
        let mut a = alloc_array_zeroed::<DeviceType>(values.len()).unwrap();
        assert!(a.as_slice().iter().all(|&t| t == DeviceType::default()));
        a.as_mut_slice().copy_from_slice(&values);
        assert_eq!(a.as_slice(), &values[..]);
        assert!(a.as_slice().get(values.len()).is_none());
    }

    #[test]
    fn uninit_from_fn_and_cloned_arrays_are_independent() {
        let values = sample();
        let mut u = alloc_array::<DeviceType>(values.len()).unwrap();
        for (slot, &t) in u.as_mut_slice().iter_mut().zip(&values) {
            slot.write(t);
        }
        drop(u);
        let f = alloc_array_from_fn(values.len(), |i| values[i]).unwrap();
        let mut a = HeapVec::from_slice(&values).unwrap();
        let b = a.clone();
        a.as_mut_slice().fill(DeviceType::VULKAN);
        drop(a);
        assert_eq!(b.as_slice(), &values[..]);
        assert_eq!(f.as_slice(), &values[..]);
        drop(alloc_array_zeroed::<DeviceType>(0).unwrap());
        drop(HeapVec::<DeviceType>::from_slice(&[]).unwrap());
    }

    #[test]
    fn values_and_arrays_cross_and_are_shared_between_threads() {
        let values = sample();
        let a = HeapVec::from_slice(&values).unwrap();
        // Shared reads from several threads at once.
        thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(|| {
                    assert_eq!(a.as_slice(), &values[..]);
                    for t in a.as_slice() {
                        let _ = format!("{t:?}");
                    }
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
        let t = thread::spawn(|| DeviceType::CUDA).join().unwrap();
        assert_eq!(t, DeviceType::CUDA);
    }
}

/// [`TransferDirection`] through each conversion, and moved and shared across
/// threads (hwcontext wave 0, batch 0). It is not a plain element: the
/// `compile_fail` doctest on the type pins that C storage never reads as one.
///
/// [`TransferDirection`]: ffmpeg::libavutil::hwcontext::TransferDirection
mod transfer_direction {
    use std::collections::HashSet;
    use std::thread;

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::hwcontext::TransferDirection;

    const ALL: [TransferDirection; 2] = [TransferDirection::From, TransferDirection::To];

    #[test]
    fn every_direction_converts_hashes_and_formats() {
        for dir in ALL {
            let raw: ffi::AVHWFrameTransferDirection = dir.into();
            assert_eq!(raw, dir.into_raw());
            assert!(!format!("{dir:?}").is_empty());
        }
        let set: HashSet<TransferDirection> = ALL.into_iter().chain(ALL).collect();
        assert_eq!(set.len(), ALL.len());
    }

    #[test]
    fn directions_cross_and_are_shared_between_threads() {
        let dirs = ALL.to_vec();
        thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(|| {
                    let raw: Vec<_> = dirs.iter().map(|d| d.into_raw().0).collect();
                    assert_eq!(raw, [0, 1]);
                });
            }
        });
        let back = thread::spawn(move || ALL.map(ffi::AVHWFrameTransferDirection::from))
            .join()
            .unwrap();
        assert_eq!(back, ALL.map(TransferDirection::into_raw));
    }
}
