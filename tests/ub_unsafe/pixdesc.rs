//! UB tests for `libavutil/pixdesc.h`'s unsafe API, each call within its
//! `# Safety` contract.

/// `PixelFormatDescriptor::from_raw`: adopting C's own table entries (empty
/// ones included), string literals, NULL, and exactly sized leaked heap
/// strings (so a read past the NUL is an ASan report), with every scalar at
/// its edges; adopted values outliving the raw struct they were copied from,
/// in `const` context, and read on several threads. Tests without `c_table`
/// in their name make no C call and also run under Miri.
mod pixel_format_descriptor_from_raw {
    use core::ffi::{CStr, c_char};
    use core::ptr;
    use std::ffi::CString;
    use std::sync::Mutex;
    use std::thread;

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::pixdesc::{PixelFormatDescriptor, PixelFormatFlags};

    /// `bytes` as an exactly sized heap C string that lives for the rest of
    /// the program, kept reachable from a static so that neither
    /// LeakSanitizer nor Miri reports it.
    fn leaked(bytes: &[u8]) -> *const c_char {
        static KEEP: Mutex<Vec<&'static CStr>> = Mutex::new(Vec::new());
        let s: &'static CStr = Box::leak(CString::new(bytes).unwrap().into_boxed_c_str());
        KEEP.lock().unwrap().push(s);
        s.as_ptr()
    }

    fn comp(v: i32) -> ffi::AVComponentDescriptor {
        ffi::AVComponentDescriptor {
            plane: v,
            step: v.wrapping_neg(),
            offset: i32::MIN.wrapping_add(v),
            shift: i32::MAX.wrapping_sub(v),
            depth: v,
        }
    }

    fn raw(name: *const c_char, alias: *const c_char, k: u8) -> ffi::AVPixFmtDescriptor {
        ffi::AVPixFmtDescriptor {
            name,
            nb_components: k,
            log2_chroma_w: k.wrapping_mul(3),
            log2_chroma_h: u8::MAX - k,
            flags: u64::MAX.checked_shr(u32::from(k)).unwrap_or(0),
            comp: [comp(i32::from(k)), comp(-1), comp(i32::MIN), comp(i32::MAX)],
            alias,
        }
    }

    /// Asserts every getter of `d` reads what `r` stores.
    fn assert_reads(d: &PixelFormatDescriptor, r: &ffi::AVPixFmtDescriptor) {
        let h = d.as_ref();
        let ptr_of = |s: Option<&CStr>| s.map_or(ptr::null(), CStr::as_ptr);
        assert_eq!(ptr_of(h.name()), r.name);
        assert_eq!(ptr_of(h.alias()), r.alias);
        assert_eq!(
            (h.component_count(), h.log2_chroma_w(), h.log2_chroma_h()),
            (r.nb_components, r.log2_chroma_w, r.log2_chroma_h)
        );
        assert_eq!(h.flags(), PixelFormatFlags::from_bits_retain(r.flags));
        for (c, rc) in h.components().iter().zip(&r.comp) {
            assert_eq!(
                [c.plane(), c.step(), c.offset(), c.shift(), c.depth()],
                [rc.plane, rc.step, rc.offset, rc.shift, rc.depth]
            );
        }
        let back = d.into_raw();
        assert_eq!(
            (back.name, back.alias, back.flags),
            (r.name, r.alias, r.flags)
        );
        assert!(!format!("{d:?}").is_empty());
    }

