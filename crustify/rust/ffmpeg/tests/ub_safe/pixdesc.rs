//! UB tests for `libavutil/pixdesc.h`, through the safe API only.

/// `ComponentDescriptor`: inline values, `av_malloc`-family owners and arrays
/// of them (the shape of a descriptor's `comp[4]`), every handle form, copies
/// outliving their source, and the asserted `Send` / `Sync`.
mod component_descriptor {
    use std::thread;

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::mem::{HeapBox, alloc_cell_array_zeroed};
    use ffmpeg::libavutil::pixdesc::{
        ComponentDescriptor, ComponentDescriptorMut, ComponentDescriptorRef,
    };

    /// Field tuples at every edge: all-zero, C's own extremes (a negative
    /// offset, 32-bit depth and step), and the `int` limits.
    const FIELDS: [[i32; 5]; 6] = [
        [0, 0, 0, 0, 0],
        [0, 2, -1, 3, 5],
        [3, 32, 30, 7, 32],
        [i32::MIN, i32::MIN, i32::MIN, i32::MIN, i32::MIN],
        [i32::MAX, i32::MAX, i32::MAX, i32::MAX, i32::MAX],
        [-1, 0, i32::MAX, -1, i32::MIN],
    ];

    fn fields(d: ComponentDescriptorRef<'_>) -> [i32; 5] {
        [d.plane(), d.step(), d.offset(), d.shift(), d.depth()]
    }

    fn fill(m: &mut ComponentDescriptorMut<'_>, [plane, step, offset, shift, depth]: [i32; 5]) {
        m.set_plane(plane);
        m.set_step(step);
        m.set_offset(offset);
        m.set_shift(shift);
        m.set_depth(depth);
    }

    /// A descriptor built without a C call, so the tests using it also run
    /// under Miri.
    fn d([plane, step, offset, shift, depth]: [i32; 5]) -> ComponentDescriptor {
        ComponentDescriptor::from_raw(ffi::AVComponentDescriptor {
            plane,
            step,
            offset,
            shift,
            depth,
        })
    }

    #[test]
    fn every_field_tuple_builds_and_reads_back() {
        for f in FIELDS {
            let mut v = ComponentDescriptor::new();
            fill(&mut v.as_mut(), f);
            assert_eq!(fields(v.as_ref()), f);
            assert_eq!(v, d(f));
            let back: ffi::AVComponentDescriptor = v.into();
            assert_eq!(
                [back.plane, back.step, back.offset, back.shift, back.depth],
                f
            );
            assert!(!format!("{v:?}").is_empty());
        }
        assert_eq!(fields(ComponentDescriptor::default().as_ref()), [0; 5]);
    }

    #[test]
    fn shared_handles_copy_and_coexist() {
        let v = d(FIELDS[1]);
        let a = v.as_ref();
        let b = a;
        let c = v.as_ref();
        assert_eq!((a.offset(), b.depth(), fields(c)), (-1, 5, FIELDS[1]));
    }

    fn reset(mut m: ComponentDescriptorMut<'_>) {
        m.set(ComponentDescriptor::new());
    }

    #[test]
    fn exclusive_handle_reborrows_without_aliasing() {
        let mut v = d(FIELDS[2]);
        let mut m = v.as_mut();
        reset(m.as_mut());
        assert_eq!(fields(m.as_ref()), [0; 5]);
        m.set_depth(i32::MIN);
        let shared = m.as_ref();
        assert_eq!(shared.depth(), i32::MIN);
        m.set_plane(3);
        assert_eq!(fields(v.as_ref()), [3, 0, 0, 0, i32::MIN]);
    }

