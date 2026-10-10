//! UB tests for `libavutil/pixfmt.h`'s safe types, through the safe API only.

/// [`ColorSpace`] and [`ColorTransfer`] (formats wave 0, batch 3): as values,
/// and as the plain elements of `av_malloc`-family arrays.
mod color_space_and_transfer {
    use std::collections::HashSet;
    use std::thread;

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::mem::{HeapVec, alloc_array, alloc_array_from_fn, alloc_array_zeroed};
    use ffmpeg::libavutil::pixfmt::{ColorSpace, ColorTransfer};

    /// Named colour spaces, `AVCOL_SPC_NB` and values C may store unnamed.
    fn space_sample() -> Vec<ColorSpace> {
        [0, 1, 2, 8, 17, 18, 19, 256, 1 << 31, u32::MAX]
            .map(|raw| ColorSpace::from_raw(ffi::AVColorSpace(raw)))
            .into()
    }

    /// Named transfer characteristics, both `_NB` markers, the extension
    /// base and values C may store unnamed.
    fn transfer_sample() -> Vec<ColorTransfer> {
        [0, 1, 2, 16, 18, 19, 255, 256, 257, 1 << 31, u32::MAX]
            .map(|raw| ColorTransfer::from_raw(ffi::AVColorTransferCharacteristic(raw)))
            .into()
    }

    #[test]
    fn debug_hash_and_conversions_accept_every_value() {
        let spaces = space_sample();
        let set: HashSet<_> = spaces.iter().copied().collect();
        assert_eq!(set.len(), spaces.len());
        for c in spaces {
            assert!(!format!("{c:?}").is_empty());
            let raw: ffi::AVColorSpace = c.into();
            assert_eq!(ColorSpace::from(raw), c);
        }
        let transfers = transfer_sample();
        let set: HashSet<_> = transfers.iter().copied().collect();
        assert_eq!(set.len(), transfers.len());
        for t in transfers {
            assert!(!format!("{t:?}").is_empty());
            let raw: ffi::AVColorTransferCharacteristic = t.into();
            assert_eq!(ColorTransfer::from(raw), t);
        }
        assert_eq!(ColorSpace::default(), ColorSpace::UNSPECIFIED);
        assert_eq!(ColorTransfer::default(), ColorTransfer::UNSPECIFIED);
    }

    #[test]
    fn color_space_arrays_fill_clone_and_free() {
        let values = space_sample();
        let mut a = alloc_array_zeroed::<ColorSpace>(values.len()).unwrap();
        assert!(a.as_slice().iter().all(|&c| c == ColorSpace::RGB));
        a.as_mut_slice().copy_from_slice(&values);
        assert!(a.as_slice().get(values.len()).is_none());
        let mut u = alloc_array::<ColorSpace>(values.len()).unwrap();
        for (slot, &c) in u.as_mut_slice().iter_mut().zip(&values) {
            slot.write(c);
        }
        drop(u);
        let f = alloc_array_from_fn(values.len(), |i| values[i]).unwrap();
        let b = a.clone();
        a.as_mut_slice().fill(ColorSpace::BT709);
        drop(a);
        assert_eq!(b.as_slice(), &values[..]);
        assert_eq!(f.as_slice(), &values[..]);
        drop(alloc_array_zeroed::<ColorSpace>(0).unwrap());
        drop(HeapVec::<ColorSpace>::from_slice(&[]).unwrap());
    }

    #[test]
    fn color_transfer_arrays_fill_clone_and_free() {
        let values = transfer_sample();
        let mut a = alloc_array_zeroed::<ColorTransfer>(values.len()).unwrap();
        assert!(a.as_slice().iter().all(|&t| t == ColorTransfer::RESERVED0));
        a.as_mut_slice().copy_from_slice(&values);
        assert!(a.as_slice().get(values.len()).is_none());
        let mut u = alloc_array::<ColorTransfer>(values.len()).unwrap();
        for (slot, &t) in u.as_mut_slice().iter_mut().zip(&values) {
            slot.write(t);
        }
        drop(u);
        let f = alloc_array_from_fn(values.len(), |i| values[i]).unwrap();
        let b = a.clone();
        a.as_mut_slice().fill(ColorTransfer::V_LOG);
        drop(a);
        assert_eq!(b.as_slice(), &values[..]);
        assert_eq!(f.as_slice(), &values[..]);
        drop(alloc_array_zeroed::<ColorTransfer>(0).unwrap());
        drop(HeapVec::<ColorTransfer>::from_slice(&[]).unwrap());
    }

