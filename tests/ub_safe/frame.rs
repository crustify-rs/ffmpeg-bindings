//! UB tests for `libavutil/frame.h`'s wrapped types, through the safe API
//! only.

/// [`SideDataType`] as a value and as the plain element of `av_malloc`-family
/// arrays, the storage a C array of `enum AVFrameSideDataType` lives in
/// (buffer-frame wave 0, batch 2).
///
/// [`SideDataType`]: ffmpeg::libavutil::frame::SideDataType
mod side_data_type {
    use std::collections::HashSet;
    use std::thread;

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::frame::SideDataType;
    use ffmpeg::libavutil::mem::{HeapVec, alloc_array, alloc_array_from_fn, alloc_array_zeroed};

    /// Named values at both ends, the first values past the table, and the
    /// `unsigned` extremes a negative `int` written into C storage reads as.
    fn sample() -> Vec<SideDataType> {
        [
            0,
            1,
            36,
            37,
            38,
            39,
            1 << 20,
            i32::MAX as u32,
            i32::MIN as u32,
            u32::MAX - 1,
            u32::MAX,
        ]
        .map(|raw| SideDataType::from_raw(ffi::AVFrameSideDataType(raw)))
        .into()
    }

    #[test]
    fn every_value_converts_hashes_and_formats() {
        let values = sample();
        for &t in &values {
            let raw: ffi::AVFrameSideDataType = t.into();
            assert_eq!(SideDataType::from(raw), t);
            assert_eq!(SideDataType::from_raw(t.into_raw()), t);
            assert!(!format!("{t:?}").is_empty());
        }
        let set: HashSet<SideDataType> = values.iter().copied().collect();
        assert_eq!(set.len(), values.len());
    }

    #[test]
    fn zeroed_array_reads_pan_scan_and_takes_every_value() {
        let values = sample();
        let mut a = alloc_array_zeroed::<SideDataType>(values.len()).unwrap();
        assert!(a.as_slice().iter().all(|&t| t == SideDataType::PAN_SCAN));
        a.as_mut_slice().copy_from_slice(&values);
        assert_eq!(a.as_slice(), &values[..]);
        assert!(a.as_slice().get(values.len()).is_none());
    }

    #[test]
    fn uninit_from_fn_and_cloned_arrays_are_independent() {
        let values = sample();
        let mut u = alloc_array::<SideDataType>(values.len()).unwrap();
        for (slot, &t) in u.as_mut_slice().iter_mut().zip(&values) {
            slot.write(t);
        }
        drop(u);
        let f = alloc_array_from_fn(values.len(), |i| values[i]).unwrap();
        let mut a = HeapVec::from_slice(&values).unwrap();
        let b = a.clone();
        a.as_mut_slice().fill(SideDataType::DOWNMIX_MATRIX);
        drop(a);
        assert_eq!(b.as_slice(), &values[..]);
        assert_eq!(f.as_slice(), &values[..]);
        drop(alloc_array_zeroed::<SideDataType>(0).unwrap());
        drop(HeapVec::<SideDataType>::from_slice(&[]).unwrap());
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
        let t = thread::spawn(|| SideDataType::ICC_PROFILE).join().unwrap();
        assert_eq!(t, SideDataType::ICC_PROFILE);
    }
}

/// `ActiveFormatDescription` (buffer-frame wave 0, batch 0): every named,
/// reserved and out-of-byte value through each conversion, `av_malloc`-family
/// arrays of it, and sharing and moving it across threads.
mod active_format_description {
    use std::thread;

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::frame::ActiveFormatDescription as Afd;
    use ffmpeg::libavutil::mem::{alloc_array_from_fn, alloc_array_zeroed};

    /// Raw values at every edge: each byte, the first values past a byte, and
    /// the `unsigned int` limits a C `int`'s negative values map onto.
    fn raws() -> impl Iterator<Item = u32> {
        (0..=0x101).chain([i32::MAX as u32, i32::MIN as u32, u32::MAX - 1, u32::MAX])
    }

    #[test]
    fn every_raw_value_converts_and_formats() {
        for raw in raws() {
            let afd = Afd::from_raw(ffi::AVActiveFormatDescription(raw));
            assert_eq!(afd.into_raw().0, raw);
            let back: ffi::AVActiveFormatDescription = afd.into();
            assert_eq!(Afd::from(back), afd);
            match afd.to_byte() {
                Some(b) => {
                    assert_eq!(u32::from(b), raw);
                    assert_eq!(Afd::from(b), afd);
                }
                None => assert!(raw > u32::from(u8::MAX)),
            }
            assert!(!format!("{afd:?}").is_empty());
        }
    }

    #[test]
    fn heap_arrays_read_any_stored_value() {
        let bytes = alloc_array_from_fn(256, |i| Afd::from_byte(i as u8)).unwrap();
        let back: Vec<Option<u8>> = bytes.as_slice().iter().map(|a| a.to_byte()).collect();
        assert_eq!(back, (0..=u8::MAX).map(Some).collect::<Vec<_>>());

        let mut zeroed = alloc_array_zeroed::<Afd>(3).unwrap();
        assert!(zeroed.as_slice().iter().all(|&a| a == Afd::from_byte(0)));
        zeroed.as_mut_slice()[1] = Afd::SP_4_3;
        zeroed.as_mut_slice()[2] = Afd::from_raw(ffi::AVActiveFormatDescription(u32::MAX));
        let copy = zeroed.clone();
        drop(zeroed);
        assert_eq!(copy.as_slice()[1], Afd::SP_4_3);
        assert_eq!(copy.as_slice()[2].to_byte(), None);
    }

    #[test]
    fn values_cross_threads() {
        let all: Vec<Afd> = raws()
            .map(|r| Afd::from_raw(ffi::AVActiveFormatDescription(r)))
            .collect();
        let shared = &all;
        thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(move || {
                    for afd in shared {
                        let _ = (afd.to_byte(), format!("{afd:?}"));
                    }
                });
            }
        });
        let afd = thread::spawn(|| Afd::ASPECT_16_9_SP_14_9).join().unwrap();
        assert_eq!(afd, Afd::ASPECT_16_9_SP_14_9);
    }
}
