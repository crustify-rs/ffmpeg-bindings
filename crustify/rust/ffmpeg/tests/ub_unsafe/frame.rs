//! UB tests for `libavutil/frame.h`'s unsafe API, each call within its
//! `# Safety` contract.

/// `SideDataDescriptor::from_raw`: adopting C's own table entries, string
/// literals, NULL, and exactly sized leaked heap strings (so a read past the
/// NUL is an ASan report), with the property bits at their edges; adopted
/// values outliving the raw struct they were copied from, in `const`
/// context, and read on several threads. Tests without `c_table` in their
/// name make no C call and also run under Miri.
mod side_data_descriptor_from_raw {
    use core::ffi::{CStr, c_char, c_uint};
    use core::ptr;
    use std::ffi::CString;
    use std::sync::Mutex;
    use std::thread;

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::frame::{SideDataDescriptor, SideDataProps};

    /// `bytes` as an exactly sized heap C string that lives for the rest of
    /// the program, kept reachable from a static so that neither
    /// LeakSanitizer nor Miri reports it.
    fn leaked(bytes: &[u8]) -> *const c_char {
        static KEEP: Mutex<Vec<&'static CStr>> = Mutex::new(Vec::new());
        let s: &'static CStr = Box::leak(CString::new(bytes).unwrap().into_boxed_c_str());
        KEEP.lock().unwrap().push(s);
        s.as_ptr()
    }

    /// Asserts every getter of `d` reads what `r` stores.
    fn assert_reads(d: &SideDataDescriptor, r: &ffi::AVSideDataDescriptor) {
        let h = d.as_ref();
        assert_eq!(h.name().map_or(ptr::null(), CStr::as_ptr), r.name);
        assert_eq!(h.props(), SideDataProps::from_bits_retain(r.props));
        let back = d.into_raw();
        assert_eq!((back.name, back.props), (r.name, r.props));
        assert_eq!(h.get(), *d);
        assert!(!format!("{d:?}").is_empty());
    }

    #[test]
    fn literal_null_and_leaked_strings_with_edge_props() {
        let names: [*const c_char; 7] = [
            ptr::null(),
            c"".as_ptr(),
            c"Stereo 3D".as_ptr(),
            leaked(b""),
            leaked(b"x"),
            leaked(b"\x80\xff"),
            leaked(&vec![b'z'; 8192]),
        ];
        let props = [0, 1, 0x1f, 0x20, 0x8000_0000, c_uint::MAX];
        for &name in &names {
            for &p in &props {
                let r = ffi::AVSideDataDescriptor { name, props: p };
                // SAFETY: `name` is NULL, a literal, or a leaked heap string,
                // none ever freed or written.
                let d = unsafe { SideDataDescriptor::from_raw(r) };
                assert_reads(&d, &r);
                if !name.is_null() {
                    // SAFETY: as above.
                    let expect = unsafe { CStr::from_ptr(name) };
                    assert_eq!(d.as_ref().name(), Some(expect));
                    assert_eq!(
                        d.as_ref().name().unwrap().count_bytes(),
                        expect.count_bytes()
                    );
                }
            }
        }
    }

    #[test]
    fn adopted_values_outlive_the_raw_struct_and_its_frame() {
        fn adopt() -> SideDataDescriptor {
            let r = Box::new(ffi::AVSideDataDescriptor {
                name: leaked(b"Spherical Mapping"),
                props: 5,
            });
            // SAFETY: a leaked heap string, never freed or written.
            unsafe { SideDataDescriptor::from_raw(*r) }
            // `r`'s box is freed here; the value holds its own copy.
        }
        let (name, copy) = {
            let d = adopt();
            (d.as_ref().name(), d.as_ref().get())
            // `d` goes out of scope; the string is the program's.
        };
        assert_eq!(name.unwrap().to_bytes(), b"Spherical Mapping");
        assert_eq!(copy.as_ref().name(), name);
        // Adopted twice from one raw struct: two independent values.
        let r = ffi::AVSideDataDescriptor {
            name: c"a".as_ptr(),
            props: 2,
        };
        // SAFETY: a literal.
        let (mut a, b) = unsafe {
            (
                SideDataDescriptor::from_raw(r),
                SideDataDescriptor::from_raw(r),
            )
        };
        a.as_mut().set_props(SideDataProps::empty());
        assert_eq!(b.as_ref().props(), SideDataProps::MULTI);
        assert_ne!(a, b);
    }