    #[test]
    fn literal_null_and_leaked_strings_with_edge_scalars() {
        let strings: [*const c_char; 7] = [
            ptr::null(),
            c"".as_ptr(),
            c"yuv420p".as_ptr(),
            leaked(b""),
            leaked(b"x"),
            leaked(b"\x80\xff"),
            leaked(&vec![b'z'; 8192]),
        ];
        for (k, &name) in strings.iter().enumerate() {
            for &alias in &strings {
                let r = raw(name, alias, k as u8 * 40);
                // SAFETY: `name` and `alias` are NULL, literals, or leaked
                // heap strings, none ever freed or written.
                let d = unsafe { PixelFormatDescriptor::from_raw(r) };
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
        fn adopt() -> PixelFormatDescriptor {
            let r = Box::new(raw(leaked(b"nv24"), c"nv24alias".as_ptr(), 2));
            // SAFETY: a leaked heap string and a literal, never freed or
            // written.
            unsafe { PixelFormatDescriptor::from_raw(*r) }
            // `r`'s box is freed here; the value holds its own copy.
        }
        let (names, copy) = {
            let d = adopt();
            ((d.as_ref().name(), d.as_ref().alias()), d.as_ref().get())
            // `d` goes out of scope; the strings are the program's.
        };
        assert_eq!(names.0.unwrap().to_bytes(), b"nv24");
        assert_eq!(names.1, Some(c"nv24alias"));
        assert_eq!(copy.as_ref().name(), names.0);
        // Adopted twice from one raw struct: two independent values.
        let r = raw(c"a".as_ptr(), ptr::null(), 1);
        // SAFETY: a literal and NULL.
        let (mut a, b) = unsafe {
            (
                PixelFormatDescriptor::from_raw(r),
                PixelFormatDescriptor::from_raw(r),
            )
        };
        a.as_mut().set_component_count(0);
        assert_eq!(b.as_ref().component_count(), 1);
        assert_ne!(a, b);
    }

    /// A descriptor adopted at compile time, as a `const`.
    const GRAY: PixelFormatDescriptor = {
        let zero = ffi::AVComponentDescriptor {
            plane: 0,
            step: 0,
            offset: 0,
            shift: 0,
            depth: 0,
        };
        let raw = ffi::AVPixFmtDescriptor {
            name: c"gray".as_ptr(),
            nb_components: 1,
            log2_chroma_w: 0,
            log2_chroma_h: 0,
            flags: 0,
            comp: [
                ffi::AVComponentDescriptor {
                    plane: 0,
                    step: 1,
                    offset: 0,
                    shift: 0,
                    depth: 8,
                },
                zero,
                zero,
                zero,
            ],
            alias: c"gray8,y8".as_ptr(),
        };
        // SAFETY: both strings are literals.
        unsafe { PixelFormatDescriptor::from_raw(raw) }
    };

    #[test]
    fn adopted_in_const_context() {
        let d = GRAY;
        assert_eq!(d.as_ref().name(), Some(c"gray"));
        assert_eq!(d.as_ref().alias(), Some(c"gray8,y8"));
        assert_eq!(d.as_ref().components().get(0).unwrap().depth(), 8);
    }

    #[test]
    fn adopted_values_cross_and_are_shared_between_threads() {
        let r = raw(leaked(b"p016be"), ptr::null(), 4);
        // SAFETY: a leaked heap string and NULL.
        let d = unsafe { PixelFormatDescriptor::from_raw(r) };
        let back = thread::spawn(move || {
            assert_eq!(d.as_ref().name().unwrap().to_bytes(), b"p016be");
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
                        assert_eq!(h.name().unwrap().to_bytes(), b"p016be");
                        assert_eq!(h.component_count(), 4);
                    }
                });
            }
        });
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn c_table_entries_adopt_and_read_back() {
        let nb = ffi::AVPixelFormat::AV_PIX_FMT_NB.0;
        let mut empty = 0;
        for fmt in 0..nb {
            // SAFETY: `av_pix_fmt_desc_get` takes any value and returns NULL
            // or a pointer into C's static table.
            let p = unsafe { ffi::av_pix_fmt_desc_get(ffi::AVPixelFormat(fmt)) };
            assert!(!p.is_null());
            // SAFETY: `p` points to a live descriptor in C's static,
            // never-written table.
            let r = unsafe { p.read() };
            // SAFETY: C's table entries' strings are NULL or literals.
            let d = unsafe { PixelFormatDescriptor::from_raw(r) };
            assert_reads(&d, &r);
            if let Some(name) = d.as_ref().name() {
                assert!(!name.is_empty());
            } else {
                empty += 1;
            }
        }
        // Out-of-range formats have no entry to adopt.
        // SAFETY: as above.
        assert!(unsafe { ffi::av_pix_fmt_desc_get(ffi::AVPixelFormat(nb)) }.is_null());
        assert!(empty < nb as usize);
    }
}
