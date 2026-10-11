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