    /// A descriptor adopted at compile time, as a `const`.
    const EXIF: SideDataDescriptor = {
        let raw = ffi::AVSideDataDescriptor {
            name: c"EXIF metadata".as_ptr(),
            props: 1,
        };
        // SAFETY: the name is a literal.
        unsafe { SideDataDescriptor::from_raw(raw) }
    };

    #[test]
    fn adopted_in_const_context() {
        let d = EXIF;
        assert_eq!(d.as_ref().name(), Some(c"EXIF metadata"));
        assert_eq!(d.as_ref().props(), SideDataProps::GLOBAL);
    }

    #[test]
    fn adopted_values_cross_and_are_shared_between_threads() {
        let r = ffi::AVSideDataDescriptor {
            name: leaked(b"Downmix Matrix"),
            props: 16,
        };
        // SAFETY: a leaked heap string.
        let d = unsafe { SideDataDescriptor::from_raw(r) };
        let back = thread::spawn(move || {
            assert_eq!(d.as_ref().name().unwrap().to_bytes(), b"Downmix Matrix");
            d
        })
        .join()
        .unwrap();
        assert_reads(&back, &r);
        let h = back.as_ref();
        thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(move || {
                    for _ in 0..100 {
                        assert_eq!(h.name().unwrap().to_bytes(), b"Downmix Matrix");
                        assert_eq!(h.props(), SideDataProps::CHANNEL_DEPENDENT);
                    }
                });
            }
        });
    }

    #[test]
    fn c_table_entries_adopt_and_read_back() {
        let mut adopted = 0;
        for t in (0..256).chain([c_uint::MAX >> 1, c_uint::MAX]) {
            // SAFETY: `av_frame_side_data_desc` takes any value and returns
            // NULL or a pointer into C's static, never-written table.
            let p = unsafe { ffi::av_frame_side_data_desc(ffi::AVFrameSideDataType(t)) };
            if p.is_null() {
                continue;
            }
            // SAFETY: `p` points to a live entry of C's static table.
            let r = unsafe { p.read() };
            // SAFETY: C's table entries' names are string literals.
            let d = unsafe { SideDataDescriptor::from_raw(r) };
            assert_reads(&d, &r);
            assert!(!d.as_ref().name().unwrap().is_empty());
            adopted += 1;
        }
        assert_eq!(
            adopted,
            ffi::AVFrameSideDataType::AV_FRAME_DATA_DOWNMIX_MATRIX.0 + 1
        );
    }
}

/// `SideDataArray::from_raw_parts`: adopting arrays C built, within the
/// contract, at its edges (NULL and 0, empty payloads, page-sized ones,
/// entries sharing buffers across two arrays, references C keeps for the
/// caller, arrays C shrank), round trips through `into_raw_parts` with C
/// changing the array in between, and adopted arrays read, written and
/// released on other threads, in every drop order (buffer-frame wave 2,
/// batch 0).
mod side_data_array_from_raw_parts {
    use core::ffi::c_int;
    use core::ptr;
    use std::thread;

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::frame::{SideDataArray, SideDataFlags, SideDataProps, SideDataType};

    type Parts = (*mut *mut ffi::AVFrameSideData, c_int);

    /// Whether every byte of `bytes` is `b`, compared as one slice: BSan
    /// retags once per element an iterator yields.
    fn is_all(bytes: &[u8], b: u8) -> bool {
        *bytes == *vec![b; bytes.len()]
    }

    /// C appends an entry of `kind` holding `len` bytes, all set to `byte`,
    /// and a `key` metadata pair when given one.
    fn push(
        parts: &mut Parts,
        kind: SideDataType,
        len: usize,
        byte: u8,
        key: Option<&core::ffi::CStr>,
    ) {
        // SAFETY: the array C built so far, or NULL and 0; the specs never
        // repeat a non-MULTI kind.
        let e = unsafe {
            ffi::av_frame_side_data_new(&mut parts.0, &mut parts.1, kind.into_raw(), len, 0)
        };
        assert!(!e.is_null());
        // SAFETY: the fresh entry's sole payload of `len` bytes, written in
        // full, and its metadata slot.
        unsafe {
            ptr::write_bytes((*e).data, byte, len);
            if let Some(key) = key {
                assert!(
                    ffi::av_dict_set(&raw mut (*e).metadata, key.as_ptr(), c"v".as_ptr(), 0) >= 0
                );
            }
        }
    }