    #[test]
    fn arrays_cross_and_are_shared_between_threads() {
        let spaces = HeapVec::from_slice(&space_sample()).unwrap();
        let transfers = HeapVec::from_slice(&transfer_sample()).unwrap();
        // Shared reads from several threads at once.
        thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(|| assert_eq!(spaces.as_slice(), &space_sample()[..]));
                s.spawn(|| assert_eq!(transfers.as_slice(), &transfer_sample()[..]));
            }
        });
        // Moved to other threads, written and freed there.
        let s = thread::spawn(move || {
            let mut a = spaces;
            a.as_mut_slice().reverse();
            a.as_slice().to_vec()
        });
        let t = thread::spawn(move || {
            let mut a = transfers;
            a.as_mut_slice().reverse();
            a.as_slice().to_vec()
        });
        let s = s.join().unwrap();
        let t = t.join().unwrap();
        assert_eq!(s.into_iter().rev().collect::<Vec<_>>(), space_sample());
        assert_eq!(t.into_iter().rev().collect::<Vec<_>>(), transfer_sample());
    }
}

/// [`AlphaMode`] (formats wave 0, batch 0): as a value, and as the plain
/// element of `av_malloc`-family arrays.
mod alpha_mode {
    use std::thread;

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::mem::{HeapVec, alloc_array, alloc_array_from_fn, alloc_array_zeroed};
    use ffmpeg::libavutil::pixfmt::AlphaMode;

    /// Named modes, `AVALPHA_MODE_NB`, `AVERROR(EINVAL)` and the extremes.
    fn alpha_mode_sample() -> Vec<AlphaMode> {
        [0, 1, 2, 3, 0xffff_ffea, 1 << 31, u32::MAX]
            .map(|raw| AlphaMode::from_raw(ffi::AVAlphaMode(raw)))
            .into()
    }

    #[test]
    fn alpha_mode_debug_and_conversions_accept_every_value() {
        for mode in alpha_mode_sample() {
            assert!(!format!("{mode:?}").is_empty());
            let raw: ffi::AVAlphaMode = mode.into();
            assert_eq!(AlphaMode::from(raw), mode);
        }
    }

    #[test]
    fn alpha_mode_arrays_fill_clone_and_free() {
        let values = alpha_mode_sample();
        let mut a = alloc_array_zeroed::<AlphaMode>(values.len()).unwrap();
        assert!(a.as_slice().iter().all(|&m| m == AlphaMode::default()));
        a.as_mut_slice().copy_from_slice(&values);
        assert!(a.as_slice().get(values.len()).is_none());
        let mut u = alloc_array::<AlphaMode>(values.len()).unwrap();
        for (slot, &m) in u.as_mut_slice().iter_mut().zip(&values) {
            slot.write(m);
        }
        drop(u);
        let f = alloc_array_from_fn(values.len(), |i| values[i]).unwrap();
        let b = a.clone();
        a.as_mut_slice().fill(AlphaMode::STRAIGHT);
        drop(a);
        assert_eq!(b.as_slice(), &values[..]);
        assert_eq!(f.as_slice(), &values[..]);
        drop(alloc_array_zeroed::<AlphaMode>(0).unwrap());
        drop(HeapVec::<AlphaMode>::from_slice(&[]).unwrap());
    }

    #[test]
    fn alpha_mode_arrays_cross_and_are_shared_between_threads() {
        let values = alpha_mode_sample();
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
}

/// [`PixelFormat`] (formats wave 0, batch 5): as a value, and as the plain
/// element of `av_malloc`-family arrays.
mod pixel_format {
    use std::collections::HashSet;
    use std::thread;

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::mem::{HeapVec, alloc_array, alloc_array_from_fn, alloc_array_zeroed};
    use ffmpeg::libavutil::pixfmt::PixelFormat;

    /// Named formats, `AV_PIX_FMT_NB` and its neighbours, and the `int` extremes.
    fn sample() -> Vec<PixelFormat> {
        let nb = ffi::AVPixelFormat::AV_PIX_FMT_NB.0;
        [
            i32::MIN,
            -2,
            -1,
            0,
            1,
            56,
            71,
            nb - 1,
            nb,
            nb + 1,
            1 << 30,
            i32::MAX,
        ]
        .map(|raw| PixelFormat::from_raw(ffi::AVPixelFormat(raw)))
        .into()
    }

    #[test]
    fn debug_and_conversions_accept_every_value() {
        for f in sample() {
            assert!(!format!("{f:?}").is_empty());
            let raw: ffi::AVPixelFormat = f.into();
            assert_eq!(PixelFormat::from(raw), f);
            assert_eq!(PixelFormat::from_raw(f.into_raw()), f);
        }
    }

