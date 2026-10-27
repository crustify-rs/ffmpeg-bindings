//! UB tests for `libavutil/frame.h`'s wrapped types, through the safe API
//! only.
#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "test inputs are chosen within range, and the casts mirror the C conversions under comparison"
)]
#![expect(
    clippy::many_single_char_names,
    clippy::similar_names,
    reason = "names follow the C fields, formulas and fixtures they mirror"
)]
#![expect(
    clippy::assert_is_empty,
    let_underscore_drop,
    reason = "assertions keep the form of the C behaviour they check"
)]

/// [`SideDataType`] as a value and as the plain element of `av_malloc`-family
/// arrays, the storage a C array of `enum AVFrameSideDataType` lives in
/// (buffer-frame wave 0, batch 2).
///
/// [`SideDataType`]: ffmpeg_bindings::libavutil::libavutil::frame::SideDataType
mod side_data_type {
    use std::collections::HashSet;
    use std::thread;

    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::frame::SideDataType;
    use ffmpeg_bindings::libavutil::libavutil::mem::{
        HeapVec, alloc_array, alloc_array_from_fn, alloc_array_zeroed,
    };

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
        .map(SideDataType::from_raw)
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
    #[cfg_attr(miri, ignore = "calls C")]
    fn zeroed_array_reads_pan_scan_and_takes_every_value() {
        let values = sample();
        let mut a = alloc_array_zeroed::<SideDataType>(values.len()).unwrap();
        assert!(a.as_slice().iter().all(|&t| t == SideDataType::PAN_SCAN));
        a.as_mut_slice().copy_from_slice(&values);
        assert_eq!(a.as_slice(), &values[..]);
        assert!(a.as_slice().get(values.len()).is_none());
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
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
        let t = thread::spawn(|| SideDataType::ICC_PROFILE).join().unwrap();
        assert_eq!(t, SideDataType::ICC_PROFILE);
    }
}

/// `ActiveFormatDescription` (buffer-frame wave 0, batch 0): every named,
/// reserved and out-of-byte value through each conversion, `av_malloc`-family
/// arrays of it, and sharing and moving it across threads.
mod active_format_description {
    use std::thread;

    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::frame::ActiveFormatDescription as Afd;
    use ffmpeg_bindings::libavutil::libavutil::mem::{alloc_array_from_fn, alloc_array_zeroed};

    /// Raw values at every edge: each byte, the first values past a byte, and
    /// the `unsigned int` limits a C `int`'s negative values map onto.
    fn raws() -> impl Iterator<Item = u32> {
        (0..=0x101).chain([i32::MAX as u32, i32::MIN as u32, u32::MAX - 1, u32::MAX])
    }