    /// A C array of one entry per length, alternating `MULTI` and unique
    /// kinds C does not name.
    fn built(lens: &[usize]) -> Parts {
        let mut parts: Parts = (ptr::null_mut(), 0);
        for (i, &len) in lens.iter().enumerate() {
            let kind = if i % 2 == 0 {
                SideDataType::SEI_UNREGISTERED
            } else {
                SideDataType::from_raw(ffi::AVFrameSideDataType(5000 + i as u32))
            };
            push(
                &mut parts,
                kind,
                len,
                i as u8,
                (i % 3 == 0).then_some(c"key"),
            );
        }
        parts
    }

    fn check(a: &SideDataArray, lens: &[usize]) {
        assert_eq!(a.len(), lens.len());
        for (i, (e, &len)) in a.as_ref().iter().zip(lens).enumerate() {
            assert_eq!(e.len(), len);
            assert!(is_all(e.data(), i as u8));
            assert_eq!(e.metadata().is_some(), i % 3 == 0);
        }
    }

    #[test]
    fn null_and_zero_is_the_empty_array() {
        // SAFETY: NULL and 0, the empty array C's routines start from.
        let a = unsafe { SideDataArray::from_raw_parts(ptr::null_mut(), 0) };
        assert!(a.is_empty());
        let (sd, nb) = a.into_raw_parts();
        assert!(sd.is_null() && nb == 0);
        // SAFETY: as above, released on drop.
        drop(unsafe { SideDataArray::from_raw_parts(sd, nb) });
    }

    #[test]
    fn exactly_sized_payloads_read_and_write_in_full() {
        let lens = [0, 1, 2, 15, 16, 17, 4095, 4096, 65537];
        let (sd, nb) = built(&lens);
        // SAFETY: an array C built and nothing else holds, every payload
        // written by `push`.
        let mut a = unsafe { SideDataArray::from_raw_parts(sd, nb) };
        check(&a, &lens);
        for (i, mut e) in a.as_mut().iter_mut().enumerate() {
            e.data_mut().unwrap().fill(i as u8);
        }
        check(&a, &lens);
    }

    #[test]
    fn round_trips_with_c_shrinking_the_array_between() {
        let lens = [3, 0, 9, 1, 30, 7];
        let (mut sd, mut nb) = built(&lens);
        for expected in [6, 3, 3] {
            // SAFETY: the array C built or changed last, every payload
            // initialised.
            let a = unsafe { SideDataArray::from_raw_parts(sd, nb) };
            assert_eq!(a.len(), expected);
            (sd, nb) = a.into_raw_parts();
            // SAFETY: the array given back; C frees its MULTI entries (the
            // even ones) the first time round, and none after.
            unsafe {
                ffi::av_frame_side_data_remove_by_props(
                    &mut sd,
                    &mut nb,
                    SideDataProps::MULTI.bits() as c_int,
                )
            };
        }
        // SAFETY: as above.
        let a = unsafe { SideDataArray::from_raw_parts(sd, nb) };
        assert!(
            a.as_ref()
                .iter()
                .all(|e| e.kind() != SideDataType::SEI_UNREGISTERED)
        );
    }