    #[test]
    fn hash_and_eq_agree_on_every_value() {
        let set: HashSet<PixelFormat> = sample().into_iter().chain(sample()).collect();
        assert_eq!(set.len(), sample().len());
        // An alias is the same value as its primary constant.
        assert!(set.contains(&PixelFormat::Y400A));
        assert!(set.contains(&PixelFormat::GBR24P));
    }

    #[test]
    fn arrays_fill_clone_and_free() {
        let values = sample();
        let mut a = alloc_array_zeroed::<PixelFormat>(values.len()).unwrap();
        assert!(a.as_slice().iter().all(|&f| f == PixelFormat::YUV420P));
        a.as_mut_slice().copy_from_slice(&values);
        assert!(a.as_slice().get(values.len()).is_none());
        let mut u = alloc_array::<PixelFormat>(values.len()).unwrap();
        for (slot, &f) in u.as_mut_slice().iter_mut().zip(&values) {
            slot.write(f);
        }
        drop(u);
        let f = alloc_array_from_fn(values.len(), |i| values[i]).unwrap();
        let b = a.clone();
        a.as_mut_slice().fill(PixelFormat::NONE);
        drop(a);
        assert_eq!(b.as_slice(), &values[..]);
        assert_eq!(f.as_slice(), &values[..]);
        drop(alloc_array_zeroed::<PixelFormat>(0).unwrap());
        drop(HeapVec::<PixelFormat>::from_slice(&[]).unwrap());
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

    #[test]
    fn values_cross_threads_by_copy() {
        let f = PixelFormat::YUV420P10;
        let names: Vec<String> = thread::scope(|s| {
            let hs: Vec<_> = (0..4).map(|_| s.spawn(move || format!("{f:?}"))).collect();
            hs.into_iter().map(|h| h.join().unwrap()).collect()
        });
        assert!(names.iter().all(|n| n == &format!("{f:?}")));
    }
}

/// [`ColorPrimaries`] and [`ColorRange`] (formats wave 0, batch 2): as values,
/// and as the plain elements of `av_malloc`-family arrays.
mod color_primaries_and_range {
    use std::thread;

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::mem::{HeapVec, alloc_array, alloc_array_from_fn, alloc_array_zeroed};
    use ffmpeg::libavutil::pixfmt::{ColorPrimaries, ColorRange};

    /// Named primaries, the H.273 gap, both `NB` values and `u32::MAX`.
    fn primaries_sample() -> Vec<ColorPrimaries> {
        [0, 1, 2, 9, 12, 13, 22, 23, 255, 256, 257, 1 << 31, u32::MAX]
            .map(|raw| ColorPrimaries::from_raw(ffi::AVColorPrimaries(raw)))
            .into()
    }

    /// Named ranges, `NB` and the far end of the encoding.
    fn range_sample() -> Vec<ColorRange> {
        [0, 1, 2, 3, 4, 1 << 31, u32::MAX]
            .map(|raw| ColorRange::from_raw(ffi::AVColorRange(raw)))
            .into()
    }

    #[test]
    fn debug_and_conversions_accept_every_value() {
        for p in primaries_sample() {
            assert!(!format!("{p:?}").is_empty());
            let raw: ffi::AVColorPrimaries = p.into();
            assert_eq!(ColorPrimaries::from(raw), p);
        }
        for r in range_sample() {
            assert!(!format!("{r:?}").is_empty());
            let raw: ffi::AVColorRange = r.into();
            assert_eq!(ColorRange::from(raw), r);
        }
    }

    #[test]
    fn primaries_arrays_fill_clone_and_free() {
        let values = primaries_sample();
        let mut a = alloc_array_zeroed::<ColorPrimaries>(values.len()).unwrap();
        assert!(a.as_slice().iter().all(|&p| p == ColorPrimaries::RESERVED0));
        a.as_mut_slice().copy_from_slice(&values);
        assert!(a.as_slice().get(values.len()).is_none());
        let mut u = alloc_array::<ColorPrimaries>(values.len()).unwrap();
        for (slot, &p) in u.as_mut_slice().iter_mut().zip(&values) {
            slot.write(p);
        }
        drop(u);
        let f = alloc_array_from_fn(values.len(), |i| values[i]).unwrap();
        let b = a.clone();
        a.as_mut_slice().fill(ColorPrimaries::default());
        drop(a);
        assert_eq!(b.as_slice(), &values[..]);
        assert_eq!(f.as_slice(), &values[..]);
        drop(alloc_array_zeroed::<ColorPrimaries>(0).unwrap());
        drop(HeapVec::<ColorPrimaries>::from_slice(&[]).unwrap());
    }