    #[test]
    fn copies_outlive_and_are_independent_of_their_source() {
        let copy = {
            let mut boxed = HeapBox::<ComponentDescriptor>::new_zeroed().unwrap();
            boxed.as_mut().set(d(FIELDS[1]));
            let copy = boxed.as_ref().get();
            boxed.as_mut().set_offset(0);
            copy
            // `boxed` is freed here; `copy` holds its own bytes.
        };
        assert_eq!(fields(copy.as_ref()), FIELDS[1]);
        let mut b = copy;
        b.as_mut().set_shift(-1);
        #[allow(clippy::clone_on_copy)] // exercises the explicit `Clone` impl
        let c = Clone::clone(&b);
        assert_eq!(fields(copy.as_ref()), FIELDS[1]);
        assert_eq!(fields(c.as_ref()), [0, 2, -1, -1, 5]);
    }

    #[test]
    fn heap_box_owner_lives_reads_writes_clones_and_frees() {
        let mut a = HeapBox::<ComponentDescriptor>::new_zeroed().unwrap();
        assert_eq!(fields(a.as_ref()), [0; 5]);
        for f in FIELDS {
            fill(&mut a.as_mut(), f);
            assert_eq!(fields(a.as_ref()), f);
        }
        let b = a.clone();
        let c = b.try_clone().unwrap();
        // The source may go first; the copies own their own blocks.
        drop(a);
        assert_eq!(fields(b.as_ref()), FIELDS[5]);
        drop(b);
        assert_eq!(fields(c.as_ref()), FIELDS[5]);
        drop(HeapBox::<ComponentDescriptor>::new_uninit().unwrap());
    }

    #[test]
    fn heap_array_handles_stay_in_bounds() {
        let mut a = alloc_cell_array_zeroed::<ComponentDescriptor>(FIELDS.len()).unwrap();
        assert!(a.as_handles().iter().all(|h| fields(h) == [0; 5]));
        {
            let mut handles = a.as_handles_mut();
            for (i, f) in FIELDS.into_iter().enumerate() {
                fill(&mut handles.get_mut(i).unwrap(), f);
            }
            assert!(handles.get_mut(FIELDS.len()).is_none());
        }
        let read: Vec<[i32; 5]> = a.as_handles().iter().map(fields).collect();
        assert_eq!(read, FIELDS);
        assert!(a.as_handles().get(FIELDS.len()).is_none());
        let b = a.clone();
        drop(a);
        assert_eq!(
            b.as_handles().iter().map(fields).collect::<Vec<_>>(),
            FIELDS
        );
        drop(alloc_cell_array_zeroed::<ComponentDescriptor>(0).unwrap());
    }

    #[test]
    fn values_and_owners_move_across_threads() {
        let v = d(FIELDS[2]);
        let b = HeapBox::<ComponentDescriptor>::new_zeroed().unwrap();
        let (v, b) = thread::spawn(move || {
            let mut b = b;
            b.as_mut().set(v);
            (v, b)
        })
        .join()
        .unwrap();
        assert_eq!(fields(b.as_ref()), fields(v.as_ref()));
        // Freed on another thread than the one that allocated it.
        thread::spawn(move || drop(b)).join().unwrap();
    }

    #[test]
    fn shared_handles_read_concurrently() {
        let v = d(FIELDS[1]);
        let boxed = HeapBox::<ComponentDescriptor>::new_zeroed().unwrap();
        let (vr, br) = (v.as_ref(), boxed.as_ref());
        thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(move || {
                    for _ in 0..1000 {
                        assert_eq!(fields(vr), FIELDS[1]);
                        assert_eq!(br.get(), ComponentDescriptor::new());
                    }
                });
            }
        });
    }

    #[test]
    fn exclusive_handles_write_on_other_threads() {
        let mut values = [ComponentDescriptor::new(); 4];
        let mut boxed = HeapBox::<ComponentDescriptor>::new_zeroed().unwrap();
        thread::scope(|s| {
            for (i, v) in values.iter_mut().enumerate() {
                let mut m = v.as_mut();
                s.spawn(move || m.set_plane(i as i32));
            }
            let mut m = boxed.as_mut();
            s.spawn(move || m.set_depth(i32::MIN));
        });
        let planes: Vec<i32> = values.iter().map(|v| v.as_ref().plane()).collect();
        assert_eq!(planes, [0, 1, 2, 3]);
        assert_eq!(fields(boxed.as_ref()), [0, 0, 0, 0, i32::MIN]);
    }

    #[test]
    fn a_shared_exclusive_handle_only_reads() {
        let mut v = d(FIELDS[2]);
        let m = v.as_mut();
        let m = &m;
        thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(move || assert_eq!(fields(m.as_ref()), FIELDS[2]));
            }
        });
    }

    #[test]
    fn heap_array_elements_written_from_separate_threads() {
        let mut a = alloc_cell_array_zeroed::<ComponentDescriptor>(4).unwrap();
        let mut handles = a.as_handles_mut();
        thread::scope(|s| {
            // Each element is reached through its own exclusive handle.
            for (m, f) in handles.iter_mut().zip(FIELDS) {
                let mut m = m;
                s.spawn(move || fill(&mut m, f));
            }
        });
        let read: Vec<[i32; 5]> = a.as_handles().iter().map(fields).collect();
        assert_eq!(read, FIELDS[..4]);
    }
}