    #[test]
    fn arrays_sharing_buffers_drop_in_either_order() {
        for first in [false, true] {
            let lens = [8, 0, 100, 5];
            let (sd, nb) = built(&lens);
            let mut copy: Parts = (ptr::null_mut(), 0);
            for i in 0..nb as usize {
                // SAFETY: entry `i` of the C array, whose buffer C shares
                // with the new entry and whose metadata it copies.
                let r = unsafe {
                    ffi::av_frame_side_data_clone(&mut copy.0, &mut copy.1, *sd.add(i), 0)
                };
                assert_eq!(r, 0);
            }
            // SAFETY: two arrays C built, each held by one owner; their
            // entries are distinct and share only buffers, by count.
            let (mut a, mut b) = unsafe {
                (
                    SideDataArray::from_raw_parts(sd, nb),
                    SideDataArray::from_raw_parts(copy.0, copy.1),
                )
            };
            check(&a, &lens);
            check(&b, &lens);
            assert!(a.as_mut().iter_mut().all(|mut e| e.data_mut().is_none()));
            if first {
                drop(a);
                assert!(b.as_mut().iter_mut().all(|mut e| e.data_mut().is_some()));
                check(&b, &lens);
            } else {
                drop(b);
                assert!(a.as_mut().iter_mut().all(|mut e| e.data_mut().is_some()));
                check(&a, &lens);
            }
        }
    }

    #[test]
    fn references_c_keeps_for_the_caller_make_payloads_read_only() {
        // SAFETY: a fresh C buffer of 6 bytes, filled.
        let mut buf = unsafe {
            let b = ffi::av_buffer_alloc(6);
            ptr::write_bytes((*b).data, 0x42, 6);
            b
        };
        let mut parts: Parts = (ptr::null_mut(), 0);
        let flags = ffi::AV_FRAME_SIDE_DATA_FLAG_NEW_REF;
        // SAFETY: NULL and 0, and the caller's reference, to which C adds
        // one for the entry and which it leaves the caller.
        let e = unsafe {
            ffi::av_frame_side_data_add(
                &mut parts.0,
                &mut parts.1,
                SideDataType::AFD.into_raw(),
                &mut buf,
                flags,
            )
        };
        assert!(!e.is_null() && !buf.is_null());
        // SAFETY: the array C built, its payload initialised.
        let mut a = unsafe { SideDataArray::from_raw_parts(parts.0, parts.1) };
        let mut entries = a.as_mut();
        let mut entry = entries.get_mut(0).unwrap();
        assert!(entry.data_mut().is_none());
        assert_eq!(entry.as_ref().data(), [0x42; 6]);
        // SAFETY: the caller's reference, released once.
        unsafe { ffi::av_buffer_unref(&mut buf) };
        entry.data_mut().unwrap()[0] = 1;
        assert_eq!(a.as_ref().get(0).unwrap().data()[..2], [1, 0x42]);
        let fresh = ffmpeg::libavutil::buffer::BufferReference::alloc_zeroed(1).unwrap();
        assert!(
            a.as_mut()
                .add(SideDataType::AFD, fresh, SideDataFlags::NONE)
                .is_err()
        );
    }

    #[test]
    fn adopted_arrays_move_to_other_threads() {
        let lens = [10, 20, 0, 40];
        let (sd, nb) = built(&lens);
        // SAFETY: an array C built and nothing else holds.
        let a = unsafe { SideDataArray::from_raw_parts(sd, nb) };
        let a = thread::spawn(move || {
            check(&a, &lens);
            a
        })
        .join()
        .unwrap();
        let (sd, nb) = a.into_raw_parts();
        let addr = (sd as usize, nb);
        thread::spawn(move || {
            // SAFETY: the array given up above, now adopted again here, its
            // only holder.
            let a = unsafe { SideDataArray::from_raw_parts(addr.0 as *mut _, addr.1) };
            check(&a, &lens);
        })
        .join()
        .unwrap();
    }
}

/// `FrameMut::set_data_ptr`, `set_extended_data_ptr` and
/// `replace_hw_frames_context` (buffer-frame wave 3, batch 0): planes of
/// frames that are not reference counted over exactly sized Rust memory
/// that outlives them (so that a read past a plane is an ASan report),
/// read by C through `try_clone`'s copy; pointers into a frame's own
/// buffers; NULL; the planes a later buffer at index 0 clears; and the
/// hardware frames context seam, whose only value this build can produce
/// (it has no hardware device type) is none.
mod frame_type {
    use core::ptr;
    use std::thread;

    use ffmpeg::libavutil::buffer::BufferReference;
    use ffmpeg::libavutil::channel_layout::ChannelLayout;
    use ffmpeg::libavutil::frame::{Frame, FrameBox, NUM_DATA_POINTERS};
    use ffmpeg::libavutil::pixfmt::PixelFormat;
    use ffmpeg::libavutil::samplefmt::SampleFormat;

