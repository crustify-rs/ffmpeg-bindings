//! UB tests for `libavutil/hwcontext.h`'s wrapped types, through the safe API
//! only.

/// [`DeviceType`] as a value and as the plain element of `av_malloc`-family
/// arrays, the storage a C array of `enum AVHWDeviceType` lives in (hwcontext
/// wave 0, batch 0).
///
/// [`DeviceType`]: ffmpeg_bindings::libavutil::libavutil::hwcontext::DeviceType
mod device_type {
    use std::collections::HashSet;
    use std::thread;

    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::hwcontext::DeviceType;
    use ffmpeg_bindings::libavutil::libavutil::mem::{HeapVec, alloc_array, alloc_array_from_fn, alloc_array_zeroed};

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
    #[cfg_attr(miri, ignore = "calls C")]
    fn zeroed_array_reads_none_and_takes_every_value() {
        let values = sample();
        let mut a = alloc_array_zeroed::<DeviceType>(values.len()).unwrap();
        assert!(a.as_slice().iter().all(|&t| t == DeviceType::default()));
        a.as_mut_slice().copy_from_slice(&values);
        assert_eq!(a.as_slice(), &values[..]);
        assert!(a.as_slice().get(values.len()).is_none());
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
/// [`TransferDirection`]: ffmpeg_bindings::libavutil::libavutil::hwcontext::TransferDirection
mod transfer_direction {
    use std::collections::HashSet;
    use std::thread;

    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::hwcontext::TransferDirection;

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

/// [`FrameRef::transfer_formats`] on software frames in every state the
/// safe API leaves one in: there is no context to ask, so each is refused
/// without C dereferencing the NULL reference (hwcontext wave 1, batch 2).
/// The campaign builds compile no hwcontext backend, so the safe API makes
/// no hardware frame; listings of one are covered by the `equiv` and
/// `ub_unsafe` suites' fake context. A transfer cannot read the frame it
/// writes (`compile_fail/hwcontext_transfer_*`).
///
/// [`FrameRef::transfer_formats`]: ffmpeg_bindings::libavutil::libavutil::frame::FrameRef::transfer_formats
mod frame_ref_transfer_formats {
    use std::thread;

    use ffmpeg_bindings::libavutil::libavutil::buffer::BufferReference;
    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::frame::{Frame, FrameBox};
    use ffmpeg_bindings::libavutil::libavutil::hwcontext::TransferDirection;
    use ffmpeg_bindings::libavutil::libavutil::pixfmt::PixelFormat;

    const EINVAL: i32 = -(ffi::EINVAL as i32);
    const DIRS: [TransferDirection; 2] = [TransferDirection::From, TransferDirection::To];

    /// Software frames: new, with buffers, reset, with a user reference,
    /// cloned, and with their (absent) context taken.
    fn frames() -> Vec<FrameBox> {
        let new = Frame::new().unwrap();
        let mut allocated = Frame::new().unwrap();
        let mut m = allocated.as_mut();
        m.set_pixel_format(PixelFormat::YUV420P);
        m.set_width(8);
        m.set_height(4);
        m.alloc_buffers(0).unwrap();
        let cloned = allocated.as_ref().try_clone().unwrap();
        let mut reset = allocated.as_ref().try_clone().unwrap();
        reset.as_mut().reset();
        let mut user = Frame::new().unwrap();
        user.as_mut()
            .set_opaque_ref(Some(BufferReference::alloc_zeroed(16).unwrap()));
        let mut taken = Frame::new().unwrap();
        assert!(taken.as_mut().take_hw_frames_context().is_none());
        vec![new, allocated, cloned, reset, user, taken]
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn software_frames_have_no_formats_to_list() {
        for frame in frames() {
            for dir in DIRS {
                assert_eq!(
                    frame.as_ref().transfer_formats(dir).map(|_| ()),
                    Err(EINVAL)
                );
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn listing_through_shared_handles_on_many_threads() {
        let frames = frames();
        thread::scope(|s| {
            for t in 0..4 {
                let frames = &frames;
                s.spawn(move || {
                    for (i, f) in frames.iter().enumerate() {
                        let dir = DIRS[(t + i) % 2];
                        assert!(f.as_ref().transfer_formats(dir).is_err());
                    }
                });
            }
        });
    }
}

/// The name table, the backend walk, map flags and the device constructors
/// (hwcontext wave 1, batch 1). The campaign builds compile no backend, so
/// the safe constructors fail for every device type and the contexts and
/// hardware frames are reached only by adopting test devices, in the unit
/// and equivalence tests; their compile-time guarantees (handles borrowed
/// from their owners, no `Send`, no frame handle of a hardware frame) are
/// `compile_fail` doctests on the types.
mod contexts {
    use std::ffi::CString;
    use std::thread;

    use ffmpeg_bindings::libavutil::libavutil::dict::Dict;
    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::hwcontext::{DeviceReference, DeviceType, MapFlags};

    /// Every value 0..64, and the `unsigned` extremes.
    fn types() -> impl Iterator<Item = DeviceType> {
        (0..64)
            .chain([1 << 20, i32::MAX as u32, i32::MIN as u32, u32::MAX])
            .map(|raw| DeviceType::from_raw(ffi::AVHWDeviceType(raw)))
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn names_of_every_value_are_static_strings_that_round_trip() {
        for t in types() {
            if let Some(name) = t.name() {
                assert!(!name.to_bytes().is_empty());
                assert_eq!(DeviceType::from_name(name), Some(t));
            }
        }
    }

    /// Strings of every length up to past the longest name, exactly sized
    /// heap copies so a read past the NUL would be caught, and bytes outside
    /// ASCII.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn lookups_read_only_their_string() {
        let names = types().filter_map(|t| t.name()).collect::<Vec<_>>();
        for len in 0..16 {
            for fill in [b'a', b'v', 0x7f, 0xff] {
                let s = CString::new(vec![fill; len]).unwrap();
                assert_eq!(DeviceType::from_name(&s), None);
            }
        }
        for name in names {
            let bytes = name.to_bytes();
            for cut in 0..bytes.len() {
                let prefix = CString::new(&bytes[..cut]).unwrap();
                assert_eq!(DeviceType::from_name(&prefix), None);
            }
            let mut longer = bytes.to_vec();
            longer.push(b'x');
            assert_eq!(DeviceType::from_name(&CString::new(longer).unwrap()), None);
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn the_backend_walk_ends_and_stays_ended() {
        let mut supported = DeviceType::supported();
        let walked: Vec<DeviceType> = supported.by_ref().collect();
        assert!(
            walked
                .windows(2)
                .all(|w| w[0].into_raw().0 < w[1].into_raw().0)
        );
        assert!(supported.next().is_none());
        assert!(supported.next().is_none());
        let copy = DeviceType::supported();
        assert_eq!(copy.clone().count(), walked.len());
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn constructors_fail_cleanly_for_every_value() {
        let options = Dict::from_entries([(c"a", c"1"), (c"b", c"")]).unwrap();
        for t in types() {
            assert!(DeviceReference::alloc(t).is_none() || DeviceType::supported().any(|s| s == t));
            for device in [None, Some(c""), Some(c"0"), Some(c"/dev/null")] {
                for opts in [&Dict::new(), &options] {
                    if let Err(e) = DeviceReference::create(t, device, opts) {
                        assert!(e < 0);
                    }
                }
            }
        }
    }

    /// The name lookups and the backend walk read static tables only, so
    /// any number of threads may run them.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn lookups_and_walks_from_many_threads() {
        let handles: Vec<_> = (0..8)
            .map(|i| {
                thread::spawn(move || {
                    for t in types().skip(i) {
                        if let Some(name) = t.name() {
                            assert_eq!(DeviceType::from_name(name), Some(t));
                        }
                        assert!(DeviceType::supported().all(|s| s.name().is_some()));
                        assert!(DeviceReference::alloc(t).is_none() || t.name().is_some());
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
    }

    #[test]
    fn map_flags_combine_any_bits() {
        let all = MapFlags::READ | MapFlags::WRITE | MapFlags::OVERWRITE | MapFlags::DIRECT;
        assert_eq!(all.bits(), 0xf);
        for bits in [0, 1, 0xf, 0x10, -1, i32::MIN, i32::MAX] {
            let f = MapFlags::from_bits_retain(bits);
            assert_eq!(f.bits(), bits);
            assert!(!format!("{f:?}").is_empty());
            assert_eq!((f & all) | (f - all), f);
        }
    }
}

/// [`FramesConstraints`] owned through its safe constructor: every getter and
/// setter, the owned and borrowed list forms, and the asserted `Send` and
/// `Sync` (hwcontext wave 1, batch 0).
///
/// [`FramesConstraints`]: ffmpeg_bindings::libavutil::libavutil::hwcontext::FramesConstraints
mod frames_constraints {
    use std::sync::Mutex;
    use std::thread;

    use ffmpeg_bindings::libavutil::libavutil::hwcontext::{FramesConstraints, FramesConstraintsBox};
    use ffmpeg_bindings::libavutil::libavutil::mem::HeapVec;
    use ffmpeg_bindings::libavutil::libavutil::pixfmt::PixelFormat;

    /// Format lists of every length shape: empty, one, a `NONE` inside, a
    /// `NONE` first, long.
    fn lists() -> Vec<Vec<PixelFormat>> {
        let long: Vec<_> = (0..300)
            .map(|i| PixelFormat::from_raw(ffmpeg_bindings::libavutil::libavutil::ffi::AVPixelFormat(i % 200)))
            .collect();
        vec![
            vec![],
            vec![PixelFormat::NV12],
            vec![PixelFormat::VAAPI, PixelFormat::NONE, PixelFormat::CUDA],
            vec![PixelFormat::NONE, PixelFormat::NV12],
            long,
        ]
    }

    /// The formats a list holds once set: up to its first `NONE`.
    fn stored(list: &[PixelFormat]) -> Vec<PixelFormat> {
        list.iter()
            .copied()
            .take_while(|&f| f != PixelFormat::NONE)
            .collect()
    }

    fn sw(c: &FramesConstraintsBox) -> Option<Vec<PixelFormat>> {
        c.as_ref().valid_sw_formats().map(|l| l.elems().collect())
    }

    fn hw(c: &FramesConstraintsBox) -> Option<Vec<PixelFormat>> {
        c.as_ref().valid_hw_formats().map(|l| l.elems().collect())
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn lists_are_copied_replaced_and_taken() {
        let mut c = FramesConstraints::new().unwrap();
        for list in lists() {
            c.as_mut().set_valid_sw_formats(Some(&list)).unwrap();
            c.as_mut().set_valid_hw_formats(Some(&list)).unwrap();
            assert_eq!(sw(&c), Some(stored(&list)));
            assert_eq!(hw(&c), Some(stored(&list)));
            // A borrowed list copies out without a reference into C memory.
            let view = c.as_ref().valid_sw_formats().unwrap();
            let mut copy = vec![PixelFormat::NONE; view.len()];
            assert!(view.copy_to_slice(&mut copy));
            assert_eq!(copy, stored(&list));
            let owned: HeapVec<PixelFormat> = view.to_cvec().unwrap();
            assert_eq!(owned.as_slice(), &stored(&list)[..]);
            assert_eq!(view.elem(view.len()), None);
        }
        let taken = c.as_mut().take_valid_hw_formats().unwrap();
        assert_eq!(hw(&c), None);
        drop(c);
        // A taken list outlives the constraints.
        assert_eq!(taken.as_slice().len(), 300);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn unknown_lists_and_sizes_round_trip() {
        let mut c = FramesConstraints::new().unwrap();
        let r = c.as_ref();
        assert_eq!((r.min_width(), r.min_height()), (0, 0));
        assert_eq!((r.max_width(), r.max_height()), (i32::MAX, i32::MAX));
        for v in [i32::MIN, -1, 0, 1, i32::MAX] {
            let mut m = c.as_mut();
            m.set_min_width(v);
            m.set_min_height(v);
            m.set_max_width(v);
            m.set_max_height(v);
            let r = c.as_ref();
            assert_eq!(
                [r.min_width(), r.min_height(), r.max_width(), r.max_height()],
                [v; 4]
            );
        }
        c.as_mut()
            .set_valid_sw_formats(Some(&[PixelFormat::NV12]))
            .unwrap();
        c.as_mut().set_valid_sw_formats(None).unwrap();
        assert_eq!(sw(&c), None);
        assert!(c.as_mut().take_valid_sw_formats().is_none());
        let debug = format!("{:?}", c.as_ref());
        assert!(debug.contains("valid_sw_formats: None"), "{debug}");
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn many_boxes_drop_in_any_order() {
        let mut boxes: Vec<_> = lists()
            .iter()
            .map(|l| {
                let mut c = FramesConstraints::new().unwrap();
                c.as_mut().set_valid_hw_formats(Some(l)).unwrap();
                c.as_mut().set_valid_sw_formats(Some(l)).unwrap();
                c
            })
            .collect();
        let taken: Vec<_> = boxes
            .iter_mut()
            .filter_map(|c| c.as_mut().take_valid_sw_formats())
            .collect();
        boxes.reverse();
        drop(boxes);
        assert_eq!(taken.len(), lists().len());
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn boxes_and_handles_cross_threads() {
        let mut c = FramesConstraints::new().unwrap();
        c.as_mut()
            .set_valid_hw_formats(Some(&[PixelFormat::VAAPI]))
            .unwrap();
        c.as_mut().set_valid_sw_formats(Some(&lists()[4])).unwrap();
        let expect = sw(&c);
        // Shared readers on many threads, through the box and copied handles.
        let r = c.as_ref();
        thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(|| assert_eq!(sw(&c), expect));
                s.spawn(move || {
                    assert_eq!(
                        r.valid_hw_formats().unwrap().elem(0),
                        Some(PixelFormat::VAAPI)
                    )
                });
            }
        });
        // A box moved to, written and dropped on other threads.
        let shared = Mutex::new(c);
        thread::scope(|s| {
            for i in 0..4 {
                let shared = &shared;
                s.spawn(move || {
                    let mut c = shared.lock().unwrap();
                    c.as_mut().set_min_width(i);
                    c.as_mut()
                        .set_valid_sw_formats(Some(&lists()[i as usize]))
                        .unwrap();
                });
            }
        });
        let c = shared.into_inner().unwrap();
        thread::spawn(move || drop(c)).join().unwrap();
    }
}
