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
