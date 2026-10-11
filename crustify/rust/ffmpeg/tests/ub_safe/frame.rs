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

/// `SideDataDescriptor` and `SideDataProps`: inline values, `av_malloc`-family
/// owners and arrays of descriptors, every handle form, names from literals
/// and from exactly sized heap allocations (so a read past the NUL is an
/// ASan report), names that outlive the descriptor they were read from,
/// copies outliving their source, and the asserted `Send` / `Sync`. Tests
/// without `heap` in their name make no C call and also run under Miri
/// (buffer-frame wave 0, batch 4).
mod side_data_descriptor {
    use core::ffi::{CStr, c_uint};
    use std::ffi::CString;
    use std::sync::Mutex;
    use std::thread;

    use ffmpeg::libavutil::frame::{
        SideDataDescriptor, SideDataDescriptorMut, SideDataDescriptorRef, SideDataProps,
    };
    use ffmpeg::libavutil::mem::{HeapBox, alloc_cell_array_zeroed};

    /// `bytes` as an exactly sized heap C string that lives for the rest of
    /// the program, kept reachable from a static so that neither
    /// LeakSanitizer nor Miri reports it.
    fn leaked(bytes: &[u8]) -> &'static CStr {
        static KEEP: Mutex<Vec<&'static CStr>> = Mutex::new(Vec::new());
        let s: &'static CStr = Box::leak(CString::new(bytes).unwrap().into_boxed_c_str());
        KEEP.lock().unwrap().push(s);
        s
    }

    /// Property bits at every edge: none, named, unnamed, the sign bit, all.
    const PROPS: [c_uint; 5] = [0, 0x1f, 0x20, 0x8000_0000, c_uint::MAX];

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
        #[allow(clippy::clone_on_copy)] // exercises the explicit `Clone` impl
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

/// `SideDataType::descriptor` and `SideDataType::name` (buffer-frame wave 1,
/// batch 1): every value, including those the table has no entry for and
/// those past its end; the `'static` borrows outliving the values and copies
/// they came from, read on other threads, and copied into Rust-held
/// descriptors.
mod side_data_lookups {
    use core::ffi::CStr;
    use std::thread;

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::frame::{SideDataDescriptor, SideDataDescriptorRef, SideDataType};

    fn every_value() -> impl Iterator<Item = SideDataType> {
        (0..=256u32)
            .chain((0..32).map(|s| 1u32 << s))
            .chain([u32::MAX, u32::MAX - 1, i32::MAX as u32])
            .map(|raw| SideDataType::from_raw(ffi::AVFrameSideDataType(raw)))
    }

    #[test]
    fn every_value_looks_up_without_reading_past_the_table() {
        for t in every_value() {
            let d = t.descriptor();
            let n = t.name();
            assert_eq!(d.and_then(|d| d.name()), n, "{t:?}");
            if let Some(n) = n {
                // The whole literal, through its NUL.
                assert!(!n.to_bytes().is_empty());
                assert!(n.to_str().is_ok());
            }
        }
    }

    /// The `'static` results outlive the value they came from, and cross
    /// threads both ways.
    #[test]
    fn static_results_outlive_their_value_and_cross_threads() {
        let found: Vec<(SideDataDescriptorRef<'static>, &'static CStr)> = {
            let values: Vec<SideDataType> = every_value().collect();
            values
                .iter()
                .filter_map(|t| Some((t.descriptor()?, t.name()?)))
                .collect()
        };
        let handles: Vec<_> = (0..4)
            .map(|i| {
                let found = found.clone();
                thread::spawn(move || {
                    let mut copies = Vec::new();
                    for (d, n) in found.iter().skip(i).step_by(4) {
                        assert_eq!(d.name(), Some(*n));
                        let mut own = SideDataDescriptor::new();
                        own.as_mut().set(d.get());
                        own.as_mut().set_name(d.name());
                        copies.push(own);
                    }
                    copies
                })
            })
            .collect();
        let copies: Vec<SideDataDescriptor> = handles
            .into_iter()
            .flat_map(|h| h.join().unwrap())
            .collect();
        assert_eq!(copies.len(), found.len());
        for (d, _) in &found {
            assert!(copies.contains(&d.get()));
        }
    }
}

/// `RegionOfInterest`: inline values and `av_malloc`-family arrays of them
/// (the shape of `REGIONS_OF_INTEREST` side data), every field at its edges,
/// the offset's handles and their reborrows, copies outliving their source,
/// and the asserted `Send` / `Sync`. Tests without `heap` in their name make
/// no C call and also run under Miri (buffer-frame wave 1, batch 0).
mod region_of_interest {
    use std::thread;

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::frame::{RegionOfInterest, RegionOfInterestMut, RegionOfInterestRef};
    use ffmpeg::libavutil::mem::{HeapVec, alloc_cell_array_zeroed};
    use ffmpeg::libavutil::rational::Rational;

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