    #[test]
    fn range_arrays_fill_clone_and_free() {
        let values = range_sample();
        let mut a = alloc_array_zeroed::<ColorRange>(values.len()).unwrap();
        assert!(a.as_slice().iter().all(|&r| r == ColorRange::default()));
        a.as_mut_slice().copy_from_slice(&values);
        assert!(a.as_slice().get(values.len()).is_none());
        let mut u = alloc_array::<ColorRange>(values.len()).unwrap();
        for (slot, &r) in u.as_mut_slice().iter_mut().zip(&values) {
            slot.write(r);
        }
        drop(u);
        let f = alloc_array_from_fn(values.len(), |i| values[i]).unwrap();
        let b = a.clone();
        a.as_mut_slice().fill(ColorRange::JPEG);
        drop(a);
        assert_eq!(b.as_slice(), &values[..]);
        assert_eq!(f.as_slice(), &values[..]);
        drop(alloc_array_zeroed::<ColorRange>(0).unwrap());
        drop(HeapVec::<ColorRange>::from_slice(&[]).unwrap());
    }

    #[test]
    fn arrays_cross_and_are_shared_between_threads() {
        let primaries = primaries_sample();
        let ranges = range_sample();
        let p = HeapVec::from_slice(&primaries).unwrap();
        let r = HeapVec::from_slice(&ranges).unwrap();
        // Shared reads from several threads at once.
        thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(|| assert_eq!(p.as_slice(), &primaries[..]));
                s.spawn(|| assert_eq!(r.as_slice(), &ranges[..]));
            }
        });
        // Moved to another thread, written and freed there.
        let (back_p, back_r) = thread::spawn(move || {
            let (mut p, mut r) = (p, r);
            p.as_mut_slice().reverse();
            r.as_mut_slice().reverse();
            (p.as_slice().to_vec(), r.as_slice().to_vec())
        })
        .join()
        .unwrap();
        assert_eq!(back_p.iter().rev().copied().collect::<Vec<_>>(), primaries);
        assert_eq!(back_r.iter().rev().copied().collect::<Vec<_>>(), ranges);
    }
}

/// [`ChromaLocation`] through the safe API only: as a value, and as the plain element
/// of `av_malloc`-family arrays, the storage a C array of `enum AVChromaLocation`
/// lives in (formats wave 0, batch 1).
mod chroma_location {
    use std::thread;

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::mem::{HeapVec, alloc_array, alloc_array_from_fn, alloc_array_zeroed};
    use ffmpeg::libavutil::pixfmt::ChromaLocation;

    /// Every value C names, the values just past them and the `unsigned`
    /// extremes, which C may store in `AVFrame.chroma_location`.
    fn sample() -> Vec<ChromaLocation> {
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 1 << 31, u32::MAX - 1, u32::MAX]
            .map(|raw| ChromaLocation::from_raw(ffi::AVChromaLocation(raw)))
            .into()
    }

    #[test]
    fn debug_default_and_conversions_accept_every_value() {
        for t in sample() {
            assert!(!format!("{t:?}").is_empty());
            let raw: ffi::AVChromaLocation = t.into();
            assert_eq!(ChromaLocation::from(raw), t);
            assert_eq!(ChromaLocation::from_raw(t.into_raw()), t);
        }
        assert_eq!(ChromaLocation::default(), ChromaLocation::UNSPECIFIED);
    }

    #[test]
    fn zeroed_array_reads_default_and_takes_every_value() {
        let values = sample();
        let mut a = alloc_array_zeroed::<ChromaLocation>(values.len()).unwrap();
        assert!(a.as_slice().iter().all(|&t| t == ChromaLocation::default()));
        a.as_mut_slice().copy_from_slice(&values);
        assert_eq!(a.as_slice(), &values[..]);
        assert!(a.as_slice().get(values.len()).is_none());
    }

    #[test]
    fn uninit_and_from_fn_arrays_fill_end_to_end() {
        let values = sample();
        let mut u = alloc_array::<ChromaLocation>(values.len()).unwrap();
        for (slot, &t) in u.as_mut_slice().iter_mut().zip(&values) {
            slot.write(t);
        }
        drop(u);
        let f = alloc_array_from_fn(values.len(), |i| values[i]).unwrap();
        assert_eq!(f.as_slice(), &values[..]);
    }

    #[test]
    fn clone_is_independent_of_its_source() {
        let values = sample();
        let mut a = HeapVec::from_slice(&values).unwrap();
        let b = a.clone();
        a.as_mut_slice().fill(ChromaLocation::BOTTOM);
        drop(a);
        assert_eq!(b.as_slice(), &values[..]);
    }

    #[test]
    fn empty_arrays_allocate_and_free() {
        drop(alloc_array_zeroed::<ChromaLocation>(0).unwrap());
        drop(HeapVec::<ChromaLocation>::from_slice(&[]).unwrap());
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
}