/// The color property lookups by name (`ColorRange::from_name` and its
/// siblings): empty, long, non-UTF-8 and near-miss names in exactly sized
/// heap strings (so a read past the NUL is an ASan report), the names C's
/// tables hold, and lookups on several threads at once.
mod color_names {
    use core::ffi::CStr;
    use std::ffi::CString;
    use std::thread;

    use ffmpeg::libavutil::pixfmt::{
        ChromaLocation, ColorPrimaries, ColorRange, ColorSpace, ColorTransfer,
    };

    /// Every name C's tables hold, each kind's names together, with the
    /// spellings around them a lookup must refuse.
    const RANGES: [&str; 3] = ["unknown", "tv", "pc"];
    const PRIMARIES: [&str; 14] = [
        "reserved",
        "bt709",
        "unknown",
        "bt470m",
        "bt470bg",
        "smpte170m",
        "smpte240m",
        "film",
        "bt2020",
        "smpte428",
        "smpte431",
        "smpte432",
        "ebu3213",
        "vgamut",
    ];
    const TRANSFERS: [&str; 20] = [
        "reserved",
        "bt709",
        "unknown",
        "bt470m",
        "bt470bg",
        "smpte170m",
        "smpte240m",
        "linear",
        "log100",
        "log316",
        "iec61966-2-4",
        "bt1361e",
        "iec61966-2-1",
        "bt2020-10",
        "bt2020-12",
        "smpte2084",
        "smpte428",
        "arib-std-b67",
        "vlog",
        "unspecified",
    ];
    const SPACES: [&str; 18] = [
        "gbr",
        "bt709",
        "unknown",
        "reserved",
        "fcc",
        "bt470bg",
        "smpte170m",
        "smpte240m",
        "ycgco",
        "bt2020nc",
        "bt2020c",
        "smpte2085",
        "chroma-derived-nc",
        "chroma-derived-c",
        "ictcp",
        "ipt-c2",
        "ycgco-re",
        "ycgco-ro",
    ];
    const LOCATIONS: [&str; 7] = [
        "unspecified",
        "left",
        "center",
        "topleft",
        "top",
        "bottomleft",
        "bottom",
    ];

    /// Whether each property's lookup finds `name`.
    fn found(name: &CStr) -> [bool; 5] {
        [
            ColorRange::from_name(name).is_some(),
            ColorPrimaries::from_name(name).is_some(),
            ColorTransfer::from_name(name).is_some(),
            ColorSpace::from_name(name).is_some(),
            ChromaLocation::from_name(name).is_some(),
        ]
    }

    /// `bytes` as an exactly sized heap C string.
    fn heap(bytes: &[u8]) -> CString {
        CString::new(bytes).unwrap()
    }