    #[test]
    fn every_raw_value_converts_and_formats() {
        for raw in raws() {
            let afd = Afd::from_raw(raw);
            assert_eq!(afd.into_raw(), raw);
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
    #[cfg_attr(miri, ignore = "calls C")]
    fn heap_arrays_read_any_stored_value() {
        let bytes = alloc_array_from_fn(256, |i| Afd::from_byte(i as u8)).unwrap();
        let back: Vec<Option<u8>> = bytes.as_slice().iter().map(|a| a.to_byte()).collect();
        assert_eq!(back, (0..=u8::MAX).map(Some).collect::<Vec<_>>());

        let mut zeroed = alloc_array_zeroed::<Afd>(3).unwrap();
        assert!(zeroed.as_slice().iter().all(|&a| a == Afd::from_byte(0)));
        zeroed.as_mut_slice()[1] = Afd::SP_4_3;
        zeroed.as_mut_slice()[2] = Afd::from_raw(u32::MAX);
        let copy = zeroed.clone();
        drop(zeroed);
        assert_eq!(copy.as_slice()[1], Afd::SP_4_3);
        assert_eq!(copy.as_slice()[2].to_byte(), None);
    }

    #[test]
    fn values_cross_threads() {
        let all: Vec<Afd> = raws().map(Afd::from_raw).collect();
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

/// `SideDataDescriptor` and `SideDataProps`: inline values, `av_malloc`-family
/// owners and arrays of descriptors, every handle form, names from literals
/// and from exactly sized heap allocations (so a read past the NUL is an
/// `ASan` report), names that outlive the descriptor they were read from,
/// copies outliving their source, and the asserted `Send` / `Sync`. Tests
/// without `heap` in their name make no C call and also run under Miri
/// (buffer-frame wave 0, batch 4).
mod side_data_descriptor {
    use core::ffi::{CStr, c_uint};
    use std::ffi::CString;
    use std::sync::Mutex;
    use std::thread;

    use ffmpeg_bindings::libavutil::libavutil::frame::{
        SideDataDescriptor, SideDataDescriptorMut, SideDataDescriptorRef, SideDataProps,
    };
    use ffmpeg_bindings::libavutil::libavutil::mem::{HeapBox, alloc_cell_array_zeroed};

    /// `bytes` as an exactly sized heap C string that lives for the rest of
    /// the program, kept reachable from a static so that neither
    /// `LeakSanitizer` nor Miri reports it.
    fn leaked(bytes: &[u8]) -> &'static CStr {
        static KEEP: Mutex<Vec<&'static CStr>> = Mutex::new(Vec::new());
        let s: &'static CStr = Box::leak(CString::new(bytes).unwrap().into_boxed_c_str());
        KEEP.lock().unwrap().push(s);
        s
    }

    /// Property bits at every edge: none, named, unnamed, the sign bit, all.
    const PROPS: [c_uint; 5] = [0, 0x1F, 0x20, 0x8000_0000, c_uint::MAX];

    /// Everything the getters read, the name by content.
    fn snapshot(d: SideDataDescriptorRef<'_>) -> (Option<Vec<u8>>, c_uint) {
        (d.name().map(|s| s.to_bytes().to_vec()), d.props().bits())
    }

    fn fill(m: &mut SideDataDescriptorMut<'_>, name: Option<&'static CStr>, k: usize) {
        m.set_name(name);
        m.set_props(SideDataProps::from_bits_retain(PROPS[k % PROPS.len()]));
    }

    #[test]
    fn every_field_edge_builds_and_reads_back() {
        let names = [
            None,
            Some(c""),
            Some(c"Stereo 3D"),
            Some(leaked(b"")),
            Some(leaked(b"\xff\xfe")),
            Some(leaked(&vec![b'n'; 4096])),
        ];
        for (k, name) in names.into_iter().enumerate() {
            let mut d = SideDataDescriptor::new();
            fill(&mut d.as_mut(), name, k);
            let s = snapshot(d.as_ref());
            assert_eq!(s.0.as_deref(), name.map(CStr::to_bytes));
            assert_eq!(s.1, PROPS[k % PROPS.len()]);
            assert!(!format!("{d:?}").is_empty());
            let raw = d.into_raw();
            assert_eq!(raw.name, name.map_or(core::ptr::null(), CStr::as_ptr));
            if let Some(name) = name {
                let n = SideDataDescriptor::named(name, d.as_ref().props());
                assert_eq!(n, d);
            }
        }
        assert_eq!(
            snapshot(SideDataDescriptor::default().as_ref()),
            snapshot(SideDataDescriptor::new().as_ref())
        );
    }

    #[test]
    fn names_outlive_the_descriptor_they_were_read_from() {
        let name = leaked(b"Mastering display metadata");
        let n = {
            let d = SideDataDescriptor::named(name, SideDataProps::GLOBAL);
            d.as_ref().name()
            // `d` goes out of scope; the string is the program's.
        };
        assert_eq!(n.unwrap().to_bytes(), b"Mastering display metadata");
        // And moves into another descriptor.
        let mut e = SideDataDescriptor::new();
        e.as_mut().set_name(n);
        assert_eq!(e.as_ref().name().unwrap().as_ptr(), name.as_ptr());
    }

    #[test]
    fn shared_handles_copy_and_coexist() {
        let d = SideDataDescriptor::named(c"AFD", SideDataProps::MULTI);
        let a = d.as_ref();
        let b = a;
        assert_eq!(snapshot(a), snapshot(b));
        assert_eq!(a.get(), b.get());
        assert_eq!(format!("{a:?}"), format!("{b:?}"));
    }

    #[test]
    fn exclusive_handle_reborrows_without_aliasing() {
        let mut v = SideDataDescriptor::new();
        let mut m = v.as_mut();
        fill(&mut m.as_mut(), Some(c"Skip samples"), 1);
        {
            let mut inner = m.as_mut();
            inner.set_props(SideDataProps::SIZE_DEPENDENT);
            let shared = inner.as_ref();
            assert_eq!(shared.name(), Some(c"Skip samples"));
        }
        let shared = m.as_ref();
        assert_eq!(shared.props(), SideDataProps::SIZE_DEPENDENT);
        m.set_name(None);
        m.set(SideDataDescriptor::named(
            c"GOP timecode",
            SideDataProps::empty(),
        ));
        assert_eq!(v.as_ref().name(), Some(c"GOP timecode"));
        assert!(v.as_ref().props().is_empty());
    }

    #[test]
    fn copies_and_whole_sets_are_independent_of_their_source() {
        let mut a = SideDataDescriptor::new();
        fill(&mut a.as_mut(), Some(leaked(b"Film grain parameters")), 4);
        let copy = a.as_ref().get();
        a.as_mut().set_props(SideDataProps::empty());
        a.as_mut().set_name(None);
        assert_eq!(copy.as_ref().props().bits(), c_uint::MAX);
        assert_eq!(
            copy.as_ref().name().unwrap().to_bytes(),
            b"Film grain parameters"
        );
        let mut b = SideDataDescriptor::new();
        b.as_mut().set(copy);
        assert_eq!(b, copy);
        #[expect(clippy::clone_on_copy, reason = "exercises the explicit `Clone` impl")]
        let c = Clone::clone(&b);
        assert_eq!(snapshot(c.as_ref()), snapshot(copy.as_ref()));
        assert_ne!(a, copy);
    }

    #[test]
    fn props_sets_compose_on_many_threads() {
        let all = SideDataProps::all();
        thread::scope(|s| {
            for bit in 0..8 {
                s.spawn(move || {
                    let p = SideDataProps::from_bits_retain(1 << bit);
                    let mut q = p;
                    q |= SideDataProps::GLOBAL;
                    q -= SideDataProps::GLOBAL;
                    assert_eq!(q.bits(), (1 << bit) & !SideDataProps::GLOBAL.bits());
                    assert_eq!(SideDataProps::from_bits(p.bits()).is_some(), bit < 5);
                    assert_eq!(all.intersects(p), bit < 5);
                });
            }
        });
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn heap_box_owner_lives_reads_writes_clones_and_frees() {
        let mut a = HeapBox::<SideDataDescriptor>::new_zeroed().unwrap();
        assert_eq!(
            snapshot(a.as_ref()),
            snapshot(SideDataDescriptor::new().as_ref())
        );
        for k in 0..PROPS.len() {
            fill(
                &mut a.as_mut(),
                Some(leaked(format!("type{k}").as_bytes())),
                k,
            );
        }
        let b = a.clone();
        let c = b.try_clone().unwrap();
        let name = a.as_ref().name();
        let copy = a.as_ref().get();
        // The source may go first: the copies own their own blocks, and the
        // strings are the program's.
        drop(a);
        assert_eq!(name.unwrap().to_bytes(), b"type4");
        assert_eq!(snapshot(b.as_ref()), snapshot(copy.as_ref()));
        drop(b);
        assert_eq!(snapshot(c.as_ref()), snapshot(copy.as_ref()));
        assert_eq!(copy.as_ref().name(), name);
        drop(HeapBox::<SideDataDescriptor>::new_uninit().unwrap());
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn heap_array_handles_stay_in_bounds() {
        let n = 5;
        let mut arr = alloc_cell_array_zeroed::<SideDataDescriptor>(n).unwrap();
        {
            let mut handles = arr.as_handles_mut();
            for k in 0..n {
                fill(
                    &mut handles.get_mut(k).unwrap(),
                    Some(leaked(&vec![b'a' + k as u8; k])),
                    k,
                );
            }
            assert!(handles.get_mut(n).is_none());
        }
        for (k, h) in arr.as_handles().iter().enumerate() {
            assert_eq!(h.name().unwrap().to_bytes(), vec![b'a' + k as u8; k]);
            assert_eq!(h.props().bits(), PROPS[k]);
        }
        assert!(arr.as_handles().get(n).is_none());
        let copy = arr.clone();
        drop(arr);
        assert_eq!(
            copy.as_handles().get(4).unwrap().props().bits(),
            c_uint::MAX
        );
        drop(alloc_cell_array_zeroed::<SideDataDescriptor>(0).unwrap());
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn heap_values_and_owners_move_across_threads() {
        let v = SideDataDescriptor::named(leaked(b"View ID"), SideDataProps::MULTI);
        let b = HeapBox::<SideDataDescriptor>::new_zeroed().unwrap();
        let (v, b) = thread::spawn(move || {
            let mut b = b;
            b.as_mut().set(v);
            (v, b)
        })
        .join()
        .unwrap();
        assert_eq!(b.as_ref().get(), v);
        // Freed on another thread than the one that allocated it.
        thread::spawn(move || drop(b)).join().unwrap();
    }

    #[test]
    fn shared_handles_read_concurrently() {
        let v = SideDataDescriptor::named(leaked(b"LCEVC NAL data"), SideDataProps::SIZE_DEPENDENT);
        let expect = snapshot(v.as_ref());
        let r = v.as_ref();
        thread::scope(|s| {
            for _ in 0..4 {
                let expect = &expect;
                s.spawn(move || {
                    for _ in 0..200 {
                        assert_eq!(&snapshot(r), expect);
                    }
                });
            }
        });
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn heap_shared_handles_read_concurrently() {
        let mut b = HeapBox::<SideDataDescriptor>::new_zeroed().unwrap();
        fill(&mut b.as_mut(), Some(c"ICC profile"), 1);
        let r = b.as_ref();
        thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(move || {
                    for _ in 0..200 {
                        assert_eq!(r.name(), Some(c"ICC profile"));
                        assert_eq!(r.get().as_ref().props(), SideDataProps::all());
                    }
                });
            }
        });
    }

    #[test]
    fn exclusive_handles_write_on_other_threads() {
        let mut values = [SideDataDescriptor::new(); 4];
        thread::scope(|s| {
            for (i, v) in values.iter_mut().enumerate() {
                let mut m = v.as_mut();
                s.spawn(move || {
                    m.set_props(SideDataProps::from_bits_retain(1 << i));
                    m.set_name(Some(c"x"));
                });
            }
        });
        let bits: Vec<c_uint> = values.iter().map(|v| v.as_ref().props().bits()).collect();
        assert_eq!(bits, [1, 2, 4, 8]);
        assert!(values.iter().all(|v| v.as_ref().name() == Some(c"x")));
    }

    #[test]
    fn a_shared_exclusive_handle_only_reads() {
        let mut v = SideDataDescriptor::named(c"AVReplayGain", SideDataProps::GLOBAL);
        let m = v.as_mut();
        let shared_m = &m;
        thread::scope(|s| {
            for _ in 0..2 {
                s.spawn(move || {
                    assert_eq!(shared_m.as_ref().props(), SideDataProps::GLOBAL);
                });
            }
        });
    }
}

/// `RegionOfInterest`: inline values and `av_malloc`-family arrays of them
/// (the shape of `REGIONS_OF_INTEREST` side data), every field at its edges,
/// the offset's handles and their reborrows, copies outliving their source,
/// and the asserted `Send` / `Sync`. Tests without `heap` in their name make
/// no C call and also run under Miri (buffer-frame wave 1, batch 0).
mod region_of_interest {
    use std::thread;

    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::frame::{
        RegionOfInterest, RegionOfInterestMut, RegionOfInterestRef,
    };
    use ffmpeg_bindings::libavutil::libavutil::mem::{HeapVec, alloc_cell_array_zeroed};
    use ffmpeg_bindings::libavutil::libavutil::rational::Rational;

    const EDGES: [i32; 5] = [i32::MIN, -1, 0, 1, i32::MAX];

    fn q(num: i32, den: i32) -> Rational {
        Rational::from_raw(ffi::AVRational { num, den })
    }

    /// Everything the getters read.
    fn snapshot(r: RegionOfInterestRef<'_>) -> (u32, [i32; 6]) {
        let o = r.qoffset();
        (
            r.self_size(),
            [r.top(), r.bottom(), r.left(), r.right(), o.num(), o.den()],
        )
    }

    fn fill(m: &mut RegionOfInterestMut<'_>, k: usize) {
        let e = |i: usize| EDGES[(k + i) % EDGES.len()];
        m.set_self_size(e(0) as u32);
        m.set_top(e(1));
        m.set_bottom(e(2));
        m.set_left(e(3));
        m.set_right(e(4));
        if k.is_multiple_of(2) {
            m.set_qoffset(q(e(5), e(6)));
        } else {
            let mut o = m.qoffset_mut();
            o.set_num(e(5));
            o.set_den(e(6));
        }
    }

    fn expected(k: usize) -> (u32, [i32; 6]) {
        let e = |i: usize| EDGES[(k + i) % EDGES.len()];
        (e(0) as u32, [e(1), e(2), e(3), e(4), e(5), e(6)])
    }

    #[test]
    fn every_field_edge_builds_and_reads_back() {
        for k in 0..2 * EDGES.len() {
            let mut r = RegionOfInterest::new(1, 2, 3, 4, q(-1, 10));
            fill(&mut r.as_mut(), k);
            assert_eq!(snapshot(r.as_ref()), expected(k));
            let copy = r.as_ref().get();
            r.as_mut().set(RegionOfInterest::default());
            assert_eq!(snapshot(copy.as_ref()), expected(k));
            assert_eq!(
                snapshot(r.as_ref()),
                (RegionOfInterest::SELF_SIZE, [0, 0, 0, 0, 0, 1])
            );
            let raw = copy.into_raw();
            assert_eq!(
                snapshot(RegionOfInterest::from_raw(raw).as_ref()),
                expected(k)
            );
        }
    }

    #[test]
    fn offset_handles_reborrow_and_outlive_nothing() {
        let mut r = RegionOfInterest::default();
        let mut m = r.as_mut();
        {
            let mut again = m.as_mut();
            let mut o = again.qoffset_mut();
            let mut o2 = o.as_mut();
            o2.set_num(3);
            assert_eq!(o.as_ref().num(), 3);
        }
        let shared = m.as_ref();
        let (o, copy) = (shared.qoffset(), shared);
        assert_eq!((o.num(), copy.qoffset().den()), (3, 1));
        // A getter's offset handle lives as long as the region's handle.
        let o = r.as_ref().qoffset();
        assert_eq!(o.num(), 3);
    }

    #[test]
    fn values_and_handles_cross_threads() {
        let regions: Vec<RegionOfInterest> = (0..8)
            .map(|k| {
                let mut r = RegionOfInterest::default();
                fill(&mut r.as_mut(), k);
                r
            })
            .collect();
        thread::scope(|s| {
            for (k, r) in regions.iter().enumerate() {
                let h = r.as_ref();
                s.spawn(move || assert_eq!(snapshot(h), expected(k)));
            }
        });
        let mut moved = regions;
        thread::scope(|s| {
            for (k, r) in moved.iter_mut().enumerate() {
                let mut h = r.as_mut();
                s.spawn(move || {
                    fill(&mut h, k + 1);
                    assert_eq!(snapshot(h.as_ref()), expected(k + 1));
                });
            }
        });
        let handles: Vec<_> = moved
            .into_iter()
            .enumerate()
            .map(|(k, r)| thread::spawn(move || assert_eq!(snapshot(r.as_ref()), expected(k + 1))))
            .collect();
        for h in handles {
            h.join().unwrap();
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn heap_arrays_of_regions_fill_and_read_element_by_element() {
        for n in [0, 1, 7, 64] {
            let mut array =
                alloc_cell_array_zeroed::<RegionOfInterest>(n).unwrap_or_else(HeapVec::empty);
            assert_eq!(array.len(), n);
            let mut handles = array.as_handles_mut();
            for k in 0..n {
                assert_eq!(snapshot(handles.get(k).unwrap()), (0, [0; 6]));
                fill(&mut handles.get_mut(k).unwrap(), k);
            }
            assert!(handles.get(n).is_none() && handles.get_mut(n).is_none());
            for (k, r) in array.as_handles().iter().enumerate() {
                assert_eq!(snapshot(r), expected(k));
            }
            let copies: Vec<RegionOfInterest> =
                array.as_handles().iter().map(|r| r.get()).collect();
            drop(array);
            for (k, r) in copies.iter().enumerate() {
                assert_eq!(snapshot(r.as_ref()), expected(k));
            }
        }
    }
}

/// [`Frame`] (buffer-frame wave 3, batch 0): every getter and setter of a
/// new frame and of one built through the safe API; planes over exactly
/// sized buffers, so that a read or write past a view is an `ASan` report;
/// writes through sole, shared and read-only buffers; buffer, extra buffer,
/// `extended_data`, side data, metadata, user data and channel layout
/// changes with the old owners outliving the frame; clones outliving their
/// source; and frames read, cloned, written and dropped across threads.
///
/// [`Frame`]: ffmpeg_bindings::libavutil::libavutil::frame::Frame
mod frame_type {
    use std::sync::{Arc, Barrier};
    use std::thread;

    use ffmpeg_bindings::libavutil::libavutil::buffer::{BufferReference, BufferReferenceBox};
    use ffmpeg_bindings::libavutil::libavutil::channel_layout::ChannelLayout;
    use ffmpeg_bindings::libavutil::libavutil::dict::Dict;
    use ffmpeg_bindings::libavutil::libavutil::frame::{
        DecodeErrorFlags, Frame, FrameBox, FrameFlags, NUM_DATA_POINTERS, PlaneError, SideDataType,
    };
    use ffmpeg_bindings::libavutil::libavutil::pixdesc::{
        AlphaMode, ChromaLocation, ColorPrimaries, ColorRange, ColorSpace, ColorTransfer,
        PixelFormat,
    };
    use ffmpeg_bindings::libavutil::libavutil::rational::Rational;
    use ffmpeg_bindings::libavutil::libavutil::samplefmt::SampleFormat;
    use ffmpeg_bindings::libavutil::libavutil::side_data::SideDataFlags;
    use ffmpeg_bindings::libavutil::libavutil::utils::PictureType;

    const NOPTS: i64 = i64::MIN;

    /// A buffer of exactly `len` bytes, each its index times 3, plus 1.
    fn bytes(len: usize) -> BufferReferenceBox {
        let data: Vec<u8> = (0..len).map(|i| (i * 3 + 1) as u8).collect();
        BufferReference::alloc(len).unwrap().copy_from_slice(&data)
    }

    /// A video-shaped frame: one buffer of `len` bytes at index 0, planes
    /// 0..3 at a quarter of it each.
    fn planar(len: usize) -> FrameBox {
        let mut f = Frame::new().unwrap();
        let mut m = f.as_mut();
        m.set_buffer(0, Some(bytes(len)));
        for i in 0..3 {
            m.set_plane(i, 0, i * len / 4).unwrap();
            m.set_linesize(i, (len / 4) as i32);
        }
        m.set_pixel_format(PixelFormat::YUV444P);
        m.set_width(1);
        m.set_height(1);
        f
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn new_frames_read_cs_defaults_everywhere() {
        let f = Frame::new().unwrap();
        let r = f.as_ref();
        for i in 0..=NUM_DATA_POINTERS {
            assert!(r.plane(i).is_none() && r.buffer(i).is_none());
            assert!(r.extended_plane(i).is_none());
        }
        assert!(r.plane(usize::MAX).is_none() && r.buffer(usize::MAX).is_none());
        assert_eq!(r.linesizes(), [0; NUM_DATA_POINTERS]);
        assert_eq!(r.extended_plane_count(), NUM_DATA_POINTERS);
        assert!(!r.has_extended_data());
        assert_eq!((r.width(), r.height(), r.sample_count()), (0, 0, 0));
        assert_eq!(r.format(), -1);
        assert_eq!(r.pixel_format(), PixelFormat::NONE);
        assert_eq!(r.sample_format(), SampleFormat::NONE);
        assert_eq!(r.picture_type(), PictureType::NONE);
        assert_eq!(r.sample_aspect_ratio().get().into_raw().num, 0);
        assert_eq!(r.sample_aspect_ratio().get().into_raw().den, 1);
        assert_eq!(r.time_base().get().into_raw().den, 1);
        assert_eq!(
            (r.pts(), r.packet_dts(), r.best_effort_timestamp()),
            (NOPTS, NOPTS, NOPTS)
        );
        assert_eq!((r.quality(), r.repeat_pict(), r.sample_rate()), (0, 0, 0));
        assert!(r.opaque().is_null());
        assert_eq!(r.extended_buffer_count(), 0);
        assert!(r.extended_buffer(0).is_none());
        assert!(r.side_data().is_empty());
        assert!(r.flags().is_empty() && r.decode_error_flags().is_empty());
        assert_eq!(r.color_range(), ColorRange::UNSPECIFIED);
        assert_eq!(r.color_primaries(), ColorPrimaries::UNSPECIFIED);
        assert_eq!(r.color_transfer(), ColorTransfer::UNSPECIFIED);
        assert_eq!(r.color_space(), ColorSpace::UNSPECIFIED);
        assert_eq!(r.chroma_location(), ChromaLocation::UNSPECIFIED);
        assert_eq!(r.alpha_mode(), AlphaMode::UNSPECIFIED);
        assert!(r.metadata().is_none() && r.hw_frames_context().is_none());
        assert!(r.opaque_ref().is_none() && !r.has_private_ref());
        assert_eq!(
            (r.crop_top(), r.crop_bottom(), r.crop_left(), r.crop_right()),
            (0, 0, 0, 0)
        );
        assert_eq!(r.channel_layout().channel_count(), 0);
        assert_eq!(r.duration(), 0);
        assert!(!format!("{r:?}").is_empty());
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn scalar_setters_round_trip_their_edges() {
        let mut f = Frame::new().unwrap();
        for v in [i32::MIN, -1, 0, 1, i32::MAX] {
            let mut m = f.as_mut();
            m.set_width(v);
            m.set_height(v);
            m.set_sample_count(v);
            m.set_format(v);
            m.set_quality(v);
            m.set_repeat_pict(v);
            m.set_sample_rate(v);
            for i in 0..NUM_DATA_POINTERS {
                m.set_linesize(i, v.wrapping_add(i as i32));
            }
            m.set_flags(FrameFlags::from_bits_retain(v));
            m.set_decode_error_flags(DecodeErrorFlags::from_bits_retain(v));
            let r = f.as_ref();
            assert_eq!(
                [r.width(), r.height(), r.sample_count(), r.format()],
                [v; 4]
            );
            assert_eq!([r.quality(), r.repeat_pict(), r.sample_rate()], [v; 3]);
            assert_eq!(r.linesizes()[7], v.wrapping_add(7));
            assert_eq!(r.flags().bits(), v);
            assert_eq!(r.decode_error_flags().bits(), v);
            assert_eq!(r.pixel_format().into_raw(), v);
            assert_eq!(r.sample_format().into_raw(), v);
        }
        for v in [i64::MIN, -1, 0, i64::MAX] {
            let mut m = f.as_mut();
            m.set_pts(v);
            m.set_packet_dts(v);
            m.set_best_effort_timestamp(v);
            m.set_duration(v);
            let r = f.as_ref();
            assert_eq!(
                [
                    r.pts(),
                    r.packet_dts(),
                    r.best_effort_timestamp(),
                    r.duration()
                ],
                [v; 4]
            );
        }
        for v in [0, 1, usize::MAX] {
            let mut m = f.as_mut();
            m.set_crop_top(v);
            m.set_crop_bottom(v);
            m.set_crop_left(v);
            m.set_crop_right(v);
            let r = f.as_ref();
            assert_eq!(
                [r.crop_top(), r.crop_bottom(), r.crop_left(), r.crop_right()],
                [v; 4]
            );
        }
        let mut m = f.as_mut();
        m.set_linesizes([3; NUM_DATA_POINTERS]);
        m.set_pixel_format(PixelFormat::RGB24);
        m.set_picture_type(PictureType::B);
        m.set_sample_aspect_ratio(Rational::new(16, 9));
        m.set_time_base(Rational::new(1, 90000));
        m.time_base_mut().set_den(48000);
        m.sample_aspect_ratio_mut().set_num(4);
        m.set_color_range(ColorRange::JPEG);
        m.set_color_primaries(ColorPrimaries::BT709);
        m.set_color_transfer(ColorTransfer::SMPTE2084);
        m.set_color_space(ColorSpace::BT2020_NCL);
        m.set_chroma_location(ChromaLocation::TOPLEFT);
        m.set_alpha_mode(AlphaMode::PREMULTIPLIED);
        let mut local = 7u8;
        m.set_opaque((&raw mut local).cast());
        let r = f.as_ref();
        assert_eq!(r.linesizes(), [3; NUM_DATA_POINTERS]);
        assert_eq!(r.pixel_format(), PixelFormat::RGB24);
        assert_eq!(r.picture_type(), PictureType::B);
        assert_eq!(r.sample_aspect_ratio().get().into_raw().num, 4);
        assert_eq!(r.time_base().get().into_raw().den, 48000);
        assert_eq!(r.color_range(), ColorRange::JPEG);
        assert_eq!(r.color_primaries(), ColorPrimaries::BT709);
        assert_eq!(r.color_transfer(), ColorTransfer::SMPTE2084);
        assert_eq!(r.color_space(), ColorSpace::BT2020_NCL);
        assert_eq!(r.chroma_location(), ChromaLocation::TOPLEFT);
        assert_eq!(r.alpha_mode(), AlphaMode::PREMULTIPLIED);
        assert_eq!(r.opaque(), (&raw mut local).cast());
        f.as_mut().set_sample_format(SampleFormat::FLTP);
        assert_eq!(f.as_ref().sample_format(), SampleFormat::FLTP);
        f.as_mut().set_opaque(core::ptr::null_mut());
        drop(f);
        assert_eq!(local, 7);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn planes_read_exactly_to_their_buffers_end() {
        for len in [1usize, 4, 7, 64, 4096] {
            let mut f = Frame::new().unwrap();
            let mut m = f.as_mut();
            m.set_buffer(0, Some(bytes(len)));
            m.set_plane(0, 0, 0).unwrap();
            m.set_plane(1, 0, len - 1).unwrap();
            assert_eq!(m.set_plane(2, 0, len), Err(PlaneError::OutOfBounds));
            assert_eq!(m.set_plane(2, 0, usize::MAX), Err(PlaneError::OutOfBounds));
            assert_eq!(
                m.set_plane(NUM_DATA_POINTERS, 0, 0),
                Err(PlaneError::NoPlane)
            );
            assert_eq!(m.set_plane(2, 1, 0), Err(PlaneError::NoBuffer));
            assert_eq!(
                m.set_plane(2, NUM_DATA_POINTERS, 0),
                Err(PlaneError::NoBuffer)
            );
            let r = f.as_ref();
            let whole = r.plane(0).unwrap();
            assert_eq!(whole.len(), len);
            assert_eq!(whole, r.buffer(0).unwrap().data());
            assert_eq!(whole[len - 1], ((len - 1) * 3 + 1) as u8);
            assert_eq!(r.plane(1).unwrap(), &whole[len - 1..]);
            assert_eq!(r.extended_plane(1).unwrap(), &whole[len - 1..]);
            assert!(r.plane(2).is_none());
            f.as_mut().clear_plane(1);
            f.as_mut().clear_plane(NUM_DATA_POINTERS);
            assert!(f.as_ref().plane(1).is_none());
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn sole_planes_are_written_in_full() {
        let mut f = planar(64);
        let mut m = f.as_mut();
        for i in 0..3 {
            let p = m.plane_mut(i).expect("a sole buffer");
            assert_eq!(p.len(), 64 - i * 16);
            p.fill(i as u8 + 1);
        }
        let all = f.as_ref().plane(0).unwrap();
        assert_eq!(&all[..16], &[1; 16][..]);
        assert_eq!(&all[16..32], &[2; 16][..]);
        assert_eq!(&all[32..], &[3; 32][..]);
        assert!(f.as_mut().plane_mut(3).is_none());
        assert!(f.as_mut().plane_mut(NUM_DATA_POINTERS).is_none());
        assert!(f.as_mut().extended_plane_mut(0).is_some());
        assert!(f.as_mut().extended_plane_mut(NUM_DATA_POINTERS).is_none());
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn shared_and_read_only_buffers_refuse_writes() {
        let mut f = planar(32);
        let copy = f.as_ref().buffer(0).unwrap().try_clone().unwrap();
        assert!(f.as_mut().plane_mut(0).is_none());
        drop(copy);
        assert!(f.as_mut().plane_mut(0).is_some());
        let clone = f.as_ref().try_clone().unwrap();
        assert!(f.as_mut().plane_mut(1).is_none());
        assert_eq!(clone.as_ref().plane(1), f.as_ref().plane(1));
        drop(clone);
        assert!(f.as_mut().plane_mut(1).is_some());
        let ro = BufferReference::from_read_only(vec![5u8; 12]).unwrap();
        f.as_mut().set_buffer(0, Some(ro));
        f.as_mut().set_plane(0, 0, 2).unwrap();
        assert_eq!(f.as_ref().plane(0).unwrap(), &[5u8; 10][..]);
        assert!(f.as_mut().plane_mut(0).is_none());
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn removed_buffers_outlive_the_frame_and_take_their_planes() {
        let mut f = planar(48);
        let old = f.as_mut().replace_buffer(0, Some(bytes(8))).unwrap();
        for i in 0..3 {
            assert!(f.as_ref().plane(i).is_none());
        }
        f.as_mut().set_plane(0, 0, 7).unwrap();
        let taken = f.as_mut().take_buffer(0).unwrap();
        assert!(f.as_ref().plane(0).is_none() && f.as_ref().buffer(0).is_none());
        assert_eq!(f.as_mut().set_plane(0, 0, 0), Err(PlaneError::NoBuffer));
        assert!(f.as_mut().take_buffer(0).is_none());
        drop(f);
        assert_eq!(old.as_ref().len(), 48);
        assert_eq!(taken.as_ref().data()[7], 22);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn extra_buffers_back_planes_of_many_channels() {
        let mut f = Frame::new().unwrap();
        let mut m = f.as_mut();
        m.set_sample_format(SampleFormat::U8P);
        m.set_sample_count(4);
        m.set_channel_layout(ChannelLayout::unspecified(12))
            .unwrap();
        for i in 0..NUM_DATA_POINTERS {
            m.set_buffer(i, Some(bytes(4 + i)));
        }
        for i in 0..4 {
            m.push_extended_buffer(bytes(20 + i)).unwrap();
        }
        m.alloc_extended_data().unwrap();
        for i in 0..12 {
            m.set_extended_plane(i, i, 0).unwrap();
        }
        assert_eq!(m.set_extended_plane(12, 0, 0), Err(PlaneError::NoPlane));
        assert_eq!(m.set_extended_plane(0, 12, 0), Err(PlaneError::NoBuffer));
        assert_eq!(m.set_extended_plane(0, 8, 20), Err(PlaneError::OutOfBounds));
        let r = f.as_ref();
        assert!(r.has_extended_data());
        assert_eq!(
            (r.extended_plane_count(), r.extended_buffer_count()),
            (12, 4)
        );
        for i in 0..12 {
            let expect = if i < 8 { 4 + i } else { 20 + i - 8 };
            assert_eq!(r.extended_plane(i).unwrap().len(), expect);
        }
        assert!(r.extended_plane(12).is_none() && r.extended_buffer(4).is_none());
        let mut m = f.as_mut();
        for i in 0..12 {
            m.extended_plane_mut(i).unwrap().fill(i as u8);
        }
        assert_eq!(f.as_ref().extended_plane(11).unwrap(), &[11u8; 23][..]);
        let clone = f.as_ref().try_clone().unwrap();
        assert_eq!(
            clone.as_ref().extended_plane(11),
            f.as_ref().extended_plane(11)
        );
        assert_eq!(clone.as_ref().extended_buffer_count(), 4);
        // More channels would overrun the separate array: refused, layout
        // kept; fewer are fine.
        assert!(f.as_mut().channel_layout_mut().is_none());
        let back = f
            .as_mut()
            .set_channel_layout(ChannelLayout::unspecified(13))
            .unwrap_err();
        assert_eq!(back.as_ref().channel_count(), 13);
        assert_eq!(f.as_ref().channel_layout().channel_count(), 12);
        f.as_mut()
            .set_channel_layout(ChannelLayout::unspecified(9))
            .unwrap();
        assert_eq!(f.as_ref().extended_plane_count(), 9);
        let popped = f.as_mut().pop_extended_buffer().unwrap();
        assert!(f.as_ref().extended_plane(11).is_none());
        let old = f.as_mut().replace_extended_buffer(0, popped).unwrap();
        assert!(f.as_ref().extended_plane(8).is_none());
        f.as_mut().clear_extended_plane(0);
        f.as_mut().clear_extended_plane(99);
        assert!(f.as_ref().extended_plane(0).is_none());
        f.as_mut().reset_extended_data();
        assert!(f.as_mut().channel_layout_mut().is_some());
        drop(f);
        assert_eq!(old.as_ref().len(), 20);
        assert_eq!(clone.as_ref().extended_plane(9).unwrap(), &[9u8; 21][..]);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn side_data_metadata_and_user_data_outlive_the_frame() {
        let mut f = planar(16);
        let mut m = f.as_mut();
        let mut entries = m.side_data_mut();
        entries
            .add(SideDataType::AFD, bytes(1), SideDataFlags::NONE)
            .unwrap()
            .set_metadata(Dict::from_entries([(c"a", c"b")]).unwrap());
        entries
            .add(
                SideDataType::SEI_UNREGISTERED,
                bytes(4096),
                SideDataFlags::NONE,
            )
            .unwrap();
        m.set_metadata(Dict::from_entries([(c"k", c"v"), (c"k2", c"v2")]).unwrap());
        m.set_opaque_ref(Some(bytes(3)));
        let r = f.as_ref();
        assert_eq!(r.side_data().len(), 2);
        assert_eq!(r.side_data().get(1).unwrap().data().len(), 4096);
        assert_eq!(r.metadata().unwrap().len(), 2);
        assert_eq!(r.opaque_ref().unwrap().data(), [1, 4, 7]);
        let clone = r.try_clone().unwrap();
        let sd = clone
            .as_ref()
            .side_data()
            .get(1)
            .unwrap()
            .buffer()
            .try_clone()
            .unwrap();
        let meta = f.as_mut().take_metadata();
        assert!(f.as_ref().metadata().is_none());
        let user = f.as_mut().replace_opaque_ref(None).unwrap();
        let replaced = f.as_mut().replace_metadata(Dict::new());
        assert!(replaced.as_ref().is_none());
        assert!(f.as_mut().take_hw_frames_context().is_none());
        drop(f);
        assert_eq!(meta.as_ref().unwrap().len(), 2);
        assert_eq!(user.as_ref().data(), [1, 4, 7]);
        assert_eq!(clone.as_ref().metadata().unwrap().len(), 2);
        assert_eq!(clone.as_ref().opaque_ref().unwrap().data(), [1, 4, 7]);
        drop(clone);
        assert_eq!(sd.as_ref().len(), 4096);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn channel_layouts_change_in_place_and_by_value() {
        let mut f = Frame::new().unwrap();
        f.as_mut()
            .channel_layout_mut()
            .unwrap()
            .copy_from(ChannelLayout::default_for(6).as_ref())
            .unwrap();
        assert_eq!(f.as_ref().channel_layout().channel_count(), 6);
        let old = f
            .as_mut()
            .replace_channel_layout(ChannelLayout::custom(3).unwrap())
            .unwrap();
        assert_eq!(old.as_ref().channel_count(), 6);
        let clone = f.as_ref().try_clone();
        assert!(clone.is_none(), "no planes to copy");
        f.as_mut()
            .set_channel_layout(ChannelLayout::unspecified(0))
            .unwrap();
        drop(f);
        assert_eq!(old.as_ref().channel_count(), 6);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn clones_of_counted_frames_outlive_their_source() {
        let mut f = planar(64);
        f.as_mut().set_pts(11);
        f.as_mut().set_flags(FrameFlags::KEY);
        let clone = f.as_ref().try_clone().unwrap();
        let again = clone.as_ref().try_clone().unwrap();
        drop(f);
        for c in [&clone, &again] {
            let r = c.as_ref();
            assert_eq!(r.pts(), 11);
            assert_eq!(r.flags(), FrameFlags::KEY);
            assert_eq!(r.plane(2).unwrap().len(), 32);
            assert_eq!(r.buffer(0).unwrap().ref_count(), 2);
        }
        drop(clone);
        let mut again = again;
        assert!(again.as_mut().plane_mut(0).is_some());
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn frames_without_planes_do_not_clone() {
        let f = Frame::new().unwrap();
        assert!(f.as_ref().try_clone().is_none());
        let mut f = Frame::new().unwrap();
        f.as_mut().set_pixel_format(PixelFormat::GRAY8);
        f.as_mut().set_width(16);
        f.as_mut().set_height(16);
        assert!(f.as_ref().try_clone().is_none());
        f.as_mut().set_pixel_format(PixelFormat::from_raw(-77));
        assert!(f.as_ref().try_clone().is_none());
        f.as_mut().set_sample_format(SampleFormat::S16);
        f.as_mut().set_width(0);
        f.as_mut().set_sample_count(1024);
        f.as_mut()
            .set_channel_layout(ChannelLayout::default_for(2))
            .unwrap();
        assert!(f.as_ref().try_clone().is_none());
        // A buffer past index 0 alone keeps the frame uncounted.
        f.as_mut().set_buffer(1, Some(bytes(8)));
        assert!(f.as_ref().try_clone().is_none());
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn reset_releases_everything_and_restores_defaults() {
        let mut f = planar(16);
        f.as_mut()
            .set_metadata(Dict::from_entries([(c"x", c"y")]).unwrap());
        f.as_mut().push_extended_buffer(bytes(2)).unwrap();
        let keep = f.as_ref().buffer(0).unwrap().try_clone().unwrap();
        f.as_mut().reset();
        let r = f.as_ref();
        assert!(r.buffer(0).is_none() && r.plane(0).is_none());
        assert_eq!((r.format(), r.width(), r.pts()), (-1, 0, NOPTS));
        assert_eq!(r.extended_buffer_count(), 0);
        assert_eq!(keep.as_ref().ref_count(), 1);
        f.as_mut().reset();
        drop(f);
        assert_eq!(keep.as_ref().len(), 16);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn frames_cross_threads() {
        let f = Arc::new(planar(256));
        let barrier = Arc::new(Barrier::new(4));
        let handles: Vec<_> = (0..4)
            .map(|i| {
                let f = Arc::clone(&f);
                let barrier = Arc::clone(&barrier);
                thread::spawn(move || {
                    barrier.wait();
                    let mut copies = Vec::new();
                    for _ in 0..16 {
                        let r = FrameBox::as_ref(&f);
                        let c = r.try_clone().unwrap();
                        assert_eq!(c.as_ref().plane(i % 3).unwrap(), r.plane(i % 3).unwrap());
                        copies.push(c);
                    }
                    copies
                })
            })
            .collect();
        let copies: Vec<FrameBox> = handles
            .into_iter()
            .flat_map(|h| h.join().unwrap())
            .collect();
        assert_eq!(FrameBox::as_ref(&f).buffer(0).unwrap().ref_count(), 65);
        thread::scope(|s| {
            for c in copies {
                s.spawn(move || drop(c));
            }
        });
        let mut f = Arc::into_inner(f).unwrap();
        let writer = thread::spawn(move || {
            f.as_mut().plane_mut(0).unwrap().fill(9);
            f
        });
        let f = writer.join().unwrap();
        assert_eq!(f.as_ref().plane(0).unwrap(), &[9u8; 256][..]);
        thread::spawn(move || drop(f)).join().unwrap();
    }

    /// The hardware frames context accessors, safe since hwcontext wave 2
    /// typed them: the only value this build reaches without a backend is
    /// none, on the frame's thread and another.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn hw_frames_context_round_trips_none() {
        let mut f = Frame::new().unwrap();
        let old = f.as_mut().replace_hw_frames_context(None);
        assert!(old.is_none() && f.as_ref().hw_frames_context().is_none());
        assert!(f.as_mut().take_hw_frames_context().is_none());
        thread::spawn(move || {
            assert!(f.as_mut().replace_hw_frames_context(None).is_none());
            assert!(f.as_ref().try_clone().is_none());
        })
        .join()
        .unwrap();
    }
}

/// `AVFrame`'s routines: buffer allocation, copies of planes and properties,
/// references, replacements and moves, writability, cropping, plane buffer
/// and side data lookups, and the copies and crops the wrappers refuse
/// because C would reach bytes it must not.
mod frame_routines {
    use std::sync::{Arc, Barrier};
    use std::thread;

    use ffmpeg_bindings::libavutil::libavutil::buffer::BufferReference;
    use ffmpeg_bindings::libavutil::libavutil::channel_layout::ChannelLayout;
    use ffmpeg_bindings::libavutil::libavutil::dict::Dict;
    use ffmpeg_bindings::libavutil::libavutil::frame::{
        CropFlags, Frame, FrameBox, FrameRef, NUM_DATA_POINTERS, SideDataType,
    };
    use ffmpeg_bindings::libavutil::libavutil::mem::HeapVec;
    use ffmpeg_bindings::libavutil::libavutil::pixdesc::PixelFormat;
    use ffmpeg_bindings::libavutil::libavutil::samplefmt::SampleFormat;

    const EINVAL: i32 = -22;
    const ENOMEM: i32 = -12;
    /// `AVERROR_BUG`, `FFERRTAG('B', 'U', 'G', '!')`.
    const AVERROR_BUG: i32 = -i32::from_le_bytes(*b"BUG!");

    /// A video frame of `format`, `w` x `h`, with buffers from
    /// `alloc_buffers` at `align`.
    fn video(format: PixelFormat, w: i32, h: i32, align: usize) -> FrameBox {
        let mut f = Frame::new().unwrap();
        let mut m = f.as_mut();
        m.set_pixel_format(format);
        m.set_width(w);
        m.set_height(h);
        m.alloc_buffers(align).unwrap();
        f
    }

    /// An audio frame of `format`, `channels` x `samples`, with buffers from
    /// `alloc_buffers` at `align`.
    fn audio(format: SampleFormat, channels: i32, samples: i32, align: usize) -> FrameBox {
        let mut f = Frame::new().unwrap();
        let mut m = f.as_mut();
        m.set_sample_format(format);
        m.set_sample_count(samples);
        m.set_channel_layout(ChannelLayout::unspecified(channels))
            .unwrap();
        m.alloc_buffers(align).unwrap();
        f
    }

    /// Whether every byte of `bytes` is `b`, compared whole.
    fn is_all(bytes: &[u8], b: u8) -> bool {
        bytes == vec![b; bytes.len()].as_slice()
    }

    /// The rows the plane of a video frame holds, as C lays them out.
    fn rows(format: PixelFormat, plane: usize, h: i32) -> usize {
        let (_, sub) = format.chroma_subsampling().unwrap();
        if plane == 1 || plane == 2 {
            (h as usize).div_ceil(1 << sub)
        } else {
            h as usize
        }
    }

    const FORMATS: [PixelFormat; 9] = [
        PixelFormat::GRAY8,
        PixelFormat::YUV420P,
        PixelFormat::YUV444P16,
        PixelFormat::NV12,
        PixelFormat::RGB24,
        PixelFormat::RGBA,
        PixelFormat::PAL8,
        PixelFormat::MONOWHITE,
        PixelFormat::YUVA420P,
    ];

    /// Allocated planes read as zeroes, every row of every plane inside the
    /// buffer holding it, and are writable whole.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn allocated_video_planes_are_zeroed_and_writable() {
        for format in FORMATS {
            for (w, h) in [(1, 1), (7, 3), (33, 17)] {
                for align in [0, 1, 16, 64, 1 << 12] {
                    let mut f = video(format, w, h, align);
                    let r = f.as_ref();
                    assert!(r.is_writable());
                    let count = format.plane_count().unwrap();
                    for i in 0..count {
                        let plane = r.plane(i).unwrap();
                        assert!(is_all(plane, 0), "{format:?} {w}x{h} {align} plane {i}");
                        let ls = r.linesizes()[i] as usize;
                        let need = (rows(format, i, h) - 1) * ls + 1;
                        assert!(plane.len() >= need, "{format:?} {w}x{h} {align} plane {i}");
                        if align > 1 {
                            assert_eq!(plane.as_ptr().addr() % align.min(64), 0);
                        }
                        assert!(r.plane_buffer(i).is_some());
                    }
                    // Video planes share one buffer: a plane's view runs on
                    // over the planes after it.
                    let mut m = f.as_mut();
                    for i in (0..count).rev() {
                        m.plane_mut(i).unwrap().fill(i as u8 + 1);
                        assert!(is_all(m.as_ref().plane(i).unwrap(), i as u8 + 1));
                    }
                }
            }
        }
    }

    /// Audio planes likewise, with an `extended_data` array and extra
    /// buffers for more than 8 planar channels.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn allocated_audio_planes_are_zeroed_and_writable() {
        let formats = [
            SampleFormat::U8,
            SampleFormat::S16P,
            SampleFormat::FLTP,
            SampleFormat::DBL,
        ];
        for format in formats {
            for channels in [1, 2, 8, 9, 20] {
                for samples in [1, 47, 1024] {
                    let mut f = audio(format, channels, samples, 0);
                    let planes = if format.is_planar() {
                        channels as usize
                    } else {
                        1
                    };
                    let r = f.as_ref();
                    assert_eq!(r.has_extended_data(), planes > NUM_DATA_POINTERS);
                    assert_eq!(
                        r.extended_buffer_count(),
                        planes.saturating_sub(NUM_DATA_POINTERS)
                    );
                    let line = samples as usize
                        * format.bytes_per_sample()
                        * if format.is_planar() {
                            1
                        } else {
                            channels as usize
                        };
                    for i in 0..planes {
                        let plane = r.extended_plane(i).unwrap();
                        assert!(plane.len() >= line && is_all(plane, 0));
                        assert!(r.plane_buffer(i).is_some());
                    }
                    assert!(r.plane_buffer(planes).is_none());
                    let mut m = f.as_mut();
                    for i in 0..planes {
                        m.extended_plane_mut(i).unwrap().fill(3);
                    }
                    assert!(f.as_ref().is_writable());
                }
            }
        }
    }

    /// Alignments that are not powers of two up to `2^27` are refused before
    /// C, and the frame keeps what it held; allocations C refuses leave the
    /// frame without planes, and nothing leaks.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn allocations_refuse_bad_alignments_and_geometry() {
        let mut f = video(PixelFormat::GRAY8, 8, 8, 0);
        for align in [3, 48, (1 << 27) + 1, 1 << 28, usize::MAX] {
            assert_eq!(f.as_mut().alloc_buffers(align), Err(EINVAL));
            assert!(f.as_ref().buffer(0).is_some());
        }
        assert_eq!(f.as_mut().alloc_buffers(1 << 27), Ok(()));
        assert!(is_all(f.as_ref().plane(0).unwrap()[..8].as_ref(), 0));
        // Unknown formats, no size, too large a size, no samples.
        let cases: [fn(&mut FrameBox); 5] = [
            |f| f.as_mut().set_format(-1),
            |f| f.as_mut().set_format(100_000),
            |f| f.as_mut().set_width(0),
            |f| f.as_mut().set_height(1 << 30),
            |f| f.as_mut().set_pixel_format(PixelFormat::VAAPI),
        ];
        for case in cases {
            let mut f = video(PixelFormat::GRAY8, 8, 8, 0);
            let keep = f.as_ref().buffer(0).unwrap().try_clone().unwrap();
            case(&mut f);
            assert!(f.as_mut().alloc_buffers(0).is_err());
            assert!(f.as_ref().buffer(0).is_none());
            assert!(f.as_ref().plane(0).is_none());
            assert_eq!(keep.as_ref().ref_count(), 1);
        }
        // A refused line size for ten planar channels.
        let mut f = Frame::new().unwrap();
        let mut m = f.as_mut();
        m.set_sample_format(SampleFormat::U8P);
        m.set_sample_count(4);
        m.set_channel_layout(ChannelLayout::unspecified(10))
            .unwrap();
        m.set_linesize(0, -1);
        assert_eq!(m.alloc_buffers(0), Err(EINVAL));
        assert_eq!(m.as_ref().extended_buffer_count(), 0);
        assert!(!m.as_ref().has_extended_data());
        assert!(m.as_ref().extended_plane(9).is_none());
        // Line sizes set beforehand are kept.
        let mut f = Frame::new().unwrap();
        let mut m = f.as_mut();
        m.set_pixel_format(PixelFormat::GRAY8);
        m.set_width(8);
        m.set_height(2);
        m.set_linesize(0, 100);
        m.alloc_buffers(0).unwrap();
        assert_eq!(m.as_ref().linesizes()[0], 100);
        assert!(m.as_ref().plane(0).unwrap().len() >= 100 + 8);
    }

    /// Copies between allocated frames move every row, and leave padding
    /// and the other frame's buffers alone.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn copies_move_every_row() {
        for format in FORMATS {
            let mut src = video(format, 9, 5, 0);
            let mut dst = video(format, 9, 5, 1);
            let count = format.plane_count().unwrap();
            for i in 0..count {
                src.as_mut().plane_mut(i).unwrap().fill(0x5A);
            }
            if format == PixelFormat::PAL8 {
                src.as_mut().plane_mut(1).unwrap()[..1024].fill(0x77);
            }
            dst.as_mut().copy_data_from(src.as_ref()).unwrap();
            let (d, s) = (dst.as_ref(), src.as_ref());
            for i in 0..count {
                let dl = d.linesizes()[i] as usize;
                for y in 0..rows(format, i, 5) {
                    // The first byte of every row is copied.
                    let dp = &d.plane(i).unwrap()[y * dl..];
                    assert_eq!(dp[0], 0x5A, "{format:?} plane {i} row {y}");
                }
            }
            if format == PixelFormat::PAL8 {
                assert!(is_all(&d.plane(1).unwrap()[..1024], 0x77));
            }
            assert!(s.is_writable() && d.is_writable());
        }
        for (format, channels) in [(SampleFormat::S16P, 12), (SampleFormat::FLT, 3)] {
            let mut src = audio(format, channels, 31, 0);
            let mut dst = audio(format, channels, 31, 1);
            let planes = if format.is_planar() {
                channels as usize
            } else {
                1
            };
            for i in 0..planes {
                src.as_mut()
                    .extended_plane_mut(i)
                    .unwrap()
                    .fill(i as u8 + 1);
            }
            dst.as_mut().copy_data_from(src.as_ref()).unwrap();
            let line =
                31 * format.bytes_per_sample() * if planes == 1 { channels as usize } else { 1 };
            for i in 0..planes {
                let plane = dst.as_ref().extended_plane(i).unwrap();
                assert!(is_all(&plane[..line], i as u8 + 1));
            }
        }
    }

    /// Copies C would run past a buffer, write into a shared one, read past
    /// `extended_data`, or abort on are refused, writing nothing; C's own
    /// refusals come back as C's errors.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn copies_refuse_what_c_must_not_reach() {
        let src = video(PixelFormat::GRAY8, 8, 4, 0);
        let mut dst = video(PixelFormat::GRAY8, 8, 4096, 0);
        // A source taller than its buffer.
        let mut tall = video(PixelFormat::GRAY8, 8, 4, 0);
        tall.as_mut().set_height(4096);
        assert_eq!(dst.as_mut().copy_data_from(tall.as_ref()), Err(EINVAL));
        assert!(is_all(dst.as_ref().plane(0).unwrap(), 0));
        // Rows wider than the source's line size (C's abort).
        let mut narrow = video(PixelFormat::GRAY8, 8, 4, 0);
        narrow.as_mut().set_linesize(0, 2);
        assert_eq!(dst.as_mut().copy_data_from(narrow.as_ref()), Err(EINVAL));
        // A bottom-up source whose rows precede its buffer.
        let mut flipped = video(PixelFormat::GRAY8, 8, 4, 0);
        flipped.as_mut().set_linesize(0, -64);
        assert_eq!(dst.as_mut().copy_data_from(flipped.as_ref()), Err(EINVAL));
        // A shared destination.
        let shared = dst.as_ref().try_clone().unwrap();
        assert_eq!(dst.as_mut().copy_data_from(src.as_ref()), Err(EINVAL));
        assert!(is_all(shared.as_ref().plane(0).unwrap(), 0));
        drop(shared);
        assert_eq!(dst.as_mut().copy_data_from(src.as_ref()), Ok(()));
        // A read-only destination buffer.
        let mut ro = video(PixelFormat::GRAY8, 8, 4, 0);
        let len = ro.as_ref().buffer(0).unwrap().len();
        let bytes = vec![1u8; len].into_boxed_slice();
        let buf = BufferReference::from_read_only(bytes).unwrap();
        ro.as_mut().set_buffer(0, Some(buf));
        ro.as_mut().set_plane(0, 0, 0).unwrap();
        assert_eq!(ro.as_mut().copy_data_from(src.as_ref()), Err(EINVAL));
        assert!(is_all(ro.as_ref().plane(0).unwrap(), 1));
        // A palette that is not there.
        let mut pal = video(PixelFormat::PAL8, 4, 4, 0);
        let mut nopal = video(PixelFormat::PAL8, 4, 4, 0);
        nopal.as_mut().clear_plane(1);
        assert_eq!(pal.as_mut().copy_data_from(nopal.as_ref()), Err(EINVAL));
        assert_eq!(nopal.as_mut().copy_data_from(pal.as_ref()), Err(EINVAL));
        // A palettized source of negative width C would copy as `size_t`.
        let mut neg = video(PixelFormat::PAL8, 4, 4, 0);
        neg.as_mut().set_width(-1);
        assert_eq!(pal.as_mut().copy_data_from(neg.as_ref()), Err(EINVAL));
        // A bitstream width whose row overflows C's round-up.
        let mut mono = video(PixelFormat::MONOWHITE, 8, 1, 0);
        let mut wide = video(PixelFormat::MONOWHITE, 8, 1, 0);
        mono.as_mut().set_width(i32::MAX);
        wide.as_mut().set_width(i32::MAX);
        assert_eq!(mono.as_mut().copy_data_from(wide.as_ref()), Err(EINVAL));
        // Planar audio past `extended_data`, and an overflowing size.
        let mut a = audio(SampleFormat::U8P, 9, 4, 0);
        let mut b = audio(SampleFormat::U8P, 9, 4, 0);
        b.as_mut().reset_extended_data();
        assert_eq!(a.as_mut().copy_data_from(b.as_ref()), Err(EINVAL));
        assert_eq!(b.as_mut().copy_data_from(a.as_ref()), Err(EINVAL));
        let mut c = audio(SampleFormat::DBL, 2, 4, 0);
        let mut d = audio(SampleFormat::DBL, 2, 4, 0);
        c.as_mut().set_sample_count(i32::MAX / 8);
        d.as_mut().set_sample_count(i32::MAX / 8);
        assert_eq!(c.as_mut().copy_data_from(d.as_ref()), Err(EINVAL));
        // C's refusals: other formats, sizes, counts, layouts, planes.
        let gray = video(PixelFormat::GRAY8, 4, 4, 0);
        assert_eq!(pal.as_mut().copy_data_from(gray.as_ref()), Err(EINVAL));
        let small = video(PixelFormat::GRAY8, 4, 2, 0);
        let mut smaller = video(PixelFormat::GRAY8, 4, 1, 0);
        assert_eq!(smaller.as_mut().copy_data_from(small.as_ref()), Err(EINVAL));
        let e = audio(SampleFormat::U8P, 2, 4, 0);
        let mut f = audio(SampleFormat::U8P, 3, 4, 0);
        assert_eq!(f.as_mut().copy_data_from(e.as_ref()), Err(EINVAL));
        let mut empty = Frame::new().unwrap();
        assert_eq!(empty.as_mut().copy_data_from(gray.as_ref()), Err(EINVAL));
    }

    /// `make_writable` copies shared or read-only planes into zeroed
    /// buffers of the frame's own, keeps writable ones, and refuses frames
    /// whose geometry describes rows outside their buffers.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn make_writable_copies_only_shared_planes() {
        let mut f = video(PixelFormat::YUV420P, 9, 5, 0);
        f.as_mut().plane_mut(0).unwrap().fill(4);
        assert_eq!(f.as_mut().make_writable(), Ok(()));
        let copy = f.as_ref().try_clone().unwrap();
        assert!(!f.as_ref().is_writable());
        assert_eq!(f.as_mut().make_writable(), Ok(()));
        assert!(f.as_ref().is_writable() && copy.as_ref().is_writable());
        let ls = f.as_ref().linesizes()[0] as usize;
        let plane = f.as_ref().plane(0).unwrap();
        for y in 0..5 {
            assert!(is_all(&plane[y * ls..y * ls + 9], 4));
            assert!(is_all(&plane[y * ls + 9..(y + 1) * ls], 0));
        }
        // Read-only buffers are copied too.
        let mut ro = Frame::new().unwrap();
        let mut m = ro.as_mut();
        m.set_pixel_format(PixelFormat::GRAY8);
        m.set_width(4);
        m.set_height(2);
        m.set_linesize(0, 4);
        let buf = BufferReference::from_read_only(vec![6u8; 8]).unwrap();
        m.set_buffer(0, Some(buf));
        m.set_plane(0, 0, 0).unwrap();
        m.set_metadata(Dict::from_entries([(c"k", c"v")]).unwrap());
        m.alloc_side_data_from_slice(SideDataType::AFD, &[9])
            .unwrap();
        assert_eq!(m.make_writable(), Ok(()));
        assert!(ro.as_ref().is_writable());
        assert!(is_all(&ro.as_ref().plane(0).unwrap()[..4], 6));
        assert_eq!(ro.as_ref().side_data().len(), 1);
        assert_eq!(ro.as_ref().metadata().unwrap().iter().count(), 1);
        // A shared frame claiming more rows than its buffer holds.
        let mut tall = video(PixelFormat::GRAY8, 8, 4, 0);
        let keep = tall.as_ref().try_clone().unwrap();
        tall.as_mut().set_height(4096);
        assert_eq!(tall.as_mut().make_writable(), Err(EINVAL));
        assert_eq!(tall.as_ref().buffer(0).unwrap().ref_count(), 2);
        // A shared frame with a side data entry at a NULL window.
        let mut nullsd = video(PixelFormat::GRAY8, 8, 4, 0);
        let empty = BufferReference::from_heap_vec(HeapVec::empty()).unwrap();
        nullsd
            .as_mut()
            .add_side_data(SideDataType::AFD, empty)
            .unwrap();
        let keep2 = nullsd.as_ref().try_clone().unwrap();
        assert_eq!(nullsd.as_mut().make_writable(), Err(EINVAL));
        drop((keep, keep2));
        // A new frame has nothing to copy.
        assert_eq!(Frame::new().unwrap().as_mut().make_writable(), Err(EINVAL));
    }

    /// References share buffers and copy properties; replacements keep
    /// shared buffers; moves leave defaults; all release what they replace.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn references_replacements_and_moves_release_what_they_replace() {
        let mut src = video(PixelFormat::YUV420P, 6, 4, 0);
        src.as_mut().set_pts(42);
        src.as_mut()
            .alloc_side_data_from_slice(SideDataType::AFD, &[3])
            .unwrap();
        src.as_mut()
            .set_metadata(Dict::from_entries([(c"a", c"1")]).unwrap());
        let mut dst = audio(SampleFormat::S16P, 10, 8, 0);
        let old = dst.as_ref().buffer(0).unwrap().try_clone().unwrap();
        dst.as_mut().try_clone_from(src.as_ref()).unwrap();
        assert_eq!(old.as_ref().ref_count(), 1);
        assert_eq!(dst.as_ref().pts(), 42);
        assert_eq!(src.as_ref().buffer(0).unwrap().ref_count(), 2);
        assert_eq!(
            dst.as_ref().plane(1).unwrap(),
            src.as_ref().plane(1).unwrap()
        );
        assert_eq!(
            dst.as_ref()
                .find_side_data(SideDataType::AFD)
                .unwrap()
                .data(),
            [3]
        );
        // A replacement from the same source keeps the reference.
        dst.as_mut().assign(src.as_ref()).unwrap();
        assert_eq!(src.as_ref().buffer(0).unwrap().ref_count(), 2);
        assert_eq!(dst.as_ref().side_data().len(), 1);
        // From another source, it trades the reference.
        let other = audio(SampleFormat::U8P, 12, 4, 0);
        dst.as_mut().assign(other.as_ref()).unwrap();
        assert_eq!(src.as_ref().buffer(0).unwrap().ref_count(), 1);
        assert_eq!(other.as_ref().extended_buffer(3).unwrap().ref_count(), 2);
        assert_eq!(dst.as_ref().extended_plane_count(), 12);
        assert!(dst.as_ref().side_data().is_empty());
        // From a frame without planes, it takes properties alone.
        let mut bare = Frame::new().unwrap();
        bare.as_mut().set_pts(5);
        dst.as_mut().assign(bare.as_ref()).unwrap();
        assert!(dst.as_ref().buffer(0).is_none());
        assert_eq!(
            (dst.as_ref().pts(), dst.as_ref().extended_plane_count()),
            (5, 8)
        );
        assert_eq!(other.as_ref().extended_buffer(3).unwrap().ref_count(), 1);
        // A reference to a frame without planes fails, resetting.
        assert_eq!(dst.as_mut().try_clone_from(bare.as_ref()), Err(EINVAL));
        assert_eq!(dst.as_ref().pts(), Frame::new().unwrap().as_ref().pts());
        // Moves.
        let mut a = video(PixelFormat::GRAY8, 4, 4, 0);
        let mut b = audio(SampleFormat::U8P, 9, 4, 0);
        let ext = b.as_ref().extended_plane(8).unwrap().as_ptr();
        a.as_mut().move_from(b.as_mut());
        assert_eq!(a.as_ref().extended_plane(8).unwrap().as_ptr(), ext);
        assert!(b.as_ref().buffer(0).is_none() && !b.as_ref().has_extended_data());
        let c = a.as_mut().take().unwrap();
        assert_eq!(c.as_ref().extended_plane(8).unwrap().as_ptr(), ext);
        assert!(a.as_ref().buffer(0).is_none());
        b.as_mut().move_from(Frame::new().unwrap().as_mut());
        drop((a, b, c));
    }

    /// Properties copy into any frame: metadata merges, side data appends
    /// with new payloads; entries at a NULL window are refused.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn property_copies_append_and_refuse_null_payloads() {
        let mut src = Frame::new().unwrap();
        let mut m = src.as_mut();
        m.set_pts(9);
        m.set_crop_left(3);
        m.alloc_side_data_from_slice(SideDataType::AFD, &[1])
            .unwrap();
        m.alloc_side_data_zeroed(SideDataType::PAN_SCAN, 20)
            .unwrap();
        m.set_metadata(Dict::from_entries([(c"a", c"1"), (c"b", c"2")]).unwrap());
        let mut dst = video(PixelFormat::GRAY8, 4, 4, 0);
        dst.as_mut()
            .set_metadata(Dict::from_entries([(c"a", c"0"), (c"c", c"3")]).unwrap());
        dst.as_mut()
            .alloc_side_data_zeroed(SideDataType::AFD, 1)
            .unwrap();
        dst.as_mut().copy_props_from(src.as_ref()).unwrap();
        let d = dst.as_ref();
        assert_eq!((d.pts(), d.crop_left()), (9, 3));
        // Pan-scan entries are skipped for frames of another size.
        assert_eq!(d.side_data().len(), 2);
        assert_eq!(d.metadata().unwrap().iter().count(), 3);
        let entry = d.side_data().get(1).unwrap();
        let source = src.as_ref().side_data().get(0).unwrap();
        assert!(!entry.buffer().buffer().ptr_eq(&source.buffer().buffer()));
        assert_eq!(entry.buffer().ref_count(), 1);
        let empty = BufferReference::from_heap_vec(HeapVec::empty()).unwrap();
        src.as_mut()
            .add_side_data(SideDataType::A53_CC, empty)
            .unwrap();
        let mut target = Frame::new().unwrap();
        assert_eq!(target.as_mut().copy_props_from(src.as_ref()), Err(EINVAL));
        assert_eq!(target.as_ref().pts(), Frame::new().unwrap().as_ref().pts());
        // References share such an entry without copying its bytes.
        let mut shared = Frame::new().unwrap();
        shared.as_mut().assign(src.as_ref()).unwrap();
        assert_eq!(shared.as_ref().side_data().len(), 3);
    }

    /// Cropping moves planes inside their buffers and shrinks the frame;
    /// crops C cannot compute, or that would leave a buffer, are refused
    /// with the frame unchanged.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn cropping_stays_inside_the_buffers() {
        for format in FORMATS {
            for flags in [CropFlags::empty(), CropFlags::UNALIGNED] {
                let mut f = video(format, 64, 32, 0);
                let mut m = f.as_mut();
                m.set_crop_top(4);
                m.set_crop_bottom(2);
                m.set_crop_left(20);
                m.set_crop_right(3);
                let cropped = m.apply_cropping(flags);
                if cropped == Err(AVERROR_BUG) && flags.is_empty() {
                    // C cannot keep this format's alignment for this edge
                    // (its offsets are more aligned than the edge).
                    assert_eq!(f.as_ref().height(), 32, "{format:?}");
                    continue;
                }
                assert_eq!(cropped, Ok(()), "{format:?} {flags:?}");
                let r = f.as_ref();
                if format == PixelFormat::MONOWHITE {
                    // Bitstream formats: the right and bottom edges only.
                    assert_eq!((r.width(), r.height()), (61, 30));
                    assert_eq!((r.crop_top(), r.crop_right()), (4, 0));
                    continue;
                }
                assert_eq!(r.height(), 26);
                // Without `UNALIGNED`, C may round the left edge down.
                assert!((41..=61).contains(&r.width()), "{format:?}");
                assert_eq!((r.crop_top(), r.crop_left()), (0, 0));
                for i in 0..format.plane_count().unwrap() {
                    assert!(r.plane(i).is_some(), "{format:?} {flags:?} plane {i}");
                }
            }
        }
        // Rows past the buffer, a negative line size cropped from the top,
        // a palettized frame with extra planes, wrapped crops.
        let mut tall = video(PixelFormat::GRAY8, 8, 8, 0);
        tall.as_mut().set_height(1 << 20);
        tall.as_mut().set_crop_top(1 << 19);
        let before = tall.as_ref().plane(0).unwrap().as_ptr();
        assert_eq!(
            tall.as_mut().apply_cropping(CropFlags::UNALIGNED),
            Err(EINVAL)
        );
        assert_eq!(tall.as_ref().plane(0).unwrap().as_ptr(), before);
        assert_eq!(tall.as_ref().crop_top(), 1 << 19);
        let mut flipped = video(PixelFormat::GRAY8, 8, 8, 0);
        let ls = flipped.as_ref().linesizes()[0];
        flipped.as_mut().set_linesize(0, -ls);
        flipped.as_mut().set_crop_top(1);
        assert_eq!(
            flipped.as_mut().apply_cropping(CropFlags::empty()),
            Err(EINVAL)
        );
        let mut pal = video(PixelFormat::PAL8, 8, 8, 0);
        pal.as_mut().set_plane(2, 0, 0).unwrap();
        pal.as_mut().set_crop_top(1);
        assert_eq!(pal.as_mut().apply_cropping(CropFlags::empty()), Err(EINVAL));
        let mut wrap = video(PixelFormat::GRAY8, 64, 8, 0);
        wrap.as_mut().set_crop_left(1 << 32);
        wrap.as_mut().set_crop_right(usize::MAX - (1 << 32) + 2);
        assert_eq!(
            wrap.as_mut().apply_cropping(CropFlags::empty()),
            Err(EINVAL)
        );
        // C's own refusals.
        let mut f = video(PixelFormat::GRAY8, 8, 8, 0);
        f.as_mut().set_crop_left(8);
        assert_eq!(f.as_mut().apply_cropping(CropFlags::empty()), Err(-34));
        f.as_mut().set_width(0);
        assert_eq!(f.as_mut().apply_cropping(CropFlags::empty()), Err(EINVAL));
        // Hardware and bitstream formats crop the far edges only.
        let mut mono = video(PixelFormat::MONOBLACK, 64, 8, 0);
        let p = mono.as_ref().plane(0).unwrap().as_ptr();
        mono.as_mut().set_crop_left(8);
        mono.as_mut().set_crop_right(8);
        assert_eq!(mono.as_mut().apply_cropping(CropFlags::empty()), Ok(()));
        assert_eq!(
            (
                mono.as_ref().width(),
                mono.as_ref().plane(0).unwrap().as_ptr()
            ),
            (56, p)
        );
    }

    /// Plane buffer lookups never read past the `extended_data` entries a
    /// frame holds.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn plane_buffer_lookups_stay_inside_extended_data() {
        let mut v = video(PixelFormat::YUV420P, 4, 4, 0);
        v.as_mut()
            .set_channel_layout(ChannelLayout::unspecified(1))
            .unwrap();
        v.as_mut().alloc_extended_data().unwrap();
        assert!(v.as_ref().plane_buffer(0).is_some());
        for i in 1..NUM_DATA_POINTERS + 2 {
            assert!(v.as_ref().plane_buffer(i).is_none());
        }
        let mut a = audio(SampleFormat::U8P, 9, 4, 0);
        assert!(a.as_ref().plane_buffer(8).is_some());
        a.as_mut().reset_extended_data();
        assert!(a.as_ref().plane_buffer(7).is_some());
        assert!(a.as_ref().plane_buffer(8).is_none());
        assert!(a.as_ref().plane_buffer(usize::MAX).is_none());
        let bare = Frame::new().unwrap();
        assert!((0..NUM_DATA_POINTERS).all(|i| bare.as_ref().plane_buffer(i).is_none()));
    }

    /// Side data appends (even repeated kinds), lookups and removals.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn side_data_appends_finds_and_removes() {
        let mut f = Frame::new().unwrap();
        let mut m = f.as_mut();
        m.alloc_side_data_from_slice(SideDataType::AFD, &[1])
            .unwrap();
        m.alloc_side_data_zeroed(SideDataType::AFD, 1)
            .unwrap()
            .data_mut()
            .unwrap()[0] = 2;
        let buf = BufferReference::alloc(3).unwrap().fill(5);
        let entry = m.add_side_data(SideDataType::A53_CC, buf).unwrap();
        assert_eq!(entry.as_ref().data(), [5; 3]);
        m.alloc_side_data_zeroed(SideDataType::SEI_UNREGISTERED, 0)
            .unwrap();
        let empty = BufferReference::from_heap_vec(HeapVec::empty()).unwrap();
        m.add_side_data(SideDataType::SEI_UNREGISTERED, empty)
            .unwrap();
        assert_eq!(m.as_ref().side_data().len(), 5);
        m.find_side_data_mut(SideDataType::A53_CC)
            .unwrap()
            .data_mut()
            .unwrap()[1] = 7;
        assert_eq!(
            m.as_ref().find_side_data(SideDataType::AFD).unwrap().data(),
            [1]
        );
        assert_eq!(
            m.as_ref()
                .find_side_data(SideDataType::A53_CC)
                .unwrap()
                .data(),
            [5, 7, 5]
        );
        assert!(
            m.as_ref()
                .find_side_data(SideDataType::GOP_TIMECODE)
                .is_none()
        );
        m.remove_side_data(SideDataType::AFD);
        assert_eq!(m.as_ref().side_data().len(), 3);
        m.remove_side_data(SideDataType::SEI_UNREGISTERED);
        m.remove_side_data(SideDataType::GOP_TIMECODE);
        assert_eq!(m.as_ref().side_data().len(), 1);
        // A shared payload is not writable.
        let copy = f.as_ref().try_clone();
        assert!(copy.is_none());
        let mut g = Frame::new().unwrap();
        g.as_mut().assign(f.as_ref()).unwrap();
        assert!(
            f.as_mut()
                .find_side_data_mut(SideDataType::A53_CC)
                .unwrap()
                .data_mut()
                .is_none()
        );
        drop(g);
        assert!(
            f.as_mut()
                .find_side_data_mut(SideDataType::A53_CC)
                .unwrap()
                .data_mut()
                .is_some()
        );
    }

    /// Runs test `name` again alone in a child whose `UBSan` halts on its
    /// first report (the asan-ubsan build otherwise only prints C's
    /// reports); true inside that child.
    fn halting(name: &str) -> bool {
        const CHILD: &str = "CRUSTIFY_FRAME_HALTING_CHILD";
        if std::env::var_os(CHILD).is_some() {
            return true;
        }
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", name, "--test-threads=1"])
            .env(CHILD, "1")
            .env("UBSAN_OPTIONS", "halt_on_error=1")
            .status()
            .unwrap();
        assert!(status.success(), "{name} failed in its child: {status}");
        false
    }

    /// Every routine that has C allocate buffers for a copy of an audio
    /// frame stays defined at `INT_MAX - 31` samples, the count whose
    /// default rounding (`nb_samples + 32 - 1` in `int`) C's range check lets
    /// through: copies of a frame that is not reference counted are refused,
    /// and `make_writable` and `alloc_buffers` hand C the line size it would
    /// compute, which it then cannot allocate.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn copies_at_a_sample_count_c_cannot_round_stay_defined() {
        const NAME: &str =
            "frame::frame_routines::copies_at_a_sample_count_c_cannot_round_stay_defined";
        if !halting(NAME) {
            return;
        }
        let shapes = [
            (SampleFormat::U8, 1),
            (SampleFormat::S16, 2),
            (SampleFormat::FLTP, 3),
            (SampleFormat::U8P, 10),
        ];
        for (format, channels) in shapes {
            let shaped = || {
                let mut f = Frame::new().unwrap();
                let mut m = f.as_mut();
                m.set_sample_format(format);
                m.set_sample_count(i32::MAX - 31);
                m.set_channel_layout(ChannelLayout::unspecified(channels))
                    .unwrap();
                if channels > NUM_DATA_POINTERS as i32 {
                    m.alloc_extended_data().unwrap();
                }
                f
            };
            let mut f = shaped();
            let what = format!("{format:?} x {channels}");
            // Not reference counted: C would copy it into buffers of its own.
            assert!(f.as_ref().try_clone().is_none(), "{what}");
            let mut dst = Frame::new().unwrap();
            dst.as_mut().set_pts(1);
            assert_eq!(
                dst.as_mut().try_clone_from(f.as_ref()),
                Err(EINVAL),
                "{what}"
            );
            assert_eq!(dst.as_ref().pts(), i64::MIN, "{what}: reset");
            // C accepts a frame's planes copied into buffers of the line
            // size it rounds to: 2 GiB for one byte per sample (more than
            // `av_max_alloc` allows), more than an `int` otherwise.
            let expected = if channels == 1 { ENOMEM } else { EINVAL };
            assert_eq!(f.as_mut().make_writable(), Err(expected), "{what}");
            let mut g = shaped();
            assert_eq!(g.as_mut().alloc_buffers(0), Err(expected), "{what}");
            // Reference counted and shared, so copied by `make_writable`.
            let buf = BufferReference::alloc_zeroed(64).unwrap();
            let keep = buf.as_ref().try_clone().unwrap();
            f.as_mut().set_buffer(0, Some(buf));
            assert!(!f.as_ref().is_writable());
            assert_eq!(f.as_mut().make_writable(), Err(expected), "{what}");
            assert_eq!(keep.as_ref().ref_count(), 2, "{what}: left as it was");
            // Its references are shared rather than copied.
            let clone = f.as_ref().try_clone().expect("a counted clone");
            assert_eq!(clone.as_ref().sample_count(), i32::MAX - 31);
            assert_eq!(dst.as_mut().try_clone_from(f.as_ref()), Ok(()), "{what}");
        }
    }

    /// Shared frames serve lookups, references, copies and property copies
    /// on many threads at once, while each thread makes its own reference
    /// writable.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn frames_serve_routines_across_threads() {
        let mut src = video(PixelFormat::YUV420P, 16, 8, 0);
        src.as_mut().plane_mut(0).unwrap().fill(8);
        src.as_mut()
            .alloc_side_data_from_slice(SideDataType::AFD, &[4])
            .unwrap();
        let src = Arc::new(src);
        let barrier = Arc::new(Barrier::new(4));
        let handles: Vec<_> = (0..4)
            .map(|_| {
                let src = Arc::clone(&src);
                let barrier = Arc::clone(&barrier);
                thread::spawn(move || {
                    let r: FrameRef<'_> = FrameBox::as_ref(&src);
                    barrier.wait();
                    for _ in 0..8 {
                        assert!(r.plane_buffer(0).is_some());
                        assert_eq!(r.find_side_data(SideDataType::AFD).unwrap().data(), [4]);
                        let _ = r.is_writable();
                        let mut mine = Frame::new().unwrap();
                        mine.as_mut().try_clone_from(r).unwrap();
                        mine.as_mut().make_writable().unwrap();
                        mine.as_mut().plane_mut(0).unwrap()[0] = 1;
                        let mut copy = video(PixelFormat::YUV420P, 16, 8, 0);
                        copy.as_mut().copy_data_from(r).unwrap();
                        copy.as_mut().copy_props_from(r).unwrap();
                        let mut other = Frame::new().unwrap();
                        other.as_mut().assign(r).unwrap();
                        assert_eq!(copy.as_ref().plane(0).unwrap()[0], 8);
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
        let src = Arc::into_inner(src).unwrap();
        assert!(src.as_ref().is_writable());
        thread::spawn(move || drop(src)).join().unwrap();
    }
}