    /// `len` bytes, each its index times 5, plus 2.
    fn pattern(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i * 5 + 2) as u8).collect()
    }

    /// A gray `w`x`h` frame whose one plane is `mem`, `w` bytes a line.
    ///
    /// # Safety
    ///
    /// `mem` must hold `w * h` bytes and outlive every use of the frame.
    unsafe fn gray(mem: &mut [u8], w: i32, h: i32) -> FrameBox {
        assert!(mem.len() >= (w * h) as usize);
        let mut f = Frame::new().unwrap();
        let mut m = f.as_mut();
        m.set_pixel_format(PixelFormat::GRAY8);
        m.set_width(w);
        m.set_height(h);
        m.set_linesize(0, w);
        // SAFETY: the frame holds no buffer; `mem` covers the one plane of
        // a `w`x`h` GRAY8 image at stride `w`, and outlives the frame by the
        // caller's contract; nothing else writes it meanwhile.
        unsafe { m.set_data_ptr(0, mem.as_mut_ptr()) };
        f
    }

    #[test]
    fn set_data_ptr_lends_external_planes_that_clones_copy() {
        for (w, h) in [(1, 1), (3, 5), (64, 2), (17, 17)] {
            let mut mem = pattern((w * h) as usize);
            // SAFETY: `mem` is exactly the plane and outlives `f`.
            let f = unsafe { gray(&mut mem, w, h) };
            // Rust reads no plane it cannot bound.
            assert!(f.as_ref().plane(0).is_none());
            let copy = f.as_ref().try_clone().expect("C copies the plane");
            let line = copy.as_ref().linesizes()[0] as usize;
            let plane = copy.as_ref().plane(0).unwrap();
            for y in 0..h as usize {
                let row = &plane[y * line..y * line + w as usize];
                assert_eq!(row, &mem[y * w as usize..(y + 1) * w as usize]);
            }
            drop(f);
            drop(mem);
            // The copy holds its own buffers.
            assert!(copy.as_ref().buffer(0).unwrap().ref_count() == 1);
        }
    }

    #[test]
    fn set_data_ptr_takes_null_and_pointers_into_own_buffers() {
        let mut f = Frame::new().unwrap();
        let mut m = f.as_mut();
        m.set_buffer(0, Some(BufferReference::alloc_zeroed(32).unwrap()));
        let start = m.as_ref().buffer(0).unwrap().data().as_ptr();
        // SAFETY: NULL is always allowed.
        unsafe { m.set_data_ptr(7, ptr::null_mut()) };
        // SAFETY: inside the frame's buffer at index 0, 31 bytes in; the
        // pointer is only ever written through by C's own buffer rules,
        // which `plane_mut` follows.
        unsafe { m.set_data_ptr(1, start.wrapping_add(31).cast_mut()) };
        assert_eq!(f.as_ref().plane(1).unwrap(), [0]);
        assert!(f.as_ref().plane(7).is_none());
        f.as_mut().plane_mut(1).unwrap()[0] = 4;
        assert_eq!(f.as_ref().buffer(0).unwrap().data()[31], 4);
    }

    #[test]
    fn a_buffer_at_index_0_clears_external_planes() {
        let mut mem = pattern(16);
        // SAFETY: `mem` is exactly the plane and outlives `f`.
        let mut f = unsafe { gray(&mut mem, 4, 4) };
        f.as_mut()
            .set_buffer(0, Some(BufferReference::alloc_zeroed(16).unwrap()));
        assert!(f.as_ref().plane(0).is_none());
        // The external memory is no plane any more: a clone shares the
        // buffer without reading it.
        let copy = f.as_ref().try_clone().unwrap();
        drop(mem);
        assert!(copy.as_ref().plane(0).is_none());
        assert_eq!(copy.as_ref().buffer(0).unwrap().ref_count(), 2);
    }

    #[test]
    #[should_panic(expected = "plane 8 out of range")]
    fn set_data_ptr_panics_past_the_planes() {
        let mut f = Frame::new().unwrap();
        // SAFETY: NULL is always allowed; the index panics first.
        unsafe { f.as_mut().set_data_ptr(NUM_DATA_POINTERS, ptr::null_mut()) };
    }

    /// Planar audio of `channels` channels of `samples` U8 samples, each
    /// channel's plane one of `mems`.
    ///
    /// # Safety
    ///
    /// Every `mems` entry must hold `samples` bytes and outlive the frame.
    unsafe fn planar_audio(mems: &mut [Vec<u8>], samples: i32) -> FrameBox {
        let channels = mems.len();
        let mut f = Frame::new().unwrap();
        let mut m = f.as_mut();
        m.set_sample_format(SampleFormat::U8P);
        m.set_sample_count(samples);
        m.set_linesize(0, samples);
        m.set_channel_layout(ChannelLayout::unspecified(channels as i32))
            .unwrap();
        if channels > NUM_DATA_POINTERS {
            m.alloc_extended_data().unwrap();
        }
        for (i, mem) in mems.iter_mut().enumerate() {
            assert!(mem.len() >= samples as usize);
            // SAFETY: the frame holds no buffer; `mem` covers channel `i`'s
            // plane of `samples` U8 samples and outlives the frame by the
            // caller's contract.
            unsafe { m.set_extended_data_ptr(i, mem.as_mut_ptr()) };
        }
        f
    }

    #[test]
    fn set_extended_data_ptr_lends_channels_that_clones_copy() {
        for channels in [1usize, 2, 8, 9, 20] {
            for samples in [1, 33] {
                let mut mems: Vec<Vec<u8>> = (0..channels)
                    .map(|c| pattern(samples as usize + c)[c..].to_vec())
                    .collect();
                // SAFETY: each entry holds `samples` bytes and outlives `f`.
                let f = unsafe { planar_audio(&mut mems, samples) };
                assert!(f.as_ref().extended_plane(0).is_none());
                let copy = f.as_ref().try_clone().expect("C copies the planes");
                drop(f);
                let r = copy.as_ref();
                let expect = if channels > NUM_DATA_POINTERS {
                    channels
                } else {
                    NUM_DATA_POINTERS
                };
                assert_eq!(r.extended_plane_count(), expect);
                for (c, mem) in mems.iter().enumerate() {
                    let plane = r.extended_plane(c).unwrap();
                    assert_eq!(&plane[..samples as usize], &mem[..samples as usize]);
                }
            }
        }
    }

    #[test]
    #[should_panic(expected = "extended plane 3 out of range")]
    fn set_extended_data_ptr_panics_past_the_channels() {
        let mut f = Frame::new().unwrap();
        f.as_mut()
            .set_channel_layout(ChannelLayout::unspecified(3))
            .unwrap();
        f.as_mut().alloc_extended_data().unwrap();
        // SAFETY: NULL is always allowed; the index panics first.
        unsafe { f.as_mut().set_extended_data_ptr(3, ptr::null_mut()) };
    }

    /// Lends `mem` as `extended_data` entry `i`.
    ///
    /// # Safety
    ///
    /// As for `set_extended_data_ptr`: `mem` must hold the channel's
    /// samples and outlive the frame's use of the entry.
    unsafe fn lend_channel(f: &mut FrameBox, i: usize, mem: &mut [u8]) {
        // SAFETY: the caller's contract.
        unsafe { f.as_mut().set_extended_data_ptr(i, mem.as_mut_ptr()) };
    }

    /// Planar audio of more channels than `data` holds, all lent, whose
    /// `extended_data` is still `data`: C's copy would take the fields
    /// after `data` for channel 8's plane, so the clone is refused. With an
    /// array of its own lending every channel, the clone copies them all
    /// (review `ub_unsafe/clone-reads-planar-channels-past-data`).
    #[test]
    fn clones_refuse_channels_past_data() {
        const SAMPLES: usize = 64;
        let mut mems: Vec<Vec<u8>> = (0..NUM_DATA_POINTERS + 1)
            .map(|c| pattern(SAMPLES + c)[c..].to_vec())
            .collect();
        let mut f = Frame::new().unwrap();
        let mut m = f.as_mut();
        m.set_sample_format(SampleFormat::U8P);
        m.set_sample_count(SAMPLES as i32);
        // C's convention for planar audio, and channel 8's would-be address.
        m.set_linesize(0, SAMPLES as i32);
        m.set_channel_layout(ChannelLayout::unspecified(9)).unwrap();
        for (i, mem) in mems.iter_mut().take(NUM_DATA_POINTERS).enumerate() {
            // SAFETY: the frame holds no buffer; `mem` holds the channel's
            // SAMPLES U8 samples and outlives the frame.
            unsafe { m.set_data_ptr(i, mem.as_mut_ptr()) };
        }
        assert!(f.as_ref().try_clone().is_none());
        f.as_mut().alloc_extended_data().unwrap();
        for (i, mem) in mems.iter_mut().enumerate() {
            // SAFETY: as above, for entry `i` of the frame's own array.
            unsafe { lend_channel(&mut f, i, mem) };
        }
        let copy = f.as_ref().try_clone().expect("C copies nine channels");
        for (c, mem) in mems.iter().enumerate() {
            let plane = copy.as_ref().extended_plane(c).unwrap();
            assert_eq!(&plane[..SAMPLES], &mem[..SAMPLES]);
        }
        // Back to `data`: refused again, until the channels fit it.
        f.as_mut().reset_extended_data();
        assert!(f.as_ref().try_clone().is_none());
        f.as_mut()
            .set_channel_layout(ChannelLayout::unspecified(8))
            .unwrap();
        let copy = f.as_ref().try_clone().expect("C copies eight channels");
        assert_eq!(
            &copy.as_ref().plane(7).unwrap()[..SAMPLES],
            &mems[7][..SAMPLES]
        );
        drop(f);
    }

    /// A lent plane that is replaced is read no more:
    /// `alloc_extended_data` copies no lent plane, so the old block may go
    /// (review `ub_unsafe/extended-data-keeps-a-replaced-external-plane`).
    #[test]
    fn extended_arrays_keep_no_lent_planes() {
        const SAMPLES: usize = 64;
        let mut left = pattern(SAMPLES);
        let mut right = pattern(SAMPLES + 1)[1..].to_vec();
        let mut new_left = pattern(SAMPLES + 2)[2..].to_vec();
        let mut f = Frame::new().unwrap();
        let mut m = f.as_mut();
        m.set_sample_format(SampleFormat::U8P);
        m.set_sample_count(SAMPLES as i32);
        m.set_linesize(0, SAMPLES as i32);
        m.set_channel_layout(ChannelLayout::unspecified(2)).unwrap();
        // SAFETY: the frame holds no buffer; each block holds a channel's
        // SAMPLES U8 samples, `left` until its plane is replaced below, the
        // others for the frame's life.
        unsafe {
            m.set_data_ptr(0, left.as_mut_ptr());
            m.set_data_ptr(1, right.as_mut_ptr());
        }
        m.alloc_extended_data().unwrap();
        // SAFETY: as above.
        unsafe { m.set_data_ptr(0, new_left.as_mut_ptr()) };
        drop(left);
        // The array's entries are NULL: C refuses to copy.
        assert!(f.as_ref().try_clone().is_none());
        // SAFETY: as above, lent to the array itself.
        unsafe {
            lend_channel(&mut f, 0, &mut new_left);
            lend_channel(&mut f, 1, &mut right);
        }
        let copy = f.as_ref().try_clone().expect("C copies both channels");
        drop(f);
        let r = copy.as_ref();
        assert_eq!(&r.extended_plane(0).unwrap()[..SAMPLES], &new_left[..]);
        assert_eq!(&r.extended_plane(1).unwrap()[..SAMPLES], &right[..]);
    }

    #[test]
    fn hw_frames_context_round_trips_none() {
        let mut f = Frame::new().unwrap();
        // SAFETY: `None` is always allowed.
        let old = unsafe { f.as_mut().replace_hw_frames_context(None) };
        assert!(old.is_none() && f.as_ref().hw_frames_context().is_none());
        assert!(f.as_mut().take_hw_frames_context().is_none());
        // The frame and the empty seam cross threads.
        thread::spawn(move || {
            // SAFETY: as above.
            let old = unsafe { f.as_mut().replace_hw_frames_context(None) };
            assert!(old.is_none());
        })
        .join()
        .unwrap();
    }
}