    #[test]
    fn degenerate_names_are_refused() {
        for name in [
            &b""[..],
            b" ",
            b"\xff",
            b"\x01\x02\x03",
            b"bt709 ",
            b"BT709",
            b"bt70",
            b"gbrx",
        ] {
            assert_eq!(found(&heap(name)), [false; 5], "{name:?}");
        }
        assert_eq!(found(&heap(&vec![b'b'; 1 << 16])), [false; 5]);
        let long = [b"bt709".as_slice(), &vec![b'9'; 8192]].concat();
        assert_eq!(found(&heap(&long)), [false; 5]);
    }

    #[test]
    fn table_names_are_found_by_their_property() {
        let tables: [&[&str]; 5] = [&RANGES, &PRIMARIES, &TRANSFERS[..19], &SPACES, &LOCATIONS];
        for (i, table) in tables.into_iter().enumerate() {
            for name in table {
                assert!(found(&heap(name.as_bytes()))[i], "property {i}, {name}");
            }
        }
        assert_eq!(found(c"unknown"), [true, true, true, true, false]);
        assert_eq!(found(c"unspecified"), [false, false, false, false, true]);
        assert_eq!(found(c"vgamut"), [false, true, false, false, false]);
        assert_eq!(found(c"vlog"), [false, false, true, false, false]);
    }

    #[test]
    fn lookups_run_on_many_threads() {
        let names: Vec<CString> = RANGES
            .iter()
            .chain(&PRIMARIES)
            .chain(&TRANSFERS)
            .chain(&SPACES)
            .chain(&LOCATIONS)
            .map(|n| heap(n.as_bytes()))
            .collect();
        let expect: Vec<[bool; 5]> = names.iter().map(|n| found(n)).collect();
        thread::scope(|s| {
            for t in 0..8 {
                let (names, expect) = (&names, &expect);
                s.spawn(move || {
                    for (name, e) in names.iter().zip(expect).cycle().skip(t).take(200) {
                        assert_eq!(&found(name), e);
                    }
                });
            }
        });
    }
}

/// [`PixelFormat::swap_endianness`] for every value, named or not, and from
/// several threads at once.
///
/// [`PixelFormat::swap_endianness`]: ffmpeg::libavutil::pixfmt::PixelFormat::swap_endianness
mod swap_endianness {
    use std::thread;

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::pixfmt::PixelFormat;

    fn values() -> impl Iterator<Item = PixelFormat> {
        (-4..300)
            .chain([i32::MIN, i32::MIN + 1, 1 << 24, i32::MAX - 1, i32::MAX])
            .map(|raw| PixelFormat::from_raw(ffi::AVPixelFormat(raw)))
    }

    #[test]
    fn every_value_swaps_or_refuses() {
        for f in values() {
            if let Some(g) = f.swap_endianness() {
                assert_ne!(g, f);
                assert_eq!(g.swap_endianness(), Some(f));
            }
        }
    }

    #[test]
    fn swaps_from_threads_at_once() {
        let expect: Vec<_> = values().map(PixelFormat::swap_endianness).collect();
        thread::scope(|s| {
            for _ in 0..8 {
                let expect = &expect;
                s.spawn(move || {
                    let got: Vec<_> = values().map(PixelFormat::swap_endianness).collect();
                    assert_eq!(&got, expect);
                });
            }
        });
    }
}

/// The value-to-name getters, `AlphaMode::from_name`, the chroma position
/// conversions and the pixel format name lookups: every value including the
/// `int`/`unsigned` extremes, names in exactly sized heap strings (so a read
/// past the NUL is an ASan report) up to well past C's 32-byte copy, the
/// `'static` names kept across threads and later calls, and the calls on
/// several threads at once.
mod names_and_positions {
    use core::ffi::CStr;
    use std::ffi::CString;
    use std::thread;

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::pixfmt::{
        AlphaMode, ChromaLocation, ColorPrimaries, ColorRange, ColorSpace, ColorTransfer,
        PixelFormat,
    };

    fn unsigned_values() -> impl Iterator<Item = u32> {
        (0..300).chain([
            511,
            512,
            1 << 16,
            i32::MAX as u32,
            1 << 31,
            u32::MAX - 1,
            u32::MAX,
        ])
    }

