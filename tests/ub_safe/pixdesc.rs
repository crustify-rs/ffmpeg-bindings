//! UB tests for `libavutil/pixdesc.h`, through the safe API only.
#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    reason = "test inputs are chosen within range, and the casts mirror the C conversions under comparison"
)]
#![expect(
    clippy::many_single_char_names,
    reason = "names follow the C fields, formulas and fixtures they mirror"
)]
#![expect(
    clippy::assert_is_empty,
    let_underscore_drop,
    reason = "assertions keep the form of the C behaviour they check"
)]

/// `ComponentDescriptor`: inline values, `av_malloc`-family owners and arrays
/// of them (the shape of a descriptor's `comp[4]`), every handle form, copies
/// outliving their source, and the asserted `Send` / `Sync`.
mod component_descriptor {
    use std::thread;

    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::mem::{HeapBox, alloc_cell_array_zeroed};
    use ffmpeg_bindings::libavutil::libavutil::pixdesc::{
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
    #[cfg_attr(miri, ignore = "calls C")]
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
        #[expect(clippy::clone_on_copy, reason = "exercises the explicit `Clone` impl")]
        let c = Clone::clone(&b);
        assert_eq!(fields(copy.as_ref()), FIELDS[1]);
        assert_eq!(fields(c.as_ref()), [0, 2, -1, -1, 5]);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
/// heap strings (so a read past the NUL is an `ASan` report), the names C's
/// tables hold, and lookups on several threads at once.
mod color_names {
    use core::ffi::CStr;
    use std::ffi::CString;
    use std::thread;

    use ffmpeg_bindings::libavutil::libavutil::pixfmt::{
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
/// [`PixelFormat::swap_endianness`]: ffmpeg_bindings::libavutil::libavutil::pixfmt::PixelFormat::swap_endianness
mod swap_endianness {
    use std::thread;

    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::pixfmt::PixelFormat;

    fn values() -> impl Iterator<Item = PixelFormat> {
        (-4..300)
            .chain([i32::MIN, i32::MIN + 1, 1 << 24, i32::MAX - 1, i32::MAX])
            .map(|raw| PixelFormat::from_raw(ffi::AVPixelFormat(raw)))
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn every_value_swaps_or_refuses() {
        for f in values() {
            if let Some(g) = f.swap_endianness() {
                assert_ne!(g, f);
                assert_eq!(g.swap_endianness(), Some(f));
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
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
/// past the NUL is an `ASan` report) up to well past C's 32-byte copy, the
/// `'static` names kept across threads and later calls, and the calls on
/// several threads at once.
mod names_and_positions {
    use core::ffi::CStr;
    use std::ffi::CString;
    use std::thread;

    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::pixfmt::{
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
/// subtracts by descriptor fields, which `UBSan` watches), and on several
/// threads at once.
mod pixel_format_loss {
    use std::thread;

    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::pixdesc::Loss;
    use ffmpeg_bindings::libavutil::libavutil::pixfmt::PixelFormat;

    fn formats() -> Vec<PixelFormat> {
        let nb = ffi::AVPixelFormat::AV_PIX_FMT_NB.0;
        (-2..nb + 2)
            .chain([i32::MIN, i32::MAX])
            .map(|r| PixelFormat::from_raw(ffi::AVPixelFormat(r)))
            .collect()
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
    fn every_ignore_set_is_accepted() {
        let all = formats();
        for bits in [0, 1, 0xFF, 0x100, i32::MAX, i32::MIN, -1, -2] {
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
    #[cfg_attr(miri, ignore = "calls C")]
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

/// `PixelFormatDescriptor`: inline values, `av_malloc`-family owners and
/// arrays of them, every handle form and its component runs, strings from
/// literals and from exactly sized heap allocations (so a read past the NUL
/// is an `ASan` report), names that outlive the descriptor they were read
/// from, copies outliving their source, and the asserted `Send` / `Sync`.
/// Tests without `heap` in their name make no C call and also run under
/// Miri.
mod pixel_format_descriptor {
    use core::ffi::CStr;
    use std::ffi::CString;
    use std::sync::Mutex;
    use std::thread;

    use ffmpeg_bindings::libavutil::libavutil::mem::{HeapBox, alloc_cell_array_zeroed};
    use ffmpeg_bindings::libavutil::libavutil::pixdesc::{
        ComponentDescriptor, PixelFormatDescriptor, PixelFormatDescriptorMut,
        PixelFormatDescriptorRef, PixelFormatFlags,
    };

    /// `bytes` as an exactly sized heap C string that lives for the rest of
    /// the program, kept reachable from a static so that neither
    /// `LeakSanitizer` nor Miri reports it.
    fn leaked(bytes: &[u8]) -> &'static CStr {
        static KEEP: Mutex<Vec<&'static CStr>> = Mutex::new(Vec::new());
        let s: &'static CStr = Box::leak(CString::new(bytes).unwrap().into_boxed_c_str());
        KEEP.lock().unwrap().push(s);
        s
    }

    /// Scalar fields at every edge: component count, chroma shifts, flags.
    const SCALARS: [(u8, u8, u8, u64); 4] = [
        (0, 0, 0, 0),
        (3, 1, 1, 1 << 4),
        (4, 2, 0, 1 << 5 | 1 << 7),
        (u8::MAX, u8::MAX, u8::MAX, u64::MAX),
    ];

    /// Everything the getters read, strings by content.
    type Snapshot = (
        Option<Vec<u8>>,
        Option<Vec<u8>>,
        u8,
        u8,
        u8,
        u64,
        [[i32; 5]; 4],
    );

    fn snapshot(d: PixelFormatDescriptorRef<'_>) -> Snapshot {
        let comps = d.components();
        (
            d.name().map(|s| s.to_bytes().to_vec()),
            d.alias().map(|s| s.to_bytes().to_vec()),
            d.component_count(),
            d.log2_chroma_w(),
            d.log2_chroma_h(),
            d.flags().bits(),
            core::array::from_fn(|i| {
                let c = comps.get(i).unwrap();
                [c.plane(), c.step(), c.offset(), c.shift(), c.depth()]
            }),
        )
    }

    /// Writes every field of `m`: the strings, the scalars of `SCALARS[k]`,
    /// and component `i` = `i + k` in every component field.
    fn fill(m: &mut PixelFormatDescriptorMut<'_>, name: Option<&'static CStr>, k: usize) {
        let (count, w, h, flags) = SCALARS[k % SCALARS.len()];
        m.set_name(name);
        m.set_alias(name);
        m.set_component_count(count);
        m.set_log2_chroma_w(w);
        m.set_log2_chroma_h(h);
        m.set_flags(PixelFormatFlags::from_bits_retain(flags));
        let mut comps = m.components_mut();
        for i in 0..4 {
            let v = (i + k) as i32;
            let mut c = comps.get_mut(i).unwrap();
            c.set_plane(v);
            c.set_step(-v);
            c.set_offset(i32::MIN + v);
            c.set_shift(i32::MAX - v);
            c.set_depth(v);
        }
        assert!(comps.get_mut(4).is_none());
    }

    #[test]
    fn every_field_edge_builds_and_reads_back() {
        let names = [
            None,
            Some(c""),
            Some(c"yuv420p"),
            Some(leaked(b"")),
            Some(leaked(b"\xff\xfe")),
            Some(leaked(&vec![b'n'; 4096])),
        ];
        for (k, name) in names.into_iter().enumerate() {
            let mut d = PixelFormatDescriptor::new();
            fill(&mut d.as_mut(), name, k);
            let s = snapshot(d.as_ref());
            assert_eq!(s.0.as_deref(), name.map(CStr::to_bytes));
            assert_eq!(s.1, s.0);
            let (count, w, h, flags) = SCALARS[k % SCALARS.len()];
            assert_eq!((s.2, s.3, s.4, s.5), (count, w, h, flags));
            assert_eq!(
                s.6[3],
                [
                    3 + k as i32,
                    -3 - k as i32,
                    i32::MIN + 3 + k as i32,
                    i32::MAX - 3 - k as i32,
                    3 + k as i32
                ]
            );
            assert!(!format!("{d:?}").is_empty());
            let raw = d.into_raw();
            assert_eq!(raw.name, name.map_or(core::ptr::null(), CStr::as_ptr));
        }
        assert_eq!(
            snapshot(PixelFormatDescriptor::default().as_ref()),
            snapshot(PixelFormatDescriptor::new().as_ref())
        );
    }

    #[test]
    fn names_outlive_the_descriptor_they_were_read_from() {
        let name = leaked(b"gray10le");
        let (n, a) = {
            let mut d = PixelFormatDescriptor::new();
            d.as_mut().set_name(Some(name));
            d.as_mut().set_alias(Some(c"y10"));
            (d.as_ref().name(), d.as_ref().alias())
            // `d` goes out of scope; the strings are the program's.
        };
        assert_eq!(n.unwrap().to_bytes(), b"gray10le");
        assert_eq!(a, Some(c"y10"));
        // And move into another descriptor.
        let mut e = PixelFormatDescriptor::new();
        e.as_mut().set_name(n);
        assert_eq!(e.as_ref().name().unwrap().as_ptr(), name.as_ptr());
    }

    #[test]
    fn shared_handles_and_component_runs_copy_and_coexist() {
        let mut v = PixelFormatDescriptor::new();
        fill(&mut v.as_mut(), Some(c"rgb24"), 1);
        let a = v.as_ref();
        let b = a;
        let run = a.components();
        let run2 = run;
        let elem = run.get(2).unwrap();
        let (lo, hi) = run.split_at(1).unwrap();
        assert_eq!((lo.len(), hi.len()), (1, 3));
        assert!(run.split_at(5).is_none());
        assert_eq!(run.slice(1..3).unwrap().get(1).unwrap().plane(), 3);
        assert!(run.slice(2..5).is_none());
        assert!(run2.get(4).is_none());
        assert_eq!(snapshot(a), snapshot(b));
        assert_eq!(elem.plane(), 3);
        assert_eq!(hi.get(0).unwrap().plane(), 2);
    }

    #[test]
    fn exclusive_handle_reborrows_without_aliasing() {
        let mut v = PixelFormatDescriptor::new();
        let mut m = v.as_mut();
        fill(&mut m.as_mut(), Some(c"nv12"), 2);
        {
            let mut comps = m.components_mut();
            let (mut lo, mut hi) = comps.split_at_mut(2).unwrap();
            lo.get_mut(0).unwrap().set_depth(-1);
            hi.get_mut(1).unwrap().set_depth(i32::MAX);
            assert!(lo.get_mut(2).is_none());
            let mut sub = comps.slice_mut(1..3).unwrap();
            sub.get_mut(0).unwrap().set_plane(9);
        }
        let shared = m.as_ref();
        assert_eq!(shared.components().get(0).unwrap().depth(), -1);
        m.set_name(None);
        let depths: Vec<i32> = v.as_ref().components().iter().map(|c| c.depth()).collect();
        assert_eq!(depths, [-1, 3, 4, i32::MAX]);
        assert_eq!(v.as_ref().components().get(1).unwrap().plane(), 9);
        assert_eq!(v.as_ref().name(), None);
    }

    #[test]
    fn a_consumed_handle_hands_out_its_components_for_its_lifetime() {
        let mut v = PixelFormatDescriptor::new();
        let mut run = v.as_mut().into_components_mut();
        for i in 0..4 {
            run.get_mut(i).unwrap().set(ComponentDescriptor::new());
            run.get_mut(i).unwrap().set_step(i as i32 + 1);
        }
        let mut last = run.into_mut(3).unwrap();
        last.set_depth(16);
        let steps: Vec<i32> = v.as_ref().components().iter().map(|c| c.step()).collect();
        assert_eq!(steps, [1, 2, 3, 4]);
        assert_eq!(v.as_ref().components().get(3).unwrap().depth(), 16);
    }

    #[test]
    fn copies_and_whole_sets_are_independent_of_their_source() {
        let mut a = PixelFormatDescriptor::new();
        fill(&mut a.as_mut(), Some(leaked(b"p010le")), 3);
        let copy = a.as_ref().get();
        a.as_mut().set_component_count(0);
        a.as_mut()
            .components_mut()
            .get_mut(0)
            .unwrap()
            .set_plane(-7);
        assert_eq!(copy.as_ref().component_count(), u8::MAX);
        assert_eq!(copy.as_ref().components().get(0).unwrap().plane(), 3);
        let mut b = PixelFormatDescriptor::new();
        b.as_mut().set(copy);
        assert_eq!(b, copy);
        #[expect(clippy::clone_on_copy, reason = "exercises the explicit `Clone` impl")]
        let c = Clone::clone(&b);
        assert_eq!(snapshot(c.as_ref()), snapshot(copy.as_ref()));
        assert_ne!(a, copy);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn heap_box_owner_lives_reads_writes_clones_and_frees() {
        let mut a = HeapBox::<PixelFormatDescriptor>::new_zeroed().unwrap();
        assert_eq!(
            snapshot(a.as_ref()),
            snapshot(PixelFormatDescriptor::new().as_ref())
        );
        for k in 0..SCALARS.len() {
            fill(
                &mut a.as_mut(),
                Some(leaked(format!("fmt{k}").as_bytes())),
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
        assert_eq!(name.unwrap().to_bytes(), b"fmt3");
        assert_eq!(snapshot(b.as_ref()), snapshot(copy.as_ref()));
        drop(b);
        assert_eq!(snapshot(c.as_ref()), snapshot(copy.as_ref()));
        assert_eq!(copy.as_ref().name(), name);
        drop(HeapBox::<PixelFormatDescriptor>::new_uninit().unwrap());
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn heap_array_handles_and_component_runs_stay_in_bounds() {
        let n = 5;
        let mut arr = alloc_cell_array_zeroed::<PixelFormatDescriptor>(n).unwrap();
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
            assert_eq!(h.components().get(3).unwrap().plane(), 3 + k as i32);
            assert!(h.components().get(4).is_none());
        }
        assert!(arr.as_handles().get(n).is_none());
        let copy = arr.clone();
        drop(arr);
        assert_eq!(copy.as_handles().get(3).unwrap().component_count(), u8::MAX);
        drop(alloc_cell_array_zeroed::<PixelFormatDescriptor>(0).unwrap());
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn heap_values_and_owners_move_across_threads() {
        let mut v = PixelFormatDescriptor::new();
        fill(&mut v.as_mut(), Some(leaked(b"bgra")), 2);
        let b = HeapBox::<PixelFormatDescriptor>::new_zeroed().unwrap();
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
        let mut v = PixelFormatDescriptor::new();
        fill(&mut v.as_mut(), Some(leaked(b"ya16be")), 1);
        let expect = snapshot(v.as_ref());
        let r = v.as_ref();
        let run = r.components();
        thread::scope(|s| {
            for _ in 0..4 {
                let expect = &expect;
                s.spawn(move || {
                    for _ in 0..200 {
                        assert_eq!(&snapshot(r), expect);
                        assert_eq!(run.get(1).unwrap().plane(), 2);
                    }
                });
            }
        });
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn heap_shared_handles_read_concurrently() {
        let mut b = HeapBox::<PixelFormatDescriptor>::new_zeroed().unwrap();
        fill(&mut b.as_mut(), Some(c"gbrp"), 2);
        let r = b.as_ref();
        thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(move || {
                    for _ in 0..200 {
                        assert_eq!(r.name(), Some(c"gbrp"));
                        assert_eq!(r.get().as_ref().component_count(), 4);
                    }
                });
            }
        });
    }

    #[test]
    fn exclusive_handles_write_on_other_threads() {
        let mut values = [PixelFormatDescriptor::new(); 4];
        thread::scope(|s| {
            for (i, v) in values.iter_mut().enumerate() {
                let mut m = v.as_mut();
                s.spawn(move || {
                    m.set_component_count(i as u8);
                    m.set_name(Some(c"x"));
                });
            }
        });
        let counts: Vec<u8> = values
            .iter()
            .map(|v| v.as_ref().component_count())
            .collect();
        assert_eq!(counts, [0, 1, 2, 3]);
    }

    #[test]
    fn component_slots_written_from_separate_threads() {
        let mut v = PixelFormatDescriptor::new();
        let mut run = v.as_mut().into_components_mut();
        thread::scope(|s| {
            for (i, m) in run.iter_mut().enumerate() {
                let mut m = m;
                s.spawn(move || m.set_depth(i as i32 * 8));
            }
        });
        let depths: Vec<i32> = v.as_ref().components().iter().map(|c| c.depth()).collect();
        assert_eq!(depths, [0, 8, 16, 24]);
    }

    #[test]
    fn a_shared_exclusive_handle_only_reads() {
        let mut v = PixelFormatDescriptor::new();
        fill(&mut v.as_mut(), Some(c"monob"), 0);
        let m = v.as_mut();
        let m = &m;
        thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(move || assert_eq!(m.as_ref().name(), Some(c"monob")));
            }
        });
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn heap_array_elements_written_from_separate_threads() {
        let mut arr = alloc_cell_array_zeroed::<PixelFormatDescriptor>(4).unwrap();
        let mut handles = arr.as_handles_mut();
        thread::scope(|s| {
            for (k, m) in handles.iter_mut().enumerate() {
                let mut m = m;
                s.spawn(move || fill(&mut m, Some(c"t"), k));
            }
        });
        let counts: Vec<u8> = arr
            .as_handles()
            .iter()
            .map(|h| h.component_count())
            .collect();
        assert_eq!(counts, [0, 3, 4, u8::MAX]);
    }
}

/// UB tests for `PixelFormat`'s descriptor queries: every value C knows and
/// values past its table (on which C would read past it), into exactly
/// sized heap buffers of every length, and on several threads.
mod pixel_format_queries {
    use std::thread;

    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::pixfmt::PixelFormat;

    fn values() -> impl Iterator<Item = PixelFormat> {
        let nb = ffi::AVPixelFormat::AV_PIX_FMT_NB.0;
        (-3..nb + 3)
            .chain([i32::MIN, i32::MAX])
            .map(|raw| PixelFormat::from_raw(ffi::AVPixelFormat(raw)))
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn queries_of_every_value_stay_in_bounds() {
        for f in values() {
            let _ = f.plane_count();
            let _ = f.chroma_subsampling();
            for len in [0, 1, 2, 11, 12, 31, 32, 33] {
                let mut buf = vec![b'#'; len].into_boxed_slice();
                if let Some(s) = f.describe(&mut buf) {
                    assert!(s.to_bytes().len() < len);
                }
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn queries_on_many_threads() {
        thread::scope(|s| {
            for _ in 0..8 {
                s.spawn(|| {
                    let mut buf = [0u8; 40];
                    for f in values() {
                        let _ = f.describe(&mut buf);
                        let _ = f.plane_count();
                    }
                });
            }
        });
    }
}

/// The descriptor table: `'static` handles kept past every call, walked and
/// looked up from many threads, and the whole-descriptor bit counters over
/// Rust-built descriptors at every edge (C indexes `steps[4]` by plane).
mod descriptor_table {
    use std::sync::OnceLock;
    use std::thread;

    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::pixdesc::{
        ComponentDescriptor, PixelFormatDescriptor, PixelFormatDescriptorRef, PixelFormatFlags,
        descriptors,
    };
    use ffmpeg_bindings::libavutil::libavutil::pixfmt::PixelFormat;

    const NB: i32 = ffi::AVPixelFormat::AV_PIX_FMT_NB.0;

    fn format(raw: i32) -> PixelFormat {
        PixelFormat::from_raw(ffi::AVPixelFormat(raw))
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn every_value_looks_up_without_ub() {
        for raw in (-3..NB + 3).chain([i32::MIN, i32::MAX]) {
            if let Some(d) = format(raw).descriptor() {
                assert_eq!(d.pixel_format(), Some(format(raw)));
                let _ = (
                    d.next_in_table(),
                    d.bits_per_pixel(),
                    d.padded_bits_per_pixel(),
                );
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn static_handles_outlive_their_lookups_and_cross_threads() {
        static KEPT: OnceLock<Vec<PixelFormatDescriptorRef<'static>>> = OnceLock::new();
        let kept = KEPT.get_or_init(|| descriptors().collect());
        thread::scope(|s| {
            for t in 0..4 {
                s.spawn(move || {
                    for (i, d) in kept.iter().enumerate().skip(t) {
                        assert!(d.name().is_some());
                        assert_eq!(
                            d.pixel_format()
                                .and_then(PixelFormat::descriptor)
                                .map(|e| e.get()),
                            Some(d.get()),
                            "{i}"
                        );
                        let _ = (d.bits_per_pixel(), d.padded_bits_per_pixel());
                    }
                    assert_eq!(descriptors().count(), kept.len());
                });
            }
        });
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn walking_from_copies_and_fresh_descriptors_stops() {
        let first = descriptors().next().unwrap();
        let name = {
            let copy = first.get();
            assert!(copy.as_ref().next_in_table().is_none());
            // The copy borrowed nothing: its strings are 'static.
            copy.as_ref().name()
        };
        assert!(
            PixelFormatDescriptor::new()
                .as_ref()
                .next_in_table()
                .is_none()
        );
        assert_eq!(name, first.name());
    }

    const EDGES: [i32; 10] = [i32::MIN, -1, 0, 1, 3, 4, 8, 1 << 27, 1 << 28, i32::MAX];

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn bit_counters_take_every_edge_descriptor() {
        let mut accepted = 0;
        for count in [0, 1, 4, 5, u8::MAX] {
            for (w, h) in [(0, 0), (1, 2), (15, 16), (16, 16), (u8::MAX, u8::MAX)] {
                for &plane in &EDGES {
                    for &v in &EDGES {
                        for flags in [PixelFormatFlags::empty(), PixelFormatFlags::BITSTREAM] {
                            let mut d = PixelFormatDescriptor::new();
                            let mut m = d.as_mut();
                            m.set_component_count(count);
                            m.set_log2_chroma_w(w);
                            m.set_log2_chroma_h(h);
                            m.set_flags(flags);
                            for (c, mut slot) in m.components_mut().iter_mut().enumerate() {
                                slot.set(ComponentDescriptor::from_raw(
                                    ffi::AVComponentDescriptor {
                                        plane: if c == 2 { plane } else { c as i32 },
                                        step: v,
                                        offset: v,
                                        shift: v,
                                        depth: if c == 1 { 8 } else { v },
                                    },
                                ));
                            }
                            accepted += usize::from(d.as_ref().bits_per_pixel().is_ok());
                            accepted += usize::from(d.as_ref().padded_bits_per_pixel().is_ok());
                        }
                    }
                }
            }
        }
        assert!(accepted > 0);
    }
}

/// `read_image_line` / `write_image_line` and their `u16` forms: C's table
/// and Rust-built descriptors at every edge, each line on planes sized to
/// exactly the bytes the wrapper checked, so a byte C touches past either
/// end is an `ASan` report, and lines on many threads at once.
mod image_lines {
    use std::thread;

    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::imgutils::{Image, Planes, PlanesMut};
    use ffmpeg_bindings::libavutil::libavutil::pixdesc::{
        ComponentDescriptor, ImageLineError, PixelFormatDescriptor, PixelFormatDescriptorRef,
        PixelFormatFlags, read_image_line, read_image_line_u16, write_image_line,
        write_image_line_u16,
    };
    use ffmpeg_bindings::libavutil::libavutil::pixfmt::PixelFormat;

    const NB: i32 = ffi::AVPixelFormat::AV_PIX_FMT_NB.0;
    /// The largest plane the searches try.
    const CAP: usize = 1 << 12;

    /// The bytes every probe of the searches reads from: one block, so a
    /// search allocates nothing.
    static PROBE: [u8; CAP] = [0x5A; CAP];

    fn under_bsan() -> bool {
        std::env::var("CRUSTIFY_FFMPEG_VARIANT").as_deref() == Ok("bsan")
    }

    /// Planes with `lens` bytes, plane `p`'s rows `linesize` apart.
    fn sized(lens: [usize; 4]) -> [Vec<u8>; 4] {
        lens.map(|n| vec![0x5A; n])
    }

    fn planes(img: &[Vec<u8>; 4], linesize: isize) -> Planes<'_> {
        Planes {
            data: img.each_ref().map(|p| &p[..]),
            linesizes: [linesize; 4],
        }
    }

    /// The shortest length of plane `p` (the others `others` long) for which
    /// `accepts` holds, when one up to `CAP` does: acceptance only grows with
    /// a top-down plane's length.
    fn shortest(p: usize, others: usize, accepts: impl Fn([usize; 4]) -> bool) -> Option<usize> {
        let lens = |n| core::array::from_fn(|i| if i == p { n } else { others });
        if !accepts(lens(CAP)) {
            return None;
        }
        let (mut lo, mut hi) = (0, CAP);
        while lo < hi {
            let mid = usize::midpoint(lo, hi);
            if accepts(lens(mid)) {
                hi = mid;
            } else {
                lo = mid + 1;
            }
        }
        Some(lo)
    }

    /// Every read and write of one line on planes each exactly as long as the
    /// wrapper requires, top-down from row `y`; the number of calls that
    /// reached C.
    fn exercise(
        d: PixelFormatDescriptorRef<'_>,
        (x, y, c, w): (usize, usize, usize, usize),
        linesize: isize,
    ) -> usize {
        let read = |lens: [usize; 4], pal: bool| {
            let probe = Planes {
                data: lens.map(|n| &PROBE[..n]),
                linesizes: [linesize; 4],
            };
            let mut dst = vec![0u32; w];
            read_image_line(&mut dst, probe, d, x, y, c, pal).is_ok()
        };
        let plane = d.components().get(c).map_or(0, |comp| comp.plane());
        let Ok(p) = usize::try_from(plane) else {
            // Refused whatever the planes (C cannot index its arrays), or
            // an absent component, which reads nothing.
            read([CAP; 4], false);
            return 0;
        };
        let Some(len) = shortest(p.min(3), 0, |lens| read(lens, false)) else {
            return 0;
        };
        let p = p.min(3);
        let mut calls = 0;
        let mut lens = [0; 4];
        lens[p] = len;
        let img = sized(lens);
        let mut wide = vec![0u32; w];
        let mut narrow = vec![0u16; w];
        read_image_line(&mut wide, planes(&img, linesize), d, x, y, c, false).unwrap();
        read_image_line_u16(&mut narrow, planes(&img, linesize), d, x, y, c, false).unwrap();
        let mut img = img;
        let mut out = PlanesMut {
            data: img.each_mut().map(|p| &mut p[..]),
            linesizes: [linesize; 4],
        };
        let src: Vec<u32> = (0..w as u32).map(|i| u32::MAX >> i).collect();
        let src16: Vec<u16> = (0..w as u16).map(|i| u16::MAX >> i).collect();
        write_image_line(&src, &mut out, d, x, y, c).unwrap();
        write_image_line(&src16, &mut out, d, x, y, c).unwrap();
        write_image_line_u16(&src16, &mut out, d, x, y, c).unwrap();
        calls += 5;
        // Palette lookups, with plane 1 exactly as long as they need.
        if p != 1 {
            let pal_lens = |n| {
                core::array::from_fn(|i| {
                    if i == p {
                        len
                    } else if i == 1 {
                        n
                    } else {
                        0
                    }
                })
            };
            if let Some(pal) = shortest(1, 0, |l| read(pal_lens(l[1]), true)) {
                let img = sized(pal_lens(pal));
                read_image_line(&mut wide, planes(&img, linesize), d, x, y, c, true).unwrap();
                read_image_line_u16(&mut narrow, planes(&img, linesize), d, x, y, c, true).unwrap();
                calls += 2;
            }
        }
        calls
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn table_lines_touch_only_the_bytes_checked() {
        let mut calls = 0;
        let lines: &[(usize, usize, usize)] = if under_bsan() {
            &[(3, 1, 4)]
        } else {
            &[(0, 0, 1), (3, 1, 4), (1, 2, 9)]
        };
        for raw in 0..NB {
            let Some(d) = PixelFormat::from_raw(ffi::AVPixelFormat(raw)).descriptor() else {
                continue;
            };
            if under_bsan() && raw % 8 != 0 {
                continue;
            }
            for c in 0..4 {
                for &(x, y, w) in lines {
                    calls += exercise(d, (x, y, c, w), 64);
                }
            }
        }
        assert!(calls > 100, "{calls}");
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn bottom_up_lines_stop_at_the_planes_first_byte() {
        // Row 1 of a bottom-up plane is its first: a byte C reads before the
        // line's start is before the allocation.
        for f in [
            PixelFormat::RGB565BE,
            PixelFormat::BGR565BE,
            PixelFormat::MONOBLACK,
            PixelFormat::YUYV422,
            PixelFormat::GRAY16BE,
        ] {
            let d = f.descriptor().unwrap();
            for c in 0..4 {
                let img = sized([65, 65, 65, 65]);
                let mut dst = [0u32; 4];
                match read_image_line(&mut dst, planes(&img, -64), d, 0, 1, c, false) {
                    Ok(()) | Err(ImageLineError::OutsidePlane) => {}
                    Err(e) => panic!("{f:?} {c}: {e:?}"),
                }
                let mut img = img;
                let mut out = PlanesMut {
                    data: img.each_mut().map(|p| &mut p[..]),
                    linesizes: [-64; 4],
                };
                match write_image_line(&[1u16; 4], &mut out, d, 0, 1, c) {
                    Ok(()) | Err(ImageLineError::OutsidePlane) => {}
                    Err(e) => panic!("{f:?} {c}: {e:?}"),
                }
            }
        }
    }

    /// Component field values at every edge of C's arithmetic.
    const PLANES: [i32; 3] = [0, 4, -1];
    const STEPS: [i32; 11] = [i32::MIN, -3, -1, 0, 1, 2, 7, 8, 9, 33, i32::MAX];
    const OFFSETS: [i32; 8] = [i32::MIN, -9, -1, 0, 1, 7, 31, 32];
    const SHIFTS: [i32; 8] = [-1, 0, 1, 7, 8, 16, 31, 32];
    const DEPTHS: [i32; 15] = [-1, 0, 1, 5, 8, 9, 16, 24, 29, 30, 31, 32, 33, 63, 64];

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn edge_descriptors_touch_only_the_bytes_checked() {
        let flag_sets = [
            PixelFormatFlags::empty(),
            PixelFormatFlags::BIG_ENDIAN,
            PixelFormatFlags::BITSTREAM,
            PixelFormatFlags::BITSTREAM | PixelFormatFlags::BIG_ENDIAN,
        ];
        let (mut n, mut calls) = (0usize, 0);
        let thin = if under_bsan() { 61 } else { 1 };
        for plane in PLANES {
            for step in STEPS {
                for offset in OFFSETS {
                    for shift in SHIFTS {
                        for depth in DEPTHS {
                            for flags in flag_sets {
                                n += 1;
                                if n % thin != 0 {
                                    continue;
                                }
                                let mut d = PixelFormatDescriptor::new();
                                let mut m = d.as_mut();
                                m.set_component_count(1);
                                m.set_flags(flags);
                                m.components_mut().get_mut(0).unwrap().set(
                                    ComponentDescriptor::from_raw(ffi::AVComponentDescriptor {
                                        plane,
                                        step,
                                        offset,
                                        shift,
                                        depth,
                                    }),
                                );
                                for (x, w) in [(0, 1), (5, 4)] {
                                    calls += exercise(d.as_ref(), (x, 1, 0, w), 48);
                                }
                            }
                        }
                    }
                }
            }
        }
        assert!(calls > 0, "{calls}");
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn lines_on_many_threads_at_once() {
        // Formats whose every component has the image's full size.
        let formats = [
            PixelFormat::YUV444P,
            PixelFormat::RGB24,
            PixelFormat::MONOWHITE,
            PixelFormat::PAL8,
            PixelFormat::RGB48BE,
            PixelFormat::GBRP,
        ];
        // Shared planes, read from several threads at once.
        let shared = Image::alloc(64, 4, PixelFormat::NV12, 32).unwrap();
        thread::scope(|s| {
            for (t, f) in formats.into_iter().enumerate() {
                s.spawn(move || {
                    let d = f.descriptor().unwrap();
                    let mut image = Image::alloc(33, 9, f, 16).unwrap();
                    for round in 0..20 {
                        let y = (round + t) % 9;
                        for c in 0..4 {
                            let src = [round as u16; 33];
                            write_image_line(&src, &mut image.planes_mut(), d, 0, y, c).unwrap();
                            let mut back = [0u32; 33];
                            read_image_line(&mut back, image.planes(), d, 0, y, c, false).unwrap();
                        }
                    }
                });
            }
            let image = &shared;
            for t in 0..4 {
                s.spawn(move || {
                    let d = PixelFormat::NV12.descriptor().unwrap();
                    // Half-width chroma: 16 pixels of 2 rows.
                    for c in 0..3 {
                        let mut line = [0u16; 16];
                        read_image_line_u16(&mut line, image.planes(), d, t, t % 2, c, false)
                            .unwrap();
                    }
                });
            }
        });
    }
}