    fn signed_values() -> impl Iterator<Item = i32> {
        let nb = ffi::AVPixelFormat::AV_PIX_FMT_NB.0;
        (-3..nb + 3).chain([i32::MIN, i32::MIN + 1, 1 << 16, i32::MAX])
    }

    /// Every name any getter returns for `raw`.
    fn names(raw: u32) -> [Option<&'static CStr>; 6] {
        [
            ColorRange::from_raw(ffi::AVColorRange(raw)).name(),
            ColorPrimaries::from_raw(ffi::AVColorPrimaries(raw)).name(),
            ColorTransfer::from_raw(ffi::AVColorTransferCharacteristic(raw)).name(),
            ColorSpace::from_raw(ffi::AVColorSpace(raw)).name(),
            ChromaLocation::from_raw(ffi::AVChromaLocation(raw)).name(),
            AlphaMode::from_raw(ffi::AVAlphaMode(raw)).name(),
        ]
    }

    /// Every name a getter returns, kept for the whole test.
    fn all_names() -> Vec<&'static CStr> {
        let props = unsigned_values().flat_map(names).flatten();
        let formats =
            signed_values().filter_map(|r| PixelFormat::from_raw(ffi::AVPixelFormat(r)).name());
        props.chain(formats).collect()
    }

    fn heap(bytes: &[u8]) -> CString {
        CString::new(bytes).unwrap()
    }

    #[test]
    fn getters_accept_every_value() {
        for raw in unsigned_values() {
            for name in names(raw).into_iter().flatten() {
                assert!(!name.is_empty() && name.to_str().is_ok());
            }
            let loc = ChromaLocation::from_raw(ffi::AVChromaLocation(raw));
            if let Some((x, y)) = loc.position() {
                assert!([0, 128, 256].contains(&x) && [0, 128, 256].contains(&y));
            }
        }
        for raw in signed_values() {
            if let Some(name) = PixelFormat::from_raw(ffi::AVPixelFormat(raw)).name() {
                assert!(name.to_bytes().len() < 32);
            }
        }
        for x in [i32::MIN, -1, 0, 128, 256, i32::MAX] {
            for y in [i32::MIN, -1, 0, 128, 256, i32::MAX] {
                if let Some(loc) = ChromaLocation::from_position(x, y) {
                    assert_eq!(loc.position(), Some((x, y)));
                }
            }
        }
    }

    #[test]
    fn static_names_outlive_every_later_call() {
        let kept = all_names();
        let copies: Vec<CString> = kept.iter().map(|n| (*n).to_owned()).collect();
        // Many unrelated calls later, on other threads too, the borrowed
        // names still read the same bytes.
        thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(|| all_names().len());
            }
        });
        let _ = all_names();
        for (name, copy) in kept.iter().zip(&copies) {
            assert_eq!(*name, copy.as_c_str());
        }
        let moved = thread::spawn(move || kept.iter().map(|n| n.to_bytes().len()).sum::<usize>())
            .join()
            .unwrap();
        assert!(moved > 0);
    }

    #[test]
    fn lookups_read_names_of_every_length_in_bounds() {
        let mut names: Vec<Vec<u8>> = vec![b"".to_vec(), b"\xff".to_vec()];
        for len in [1, 2, 28, 29, 30, 31, 32, 33, 64, 4096] {
            names.push(vec![b'g'; len]);
            names.push([b"gray16".as_slice(), &vec![b'x'; len]].concat());
        }
        for name in all_names() {
            let n = name.to_bytes();
            names.push(n.to_vec());
            names.push(n[..n.len() - 1].to_vec());
            names.push([n, b"le"].concat());
        }
        for bytes in names {
            let name = heap(&bytes);
            let _ = PixelFormat::from_name(&name);
            let _ = AlphaMode::from_name(&name);
        }
        assert_eq!(PixelFormat::from_name(&heap(&[b'x'; 4096])), None);
    }

    #[test]
    fn calls_run_on_many_threads() {
        let expect: Vec<_> = unsigned_values().map(names).collect();
        let formats: Vec<_> = signed_values()
            .map(|r| PixelFormat::from_raw(ffi::AVPixelFormat(r)))
            .collect();
        thread::scope(|s| {
            for t in 0..8 {
                let (expect, formats) = (&expect, &formats);
                s.spawn(move || {
                    for (raw, e) in unsigned_values().zip(expect).skip(t) {
                        assert_eq!(&names(raw), e);
                        let loc = ChromaLocation::from_raw(ffi::AVChromaLocation(raw));
                        if let Some((x, y)) = loc.position() {
                            assert_eq!(ChromaLocation::from_position(x, y), Some(loc));
                        }
                    }
                    for f in formats.iter().skip(t) {
                        if let Some(name) = f.name() {
                            assert_eq!(PixelFormat::from_name(name), Some(*f));
                        }
                    }
                    for name in [c"premultiplied", c"straight", c"bogus"] {
                        let _ = AlphaMode::from_name(name);
                    }
                });
            }
        });
    }
}

/// `PixelFormat::conversion_loss`, `best_of_two` and
/// `best_of_two_with_loss` on every pair of values, the `int` extremes and
/// every ignore set's sign and width included (C's scoring shifts and
/// subtracts by descriptor fields, which UBSan watches), and on several
/// threads at once.
mod pixel_format_loss {
    use std::thread;

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::pixdesc::Loss;
    use ffmpeg::libavutil::pixfmt::PixelFormat;

    fn formats() -> Vec<PixelFormat> {
        let nb = ffi::AVPixelFormat::AV_PIX_FMT_NB.0;
        (-2..nb + 2)
            .chain([i32::MIN, i32::MAX])
            .map(|r| PixelFormat::from_raw(ffi::AVPixelFormat(r)))
            .collect()
    }

    #[test]
    fn every_pair_scores_without_ub() {
        let all = formats();
        for &src in &all {
            for &dst in &all {
                for has_alpha in [false, true] {
                    match src.conversion_loss(dst, has_alpha) {
                        Ok(loss) => assert!(loss.bits() >= 0 && loss.bits() < 0x100),
                        Err(e) => assert!((-4..=-1).contains(&e), "{e}"),
                    }
                }
                let best = src.best_of_two(dst, src, true);
                assert!(best == dst || best == src);
            }
        }
    }

    #[test]
    fn every_ignore_set_is_accepted() {
        let all = formats();
        for bits in [0, 1, 0xff, 0x100, i32::MAX, i32::MIN, -1, -2] {
            let ignore = Loss::from_bits_retain(bits);
            for (i, &src) in all.iter().enumerate() {
                let d1 = all[(i * 7) % all.len()];
                let d2 = all[(i * 13 + 5) % all.len()];
                let (best, loss) = src.best_of_two_with_loss(d1, d2, i % 2 == 0, ignore);
                assert!(best == d1 || best == d2);
                if let Ok(loss) = loss {
                    assert!(loss.bits() >= 0);
                }
            }
        }
    }

    #[test]
    fn scoring_runs_on_many_threads() {
        let all = formats();
        let expect: Vec<_> = all
            .iter()
            .map(|&f| {
                f.best_of_two_with_loss(
                    PixelFormat::YUV420P,
                    PixelFormat::RGBA,
                    true,
                    Loss::empty(),
                )
            })
            .collect();
        thread::scope(|s| {
            for t in 0..8 {
                let (all, expect) = (&all, &expect);
                s.spawn(move || {
                    for (f, e) in all.iter().zip(expect).skip(t) {
                        let got = f.best_of_two_with_loss(
                            PixelFormat::YUV420P,
                            PixelFormat::RGBA,
                            true,
                            Loss::empty(),
                        );
                        assert_eq!(&got, e);
                        let _ = f.conversion_loss(PixelFormat::GRAY8, t % 2 == 0);
                    }
                });
            }
        });
    }
}
