//! Equivalence of `libavutil/pixfmt.h`'s safe types with the C enums, as C
//! observes them.

/// [`ColorSpace`] and [`ColorTransfer`] against `AVColorSpace` and
/// `AVColorTransferCharacteristic` (formats wave 0, batch 3): through
/// `av_color_space_name` / `av_color_transfer_name` and their `_from_name`
/// inverses, and through C-allocated storage holding the values.
mod color_space_and_transfer {
    use core::ffi::{CStr, c_int, c_uint, c_void};
    use core::mem::size_of;

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::mem::{HeapVec, alloc_array_zeroed};
    use ffmpeg::libavutil::pixfmt::{ColorSpace, ColorTransfer};

    type Spc = ffi::AVColorSpace;
    type Trc = ffi::AVColorTransferCharacteristic;

    /// Every colour space constant beside its C enumerator and C name.
    const SPACE_NAMED: [(ColorSpace, Spc, &str); 19] = [
        (ColorSpace::RGB, Spc::AVCOL_SPC_RGB, "gbr"),
        (ColorSpace::BT709, Spc::AVCOL_SPC_BT709, "bt709"),
        (
            ColorSpace::UNSPECIFIED,
            Spc::AVCOL_SPC_UNSPECIFIED,
            "unknown",
        ),
        (ColorSpace::RESERVED, Spc::AVCOL_SPC_RESERVED, "reserved"),
        (ColorSpace::FCC, Spc::AVCOL_SPC_FCC, "fcc"),
        (ColorSpace::BT470BG, Spc::AVCOL_SPC_BT470BG, "bt470bg"),
        (ColorSpace::SMPTE170M, Spc::AVCOL_SPC_SMPTE170M, "smpte170m"),
        (ColorSpace::SMPTE240M, Spc::AVCOL_SPC_SMPTE240M, "smpte240m"),
        (ColorSpace::YCGCO, Spc::AVCOL_SPC_YCGCO, "ycgco"),
        (ColorSpace::YCOCG, Spc::AVCOL_SPC_YCOCG, "ycgco"),
        (
            ColorSpace::BT2020_NCL,
            Spc::AVCOL_SPC_BT2020_NCL,
            "bt2020nc",
        ),
        (ColorSpace::BT2020_CL, Spc::AVCOL_SPC_BT2020_CL, "bt2020c"),
        (ColorSpace::SMPTE2085, Spc::AVCOL_SPC_SMPTE2085, "smpte2085"),
        (
            ColorSpace::CHROMA_DERIVED_NCL,
            Spc::AVCOL_SPC_CHROMA_DERIVED_NCL,
            "chroma-derived-nc",
        ),
        (
            ColorSpace::CHROMA_DERIVED_CL,
            Spc::AVCOL_SPC_CHROMA_DERIVED_CL,
            "chroma-derived-c",
        ),
        (ColorSpace::ICTCP, Spc::AVCOL_SPC_ICTCP, "ictcp"),
        (ColorSpace::IPT_C2, Spc::AVCOL_SPC_IPT_C2, "ipt-c2"),
        (ColorSpace::YCGCO_RE, Spc::AVCOL_SPC_YCGCO_RE, "ycgco-re"),
        (ColorSpace::YCGCO_RO, Spc::AVCOL_SPC_YCGCO_RO, "ycgco-ro"),
    ];

    /// Every transfer characteristic constant beside its C enumerator and C
    /// name; C gives both reserved values the name `reserved`.
    const TRANSFER_NAMED: [(ColorTransfer, Trc, &str); 23] = [
        (
            ColorTransfer::RESERVED0,
            Trc::AVCOL_TRC_RESERVED0,
            "reserved",
        ),
        (ColorTransfer::BT709, Trc::AVCOL_TRC_BT709, "bt709"),
        (
            ColorTransfer::UNSPECIFIED,
            Trc::AVCOL_TRC_UNSPECIFIED,
            "unknown",
        ),
        (ColorTransfer::RESERVED, Trc::AVCOL_TRC_RESERVED, "reserved"),
        (ColorTransfer::GAMMA22, Trc::AVCOL_TRC_GAMMA22, "bt470m"),
        (ColorTransfer::GAMMA28, Trc::AVCOL_TRC_GAMMA28, "bt470bg"),
        (
            ColorTransfer::SMPTE170M,
            Trc::AVCOL_TRC_SMPTE170M,
            "smpte170m",
        ),
        (
            ColorTransfer::SMPTE240M,
            Trc::AVCOL_TRC_SMPTE240M,
            "smpte240m",
        ),
        (ColorTransfer::LINEAR, Trc::AVCOL_TRC_LINEAR, "linear"),
        (ColorTransfer::LOG, Trc::AVCOL_TRC_LOG, "log100"),
        (ColorTransfer::LOG_SQRT, Trc::AVCOL_TRC_LOG_SQRT, "log316"),
        (
            ColorTransfer::IEC61966_2_4,
            Trc::AVCOL_TRC_IEC61966_2_4,
            "iec61966-2-4",
        ),
        (
            ColorTransfer::BT1361_ECG,
            Trc::AVCOL_TRC_BT1361_ECG,
            "bt1361e",
        ),
        (
            ColorTransfer::IEC61966_2_1,
            Trc::AVCOL_TRC_IEC61966_2_1,
            "iec61966-2-1",
        ),
        (
            ColorTransfer::BT2020_10,
            Trc::AVCOL_TRC_BT2020_10,
            "bt2020-10",
        ),
        (
            ColorTransfer::BT2020_12,
            Trc::AVCOL_TRC_BT2020_12,
            "bt2020-12",
        ),
        (
            ColorTransfer::SMPTE2084,
            Trc::AVCOL_TRC_SMPTE2084,
            "smpte2084",
        ),
        (
            ColorTransfer::SMPTEST2084,
            Trc::AVCOL_TRC_SMPTEST2084,
            "smpte2084",
        ),
        (ColorTransfer::SMPTE428, Trc::AVCOL_TRC_SMPTE428, "smpte428"),
        (
            ColorTransfer::SMPTEST428_1,
            Trc::AVCOL_TRC_SMPTEST428_1,
            "smpte428",
        ),
        (
            ColorTransfer::ARIB_STD_B67,
            Trc::AVCOL_TRC_ARIB_STD_B67,
            "arib-std-b67",
        ),
        (ColorTransfer::EXT_BASE, Trc::AVCOL_TRC_EXT_BASE, "vlog"),
        (ColorTransfer::V_LOG, Trc::AVCOL_TRC_V_LOG, "vlog"),
    ];

    /// Values C may store though it names none of them: the `_NB` markers,
    /// the gap before the extensions, and negative `int`s as their unsigned
    /// encoding.
    const SPACE_UNNAMED: [c_uint; 7] = [18, 19, 255, 256, 1 << 31, c_uint::MAX - 1, c_uint::MAX];
    const TRANSFER_UNNAMED: [c_uint; 8] =
        [19, 20, 255, 257, 258, 1 << 31, c_uint::MAX - 1, c_uint::MAX];

    /// `name` from a C function returning a static string or NULL.
    fn c_str(name: *const core::ffi::c_char) -> Option<&'static str> {
        if name.is_null() {
            return None;
        }
        // SAFETY: a non-NULL result is one of the static string literals in
        // `pixdesc.c`'s name tables.
        Some(unsafe { CStr::from_ptr(name) }.to_str().unwrap())
    }

    /// What C calls `c`: `av_color_space_name`, NULL as `None`.
    fn c_space_name(c: Spc) -> Option<&'static str> {
        // SAFETY: `av_color_space_name` takes the enum by value and accepts
        // any value, returning NULL for those past its table.
        c_str(unsafe { ffi::av_color_space_name(c) })
    }

    /// What C calls `t`: `av_color_transfer_name`, NULL as `None`.
    fn c_transfer_name(t: Trc) -> Option<&'static str> {
        // SAFETY: as for `av_color_space_name`, over both of its tables.
        c_str(unsafe { ffi::av_color_transfer_name(t) })
    }

    /// The value C parses `name` as, or its negative error.
    fn c_space_from_name(name: &str) -> c_int {
        let name = std::ffi::CString::new(name).unwrap();
        // SAFETY: `name` is a NUL-terminated string, live for the call.
        unsafe { ffi::av_color_space_from_name(name.as_ptr()) }
    }

    /// The value C parses `name` as, or its negative error.
    fn c_transfer_from_name(name: &str) -> c_int {
        let name = std::ffi::CString::new(name).unwrap();
        // SAFETY: `name` is a NUL-terminated string, live for the call.
        unsafe { ffi::av_color_transfer_from_name(name.as_ptr()) }
    }

    /// Every colour space the tests compare, named first.
    fn space_values() -> Vec<Spc> {
        SPACE_NAMED
            .iter()
            .map(|&(_, c, _)| c)
            .chain(SPACE_UNNAMED.map(ffi::AVColorSpace))
            .collect()
    }

    /// Every transfer characteristic the tests compare, named first.
    fn transfer_values() -> Vec<Trc> {
        TRANSFER_NAMED
            .iter()
            .map(|&(_, c, _)| c)
            .chain(TRANSFER_UNNAMED.map(ffi::AVColorTransferCharacteristic))
            .collect()
    }

    /// A C block of `len` zeroed `T`, freed with `av_free` on drop.
    struct CBlock<T: Copy>(*mut T, usize);

    impl<T: Copy> CBlock<T> {
        fn zeroed(len: usize) -> Self {
            // SAFETY: `av_calloc` takes its sizes by value.
            let ptr = unsafe { ffi::av_calloc(len, size_of::<T>()) };
            assert!(!ptr.is_null());
            Self(ptr.cast(), len)
        }

        fn get(&self, i: usize) -> T {
            assert!(i < self.1);
            // SAFETY: `i` is in bounds of the live block, initialised by
            // `av_calloc` (all zero, a valid C enum value) or by `set`.
            unsafe { self.0.add(i).read() }
        }

        fn set(&mut self, i: usize, v: T) {
            assert!(i < self.1);
            // SAFETY: `i` is in bounds of the live block, which we own.
            unsafe { self.0.add(i).write(v) }
        }

        fn values(&self) -> Vec<T> {
            (0..self.1).map(|i| self.get(i)).collect()
        }
    }

    impl<T: Copy> Drop for CBlock<T> {
        fn drop(&mut self) {
            // SAFETY: `self.0` is a live `av_calloc` block we own.
            unsafe { ffi::av_free(self.0.cast::<c_void>()) }
        }
    }

    /// The C values a Rust array of `R` holds, read back through its C-typed
    /// pointer.
    fn rust_as_c<R: Copy, C: Copy>(a: &HeapVec<R>) -> Vec<C> {
        assert_eq!(size_of::<R>(), size_of::<C>());
        let ptr = a.as_ptr().cast::<C>();
        (0..a.len())
            // SAFETY: the callers' `R` is `#[repr(transparent)]` over `C`, and
            // `i` is in bounds of the live, initialised array `a` borrows.
            .map(|i| unsafe { ptr.add(i).read() })
            .collect()
    }

    #[test]
    fn color_space_constants_are_the_c_enumerators() {
        for (c, raw, name) in SPACE_NAMED {
            assert_eq!(c.into_raw(), raw);
            assert_eq!(ColorSpace::from_raw(raw), c);
            assert_eq!(c_space_name(c.into_raw()), Some(name));
            // C's parser maps the name back to the primary constant.
            assert_eq!(c_space_from_name(name), c.into_raw().0 as c_int, "{name}");
        }
        for raw in SPACE_UNNAMED {
            let c = ColorSpace::from_raw(ffi::AVColorSpace(raw));
            assert!(SPACE_NAMED.iter().all(|&(n, _, _)| n != c));
            assert_eq!(c_space_name(c.into_raw()), None, "raw {raw}");
            assert_eq!(c.into_raw().0, raw);
        }
        assert_eq!(c_space_name(ColorSpace::default().into()), Some("unknown"));
        // A name C does not know parses as no value at all.
        assert_eq!(c_space_from_name("no-such-space"), -(ffi::EINVAL as c_int));
    }

    #[test]
    fn color_transfer_constants_are_the_c_enumerators() {
        for (t, raw, name) in TRANSFER_NAMED {
            assert_eq!(t.into_raw(), raw);
            assert_eq!(ColorTransfer::from_raw(raw), t);
            assert_eq!(c_transfer_name(t.into_raw()), Some(name));
            // C's parser takes the first value of a shared name: both
            // reserved values parse as `RESERVED0`.
            let back = if t == ColorTransfer::RESERVED {
                ColorTransfer::RESERVED0
            } else {
                t
            };
            assert_eq!(
                c_transfer_from_name(name),
                back.into_raw().0 as c_int,
                "{name}"
            );
        }
        for raw in TRANSFER_UNNAMED {
            let t = ColorTransfer::from_raw(ffi::AVColorTransferCharacteristic(raw));
            assert!(TRANSFER_NAMED.iter().all(|&(n, _, _)| n != t));
            assert_eq!(c_transfer_name(t.into_raw()), None, "raw {raw}");
            assert_eq!(t.into_raw().0, raw);
        }
        assert_eq!(
            c_transfer_name(ColorTransfer::default().into()),
            Some("unknown")
        );
        assert_eq!(
            c_transfer_from_name("no-such-transfer"),
            -(ffi::EINVAL as c_int)
        );
    }

    #[test]
    fn debug_names_exactly_what_c_names() {
        // Rust names a value with a constant exactly when C has a name for it.
        for raw in space_values()
            .into_iter()
            .chain((0..=300).map(ffi::AVColorSpace))
        {
            let debug = format!("{:?}", ColorSpace::from_raw(raw));
            assert_eq!(
                !debug.starts_with("ColorSpace("),
                c_space_name(raw).is_some(),
                "{raw:?}"
            );
        }
        for raw in transfer_values()
            .into_iter()
            .chain((0..=300).map(ffi::AVColorTransferCharacteristic))
        {
            let debug = format!("{:?}", ColorTransfer::from_raw(raw));
            assert_eq!(
                !debug.starts_with("ColorTransfer("),
                c_transfer_name(raw).is_some(),
                "{raw:?}"
            );
        }
    }

    #[test]
    fn zeroed_storage_reads_as_c_zeroed_storage() {
        const LEN: usize = 8;
        let spaces = alloc_array_zeroed::<ColorSpace>(LEN).unwrap();
        let c_spaces = CBlock::<Spc>::zeroed(LEN);
        assert_eq!(rust_as_c::<_, Spc>(&spaces), c_spaces.values());
        // A zero-filled C struct's colour space is `AVCOL_SPC_RGB`.
        assert!(spaces.as_slice().iter().all(|&c| c == ColorSpace::RGB));

        let transfers = alloc_array_zeroed::<ColorTransfer>(LEN).unwrap();
        let c_transfers = CBlock::<Trc>::zeroed(LEN);
        assert_eq!(rust_as_c::<_, Trc>(&transfers), c_transfers.values());
        // ... and its transfer characteristic `AVCOL_TRC_RESERVED0`.
        assert!(
            transfers
                .as_slice()
                .iter()
                .all(|&t| t == ColorTransfer::RESERVED0)
        );
    }

    #[test]
    fn color_space_writes_through_each_side_agree_after_every_step() {
        let values = space_values();
        let len = values.len();
        let mut rust = alloc_array_zeroed::<ColorSpace>(len).unwrap();
        let mut c = CBlock::<Spc>::zeroed(len);
        for (step, &v) in values.iter().enumerate() {
            // Each step writes slot `step` and rotates an earlier one.
            let back = (step * 7) % len;
            let w = values[(step + 3) % len];
            rust.as_mut_slice()[step] = ColorSpace::from_raw(v);
            rust.as_mut_slice()[back] = w.into();
            c.set(step, v);
            c.set(back, w);
            assert_eq!(rust_as_c::<_, Spc>(&rust), c.values(), "step {step}");
            for i in 0..len {
                let r = rust.as_slice()[i].into_raw();
                assert_eq!(
                    c_space_name(r),
                    c_space_name(c.get(i)),
                    "step {step} slot {i}"
                );
            }
        }
        // A clone is `av_memdup`'s copy of the same encodings.
        let copy = rust.clone();
        assert_eq!(rust_as_c::<_, Spc>(&copy), c.values());
    }

    #[test]
    fn color_transfer_writes_through_each_side_agree_after_every_step() {
        let values = transfer_values();
        let len = values.len();
        let mut rust = alloc_array_zeroed::<ColorTransfer>(len).unwrap();
        let mut c = CBlock::<Trc>::zeroed(len);
        for (step, &v) in values.iter().enumerate() {
            let back = (step * 5) % len;
            let w = values[(step + 3) % len];
            rust.as_mut_slice()[step] = ColorTransfer::from_raw(v);
            rust.as_mut_slice()[back] = w.into();
            c.set(step, v);
            c.set(back, w);
            assert_eq!(rust_as_c::<_, Trc>(&rust), c.values(), "step {step}");
            for i in 0..len {
                let r = rust.as_slice()[i].into_raw();
                assert_eq!(
                    c_transfer_name(r),
                    c_transfer_name(c.get(i)),
                    "step {step} slot {i}"
                );
            }
        }
        let copy = rust.clone();
        assert_eq!(rust_as_c::<_, Trc>(&copy), c.values());
    }

    #[test]
    fn values_c_writes_are_read_back_unchanged() {
        // The path a field accessor over a C array of these enums takes: adopt
        // the C block and read it as the safe type.
        let spaces = space_values();
        let mut c = CBlock::<Spc>::zeroed(spaces.len());
        for (i, &v) in spaces.iter().enumerate() {
            c.set(i, v);
        }
        let (ptr, len) = (c.0, c.1);
        core::mem::forget(c);
        // SAFETY: `ptr` is a live `av_calloc` block of `len` initialised
        // `AVColorSpace`, whose ownership `forget` gave up; `ColorSpace` is
        // layout-compatible with it, and `av_free` releases it.
        let adopted = unsafe { HeapVec::<ColorSpace>::from_raw_parts(ptr.cast(), len) }.unwrap();
        for (i, &v) in spaces.iter().enumerate() {
            assert_eq!(adopted.as_slice()[i], ColorSpace::from_raw(v));
        }

        let transfers = transfer_values();
        let mut c = CBlock::<Trc>::zeroed(transfers.len());
        for (i, &v) in transfers.iter().enumerate() {
            c.set(i, v);
        }
        let (ptr, len) = (c.0, c.1);
        core::mem::forget(c);
        // SAFETY: as above, for `AVColorTransferCharacteristic` and
        // `ColorTransfer`.
        let adopted = unsafe { HeapVec::<ColorTransfer>::from_raw_parts(ptr.cast(), len) }.unwrap();
        for (i, &v) in transfers.iter().enumerate() {
            assert_eq!(adopted.as_slice()[i], ColorTransfer::from_raw(v));
        }
    }
}

/// [`AlphaMode`] against `AVAlphaMode` (formats wave 0, batch 0): through
/// `av_alpha_mode_name` and `av_alpha_mode_from_name`, and through
/// C-allocated storage holding the values.
mod alpha_mode {
    use core::ffi::{CStr, c_uint, c_void};
    use core::mem::size_of;

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::mem::{HeapVec, alloc_array_zeroed};
    use ffmpeg::libavutil::pixfmt::AlphaMode;

    /// Every named alpha mode beside its C enumerator and C name.
    const ALPHA_MODE_NAMED: [(AlphaMode, ffi::AVAlphaMode, &CStr); 3] = [
        (
            AlphaMode::UNSPECIFIED,
            ffi::AVAlphaMode::AVALPHA_MODE_UNSPECIFIED,
            c"unspecified",
        ),
        (
            AlphaMode::PREMULTIPLIED,
            ffi::AVAlphaMode::AVALPHA_MODE_PREMULTIPLIED,
            c"premultiplied",
        ),
        (
            AlphaMode::STRAIGHT,
            ffi::AVAlphaMode::AVALPHA_MODE_STRAIGHT,
            c"straight",
        ),
    ];

    /// Values C may store though it names none of them: `AVALPHA_MODE_NB`,
    /// `av_alpha_mode_from_name`'s `AVERROR(EINVAL)` and the extremes.
    const ALPHA_MODE_UNNAMED: [c_uint; 6] = [3, 4, 255, (-22i32) as c_uint, 1 << 31, c_uint::MAX];

    /// What C calls `mode`: `av_alpha_mode_name`, NULL as `None`.
    fn c_name(mode: ffi::AVAlphaMode) -> Option<&'static CStr> {
        // SAFETY: `av_alpha_mode_name` takes the enum by value and range-checks
        // it before indexing.
        let name = unsafe { ffi::av_alpha_mode_name(mode) };
        // SAFETY: a non-NULL result is one of the static string literals in
        // `pixdesc.c`.
        (!name.is_null()).then(|| unsafe { CStr::from_ptr(name) })
    }

    /// The mode C parses `name` as: `av_alpha_mode_from_name`.
    fn c_from_name(name: &CStr) -> ffi::AVAlphaMode {
        // SAFETY: `name` is a NUL-terminated string `av_alpha_mode_from_name`
        // only reads.
        unsafe { ffi::av_alpha_mode_from_name(name.as_ptr()) }
    }

    /// Every alpha mode the tests compare, named first.
    fn values() -> Vec<ffi::AVAlphaMode> {
        ALPHA_MODE_NAMED
            .iter()
            .map(|&(_, c, _)| c)
            .chain(ALPHA_MODE_UNNAMED.map(ffi::AVAlphaMode))
            .collect()
    }

    /// A C block of `ffi::AVAlphaMode`, freed with `av_free` on drop.
    struct CModes(*mut ffi::AVAlphaMode, usize);

    impl CModes {
        fn zeroed(len: usize) -> Self {
            // SAFETY: `av_calloc` takes its sizes by value.
            let ptr = unsafe { ffi::av_calloc(len, size_of::<ffi::AVAlphaMode>()) };
            assert!(!ptr.is_null());
            Self(ptr.cast(), len)
        }

        fn get(&self, i: usize) -> ffi::AVAlphaMode {
            assert!(i < self.1);
            // SAFETY: `i` is in bounds of the live, initialised block.
            unsafe { self.0.add(i).read() }
        }

        fn set(&mut self, i: usize, mode: ffi::AVAlphaMode) {
            assert!(i < self.1);
            // SAFETY: `i` is in bounds of the live block, which we own.
            unsafe { self.0.add(i).write(mode) }
        }
    }

    impl Drop for CModes {
        fn drop(&mut self) {
            // SAFETY: `self.0` is a live `av_calloc` block we own.
            unsafe { ffi::av_free(self.0.cast::<c_void>()) }
        }
    }

    #[test]
    fn alpha_mode_constants_are_the_c_enumerators() {
        for (mode, c, name) in ALPHA_MODE_NAMED {
            assert_eq!(mode.into_raw(), c);
            assert_eq!(AlphaMode::from_raw(c), mode);
            assert_eq!(c_name(mode.into_raw()), Some(name));
            assert_eq!(AlphaMode::from_raw(c_from_name(name)), mode);
        }
        for raw in ALPHA_MODE_UNNAMED {
            let mode = AlphaMode::from_raw(ffi::AVAlphaMode(raw));
            assert!(ALPHA_MODE_NAMED.iter().all(|&(n, _, _)| n != mode));
            assert_eq!(c_name(mode.into_raw()), None, "raw {raw}");
            assert_eq!(format!("{mode:?}"), format!("AlphaMode({raw})"));
        }
    }

    #[test]
    fn alpha_mode_holds_c_parse_errors_unchanged() {
        for name in [c"", c"Straight", c"premultiplied ", c"none", c"unknown"] {
            let c = c_from_name(name);
            let mode = AlphaMode::from(c);
            assert_eq!(mode.into_raw(), c);
            assert_eq!(c.0, (-22i32) as c_uint, "{name:?}");
            assert!(ALPHA_MODE_NAMED.iter().all(|&(n, _, _)| n != mode));
            assert_eq!(c_name(mode.into_raw()), None);
        }
    }

    #[test]
    fn alpha_mode_zeroed_storage_reads_as_c_zeroed_storage() {
        const LEN: usize = 8;
        let rust = alloc_array_zeroed::<AlphaMode>(LEN).unwrap();
        let c = CModes::zeroed(LEN);
        for (i, mode) in rust.as_slice().iter().enumerate() {
            assert_eq!(mode.into_raw(), c.get(i));
            assert_eq!(c_name(mode.into_raw()), c_name(c.get(i)));
        }
        // A zero-filled C frame's alpha mode is `AVALPHA_MODE_UNSPECIFIED`.
        assert!(rust.as_slice().iter().all(|&m| m == AlphaMode::default()));
    }

    #[test]
    fn alpha_mode_writes_through_each_side_agree_after_every_step() {
        let values = values();
        let len = values.len();
        let mut rust = alloc_array_zeroed::<AlphaMode>(len).unwrap();
        let mut c = CModes::zeroed(len);
        for (step, &v) in values.iter().enumerate() {
            // Each step writes slot `step` and rotates an earlier one.
            let back = (step * 5) % len;
            let w = values[(step + 4) % len];
            rust.as_mut_slice()[step] = AlphaMode::from_raw(v);
            rust.as_mut_slice()[back] = w.into();
            c.set(step, v);
            c.set(back, w);
            for i in 0..len {
                let ri = rust.as_slice()[i];
                assert_eq!(ri.into_raw(), c.get(i), "step {step} slot {i}");
                assert_eq!(c_name(ri.into_raw()), c_name(c.get(i)));
            }
        }
        // A clone is `av_memdup`'s copy of the same encodings.
        let copy = rust.clone();
        drop(rust);
        for i in 0..len {
            assert_eq!(copy.as_slice()[i].into_raw(), c.get(i));
        }
    }

    #[test]
    fn alpha_mode_values_c_writes_are_read_back_unchanged() {
        let values = values();
        let mut c = CModes::zeroed(values.len());
        for (i, &v) in values.iter().enumerate() {
            c.set(i, v);
        }
        // The path a field accessor over C-written storage takes: adopt the C
        // block and read it as `AlphaMode`s.
        let (ptr, len) = (c.0, c.1);
        core::mem::forget(c);
        // SAFETY: `ptr` is a live `av_calloc` block of `len` initialised
        // `ffi::AVAlphaMode`, whose ownership `forget` gave up; `AlphaMode` is
        // layout-compatible with it, and `av_free` releases it.
        let adopted = unsafe { HeapVec::<AlphaMode>::from_raw_parts(ptr.cast(), len) }.unwrap();
        for (i, &v) in values.iter().enumerate() {
            assert_eq!(adopted.as_slice()[i], AlphaMode::from_raw(v));
            assert_eq!(c_name(adopted.as_slice()[i].into_raw()), c_name(v));
        }
    }
}

/// [`PixelFormat`] against `AVPixelFormat` (formats wave 0, batch 5): through
/// `av_get_pix_fmt_name`, whose descriptor table names every enumerator,
/// through the native-endian `AV_PIX_FMT_NE` aliases the
/// `crustify_AV_PIX_FMT_NE` shim evaluates, and through C-allocated storage
/// holding the values.
mod pixel_format {
    use core::ffi::{CStr, c_char, c_int, c_void};
    use core::mem::size_of;
    use core::ptr;

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::mem::{HeapVec, alloc_array_zeroed};
    use ffmpeg::libavutil::pixfmt::PixelFormat;

    /// The constant's name for every C alias spelled `AV_PIX_FMT_NE(be, le)`.
    const NATIVE_ENDIAN: [(&str, PixelFormat); 104] = [
        ("RGB32", PixelFormat::RGB32),
        ("RGB32_1", PixelFormat::RGB32_1),
        ("BGR32", PixelFormat::BGR32),
        ("BGR32_1", PixelFormat::BGR32_1),
        ("0RGB32", PixelFormat::XRGB32),
        ("0BGR32", PixelFormat::XBGR32),
        ("GRAY9", PixelFormat::GRAY9),
        ("GRAY10", PixelFormat::GRAY10),
        ("GRAY12", PixelFormat::GRAY12),
        ("GRAY14", PixelFormat::GRAY14),
        ("GRAY16", PixelFormat::GRAY16),
        ("GRAY32", PixelFormat::GRAY32),
        ("YA16", PixelFormat::YA16),
        ("RGB48", PixelFormat::RGB48),
        ("RGB565", PixelFormat::RGB565),
        ("RGB555", PixelFormat::RGB555),
        ("RGB444", PixelFormat::RGB444),
        ("RGBA64", PixelFormat::RGBA64),
        ("BGR48", PixelFormat::BGR48),
        ("BGR565", PixelFormat::BGR565),
        ("BGR555", PixelFormat::BGR555),
        ("BGR444", PixelFormat::BGR444),
        ("BGRA64", PixelFormat::BGRA64),
        ("YUV420P9", PixelFormat::YUV420P9),
        ("YUV422P9", PixelFormat::YUV422P9),
        ("YUV444P9", PixelFormat::YUV444P9),
        ("YUV420P10", PixelFormat::YUV420P10),
        ("YUV422P10", PixelFormat::YUV422P10),
        ("YUV440P10", PixelFormat::YUV440P10),
        ("YUV444P10", PixelFormat::YUV444P10),
        ("YUV420P12", PixelFormat::YUV420P12),
        ("YUV422P12", PixelFormat::YUV422P12),
        ("YUV440P12", PixelFormat::YUV440P12),
        ("YUV444P12", PixelFormat::YUV444P12),
        ("YUV420P14", PixelFormat::YUV420P14),
        ("YUV422P14", PixelFormat::YUV422P14),
        ("YUV444P14", PixelFormat::YUV444P14),
        ("YUV420P16", PixelFormat::YUV420P16),
        ("YUV422P16", PixelFormat::YUV422P16),
        ("YUV444P16", PixelFormat::YUV444P16),
        ("YUV444P10MSB", PixelFormat::YUV444P10MSB),
        ("YUV444P12MSB", PixelFormat::YUV444P12MSB),
        ("GBRP9", PixelFormat::GBRP9),
        ("GBRP10", PixelFormat::GBRP10),
        ("GBRP12", PixelFormat::GBRP12),
        ("GBRP14", PixelFormat::GBRP14),
        ("GBRP16", PixelFormat::GBRP16),
        ("GBRAP10", PixelFormat::GBRAP10),
        ("GBRAP12", PixelFormat::GBRAP12),
        ("GBRAP14", PixelFormat::GBRAP14),
        ("GBRAP16", PixelFormat::GBRAP16),
        ("GBRAP32", PixelFormat::GBRAP32),
        ("GBRP10MSB", PixelFormat::GBRP10MSB),
        ("GBRP12MSB", PixelFormat::GBRP12MSB),
        ("BAYER_BGGR16", PixelFormat::BAYER_BGGR16),
        ("BAYER_RGGB16", PixelFormat::BAYER_RGGB16),
        ("BAYER_GBRG16", PixelFormat::BAYER_GBRG16),
        ("BAYER_GRBG16", PixelFormat::BAYER_GRBG16),
        ("GBRPF16", PixelFormat::GBRPF16),
        ("GBRAPF16", PixelFormat::GBRAPF16),
        ("GBRPF32", PixelFormat::GBRPF32),
        ("GBRAPF32", PixelFormat::GBRAPF32),
        ("GRAYF16", PixelFormat::GRAYF16),
        ("GRAYF32", PixelFormat::GRAYF32),
        ("YAF16", PixelFormat::YAF16),
        ("YAF32", PixelFormat::YAF32),
        ("YUVA420P9", PixelFormat::YUVA420P9),
        ("YUVA422P9", PixelFormat::YUVA422P9),
        ("YUVA444P9", PixelFormat::YUVA444P9),
        ("YUVA420P10", PixelFormat::YUVA420P10),
        ("YUVA422P10", PixelFormat::YUVA422P10),
        ("YUVA444P10", PixelFormat::YUVA444P10),
        ("YUVA422P12", PixelFormat::YUVA422P12),
        ("YUVA444P12", PixelFormat::YUVA444P12),
        ("YUVA420P16", PixelFormat::YUVA420P16),
        ("YUVA422P16", PixelFormat::YUVA422P16),
        ("YUVA444P16", PixelFormat::YUVA444P16),
        ("XYZ12", PixelFormat::XYZ12),
        ("NV20", PixelFormat::NV20),
        ("AYUV64", PixelFormat::AYUV64),
        ("P010", PixelFormat::P010),
        ("P012", PixelFormat::P012),
        ("P016", PixelFormat::P016),
        ("Y210", PixelFormat::Y210),
        ("Y212", PixelFormat::Y212),
        ("Y216", PixelFormat::Y216),
        ("XV30", PixelFormat::XV30),
        ("XV36", PixelFormat::XV36),
        ("XV48", PixelFormat::XV48),
        ("V30X", PixelFormat::V30X),
        ("X2RGB10", PixelFormat::X2RGB10),
        ("X2BGR10", PixelFormat::X2BGR10),
        ("P210", PixelFormat::P210),
        ("P410", PixelFormat::P410),
        ("P212", PixelFormat::P212),
        ("P412", PixelFormat::P412),
        ("P216", PixelFormat::P216),
        ("P416", PixelFormat::P416),
        ("RGBF16", PixelFormat::RGBF16),
        ("RGBAF16", PixelFormat::RGBAF16),
        ("RGBF32", PixelFormat::RGBF32),
        ("RGBAF32", PixelFormat::RGBAF32),
        ("RGB96", PixelFormat::RGB96),
        ("RGBA128", PixelFormat::RGBA128),
    ];

    /// The formats whose C descriptor name is not their constant's name in
    /// lower case.
    const RENAMED: [(PixelFormat, &str); 7] = [
        (PixelFormat::GRAY8, "gray"),
        (PixelFormat::MONOWHITE, "monow"),
        (PixelFormat::MONOBLACK, "monob"),
        (PixelFormat::XRGB, "0rgb"),
        (PixelFormat::XBGR, "0bgr"),
        (PixelFormat::VIDEOTOOLBOX, "videotoolbox_vld"),
        (PixelFormat::AMF_SURFACE, "amf"),
    ];

    /// Values C may store though it names none of them, `AV_PIX_FMT_NB` included.
    fn unnamed() -> [c_int; 8] {
        let nb = ffi::AVPixelFormat::AV_PIX_FMT_NB.0;
        [
            c_int::MIN,
            -1000,
            -2,
            nb,
            nb + 1,
            1 << 16,
            1 << 30,
            c_int::MAX,
        ]
    }

    /// What C calls `f`: `av_get_pix_fmt_name`, NULL as `None`.
    fn c_name(f: ffi::AVPixelFormat) -> Option<&'static str> {
        // SAFETY: `av_get_pix_fmt_name` takes the enum by value and returns NULL
        // for any value outside its descriptor table.
        let name = unsafe { ffi::av_get_pix_fmt_name(f) };
        if name.is_null() {
            return None;
        }
        // SAFETY: a non-NULL result is a descriptor's static string literal in
        // `pixdesc.c`.
        Some(unsafe { CStr::from_ptr(name) }.to_str().unwrap())
    }

    /// The constant name `Debug` prints for `f`, or `None` for an unnamed value.
    fn rust_name(f: PixelFormat) -> Option<String> {
        let s = format!("{f:?}");
        (!s.starts_with("PixelFormat(")).then_some(s)
    }

    /// The C alias table, as `crustify_AV_PIX_FMT_NE` enumerates it.
    fn c_native_endian() -> Vec<(String, ffi::AVPixelFormat)> {
        let mut out = Vec::new();
        for i in 0.. {
            let mut name: *const c_char = ptr::null();
            // SAFETY: the shim takes `i` by value and writes one pointer through
            // `&mut name`, which is live and writable.
            let f = unsafe { ffi::crustify_AV_PIX_FMT_NE(i, &mut name) };
            if name.is_null() {
                assert_eq!(f, ffi::AVPixelFormat::AV_PIX_FMT_NONE);
                break;
            }
            // SAFETY: a non-NULL name is a string literal in the shim's table.
            let name = unsafe { CStr::from_ptr(name) }.to_str().unwrap();
            out.push((name.to_owned(), f));
        }
        out
    }

    #[test]
    fn named_values_are_exactly_the_ones_c_describes() {
        let nb = ffi::AVPixelFormat::AV_PIX_FMT_NB.0;
        let mut named = 0;
        for raw in (-3..nb + 4).chain(unnamed()) {
            let c = ffi::AVPixelFormat(raw);
            let f = PixelFormat::from_raw(c);
            assert_eq!(f.into_raw(), c);
            let rust = rust_name(f);
            if f == PixelFormat::NONE {
                // C describes no format for its sentinel.
                assert_eq!(rust.as_deref(), Some("NONE"));
                assert_eq!(c_name(c), None);
                continue;
            }
            assert_eq!(rust.is_some(), c_name(c).is_some(), "raw {raw}");
            if let Some(rust) = rust {
                named += 1;
                let expected = RENAMED
                    .iter()
                    .find(|&&(r, _)| r == f)
                    .map_or_else(|| rust.to_lowercase(), |&(_, n)| n.to_owned());
                assert_eq!(c_name(c), Some(expected.as_str()), "raw {raw}");
            }
        }
        assert_eq!(named, nb);
    }

    #[test]
    fn aliases_are_the_c_aliases() {
        for (alias, c) in [
            (PixelFormat::Y400A, ffi::AVPixelFormat::AV_PIX_FMT_Y400A),
            (PixelFormat::GRAY8A, ffi::AVPixelFormat::AV_PIX_FMT_GRAY8A),
            (PixelFormat::GBR24P, ffi::AVPixelFormat::AV_PIX_FMT_GBR24P),
        ] {
            assert_eq!(alias.into_raw(), c);
            assert_eq!(c_name(alias.into_raw()), c_name(c));
            assert!(c_name(c).is_some());
        }
        assert_eq!(c_name(PixelFormat::Y400A.into_raw()), Some("ya8"));
        assert_eq!(c_name(PixelFormat::GBR24P.into_raw()), Some("gbrp"));
    }

    #[test]
    fn native_endian_constants_are_the_c_aliases() {
        let c = c_native_endian();
        assert_eq!(c.len(), NATIVE_ENDIAN.len());
        for (name, value) in &c {
            let &(_, f) = NATIVE_ENDIAN
                .iter()
                .find(|&&(n, _)| n == name)
                .unwrap_or_else(|| panic!("no constant for AV_PIX_FMT_{name}"));
            assert_eq!(f.into_raw(), *value, "AV_PIX_FMT_{name}");
            assert_eq!(c_name(f.into_raw()), c_name(*value));
            assert!(c_name(*value).is_some(), "AV_PIX_FMT_{name}");
        }
    }

    /// A C block of `ffi::AVPixelFormat`, freed with `av_free` on drop.
    struct CFormats(*mut ffi::AVPixelFormat, usize);

    impl CFormats {
        fn zeroed(len: usize) -> Self {
            // SAFETY: `av_calloc` takes its sizes by value.
            let ptr = unsafe { ffi::av_calloc(len, size_of::<ffi::AVPixelFormat>()) };
            assert!(!ptr.is_null());
            Self(ptr.cast(), len)
        }

        fn get(&self, i: usize) -> ffi::AVPixelFormat {
            assert!(i < self.1);
            // SAFETY: `i` is in bounds of the live, initialised block.
            unsafe { self.0.add(i).read() }
        }

        fn set(&mut self, i: usize, f: ffi::AVPixelFormat) {
            assert!(i < self.1);
            // SAFETY: `i` is in bounds of the live block, which we own.
            unsafe { self.0.add(i).write(f) }
        }
    }

    impl Drop for CFormats {
        fn drop(&mut self) {
            // SAFETY: `self.0` is a live `av_calloc` block we own.
            unsafe { ffi::av_free(self.0.cast::<c_void>()) }
        }
    }

    /// The C values a Rust array holds, read back through its C-typed pointer.
    fn rust_as_c(a: &HeapVec<PixelFormat>) -> Vec<ffi::AVPixelFormat> {
        let ptr = a.as_ptr().cast::<ffi::AVPixelFormat>();
        (0..a.len())
            // SAFETY: `PixelFormat` is `#[repr(transparent)]` over
            // `ffi::AVPixelFormat`, and `i` is in bounds of the live, initialised
            // array `a` borrows.
            .map(|i| unsafe { ptr.add(i).read() })
            .collect()
    }

    fn c_values(c: &CFormats) -> Vec<ffi::AVPixelFormat> {
        (0..c.1).map(|i| c.get(i)).collect()
    }

    /// Named formats, an alias, native-endian ones and unnamed values.
    fn values() -> Vec<ffi::AVPixelFormat> {
        [
            PixelFormat::NONE,
            PixelFormat::YUV420P,
            PixelFormat::GRAY8A,
            PixelFormat::RGB32,
            PixelFormat::YUV420P10,
            PixelFormat::CUARRAY,
        ]
        .map(PixelFormat::into_raw)
        .into_iter()
        .chain(unnamed().map(ffi::AVPixelFormat))
        .collect()
    }

    #[test]
    fn zeroed_storage_reads_as_c_zeroed_storage() {
        const LEN: usize = 8;
        let rust = alloc_array_zeroed::<PixelFormat>(LEN).unwrap();
        let c = CFormats::zeroed(LEN);
        assert_eq!(rust_as_c(&rust), c_values(&c));
        // A zero-filled C struct's pixel format is `AV_PIX_FMT_YUV420P`, not
        // `AV_PIX_FMT_NONE`.
        assert!(rust.as_slice().iter().all(|&f| f == PixelFormat::YUV420P));
        assert_eq!(c_name(c.get(0)), Some("yuv420p"));
    }

    #[test]
    fn writes_through_each_side_agree_after_every_step() {
        let values = values();
        let len = values.len();
        let mut rust = alloc_array_zeroed::<PixelFormat>(len).unwrap();
        let mut c = CFormats::zeroed(len);
        for (step, &v) in values.iter().enumerate() {
            // Each step writes slot `step` and rotates an earlier one.
            rust.as_mut_slice()[step] = PixelFormat::from_raw(v);
            c.set(step, v);
            let back = (step * 5) % len;
            let w = values[(step + 3) % len];
            rust.as_mut_slice()[back] = w.into();
            c.set(back, w);
            assert_eq!(rust_as_c(&rust), c_values(&c), "step {step}");
            for i in 0..len {
                assert_eq!(
                    rust.as_slice()[i].into_raw(),
                    c.get(i),
                    "step {step} slot {i}"
                );
                assert_eq!(c_name(rust.as_slice()[i].into_raw()), c_name(c.get(i)));
            }
        }
    }

    #[test]
    fn none_terminated_c_list_reads_back_unchanged() {
        // The shape of C's format lists (`AVCodec.pix_fmts`,
        // `AVHWFramesConstraints.valid_sw_formats`): values up to a terminating
        // `AV_PIX_FMT_NONE`.
        let mut values = values();
        values.retain(|&v| v != ffi::AVPixelFormat::AV_PIX_FMT_NONE);
        values.push(ffi::AVPixelFormat::AV_PIX_FMT_NONE);
        let mut c = CFormats::zeroed(values.len());
        for (i, &v) in values.iter().enumerate() {
            c.set(i, v);
        }
        // The path a field accessor over such a list takes: adopt the C block
        // and read it as `PixelFormat`s.
        let len = c.1;
        let ptr = c.0;
        core::mem::forget(c);
        // SAFETY: `ptr` is a live `av_calloc` block of `len` initialised
        // `ffi::AVPixelFormat`, whose ownership `forget` gave up; `PixelFormat` is
        // layout-compatible with it, and `av_free` releases it.
        let adopted = unsafe { HeapVec::<PixelFormat>::from_raw_parts(ptr.cast(), len) }.unwrap();
        let listed: Vec<_> = adopted
            .as_slice()
            .iter()
            .take_while(|&&f| f != PixelFormat::NONE)
            .collect();
        assert_eq!(listed.len(), len - 1);
        for (f, &v) in listed.into_iter().zip(&values) {
            assert_eq!(*f, PixelFormat::from_raw(v));
            assert_eq!(rust_name(*f).is_some(), c_name(v).is_some());
        }
    }

    #[test]
    fn clone_copies_the_c_encoding() {
        let src: Vec<PixelFormat> = values().into_iter().map(PixelFormat::from).collect();
        let rust = HeapVec::from_slice(&src).unwrap();
        let copy = rust.clone();
        let mut c = CFormats::zeroed(src.len());
        for (i, f) in src.iter().enumerate() {
            c.set(i, f.into_raw());
        }
        assert_eq!(rust_as_c(&copy), c_values(&c));
        assert_eq!(rust_as_c(&rust), rust_as_c(&copy));
    }
}

/// [`ColorPrimaries`] and [`ColorRange`] against `AVColorPrimaries` and
/// `AVColorRange` (formats wave 0, batch 2): through `av_color_primaries_name`
/// / `av_color_range_name` and their `_from_name` inverses, the defaults
/// `av_frame_alloc` writes, and C-allocated storage holding the values.
mod color_primaries_and_range {
    use core::ffi::{CStr, c_int, c_uint, c_void};
    use core::mem::size_of;
    use core::ptr::{addr_of, addr_of_mut};

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::mem::{HeapVec, alloc_array_zeroed};
    use ffmpeg::libavutil::pixfmt::{ColorPrimaries, ColorRange};

    /// Every primaries constant, aliases included, beside its C enumerator and
    /// the name `av_color_primaries_name` gives it.
    const PRIMARIES_NAMED: [(ColorPrimaries, ffi::AVColorPrimaries, &str); 18] = [
        (
            ColorPrimaries::RESERVED0,
            ffi::AVColorPrimaries::AVCOL_PRI_RESERVED0,
            "reserved",
        ),
        (
            ColorPrimaries::BT709,
            ffi::AVColorPrimaries::AVCOL_PRI_BT709,
            "bt709",
        ),
        (
            ColorPrimaries::UNSPECIFIED,
            ffi::AVColorPrimaries::AVCOL_PRI_UNSPECIFIED,
            "unknown",
        ),
        (
            ColorPrimaries::RESERVED,
            ffi::AVColorPrimaries::AVCOL_PRI_RESERVED,
            "reserved",
        ),
        (
            ColorPrimaries::BT470M,
            ffi::AVColorPrimaries::AVCOL_PRI_BT470M,
            "bt470m",
        ),
        (
            ColorPrimaries::BT470BG,
            ffi::AVColorPrimaries::AVCOL_PRI_BT470BG,
            "bt470bg",
        ),
        (
            ColorPrimaries::SMPTE170M,
            ffi::AVColorPrimaries::AVCOL_PRI_SMPTE170M,
            "smpte170m",
        ),
        (
            ColorPrimaries::SMPTE240M,
            ffi::AVColorPrimaries::AVCOL_PRI_SMPTE240M,
            "smpte240m",
        ),
        (
            ColorPrimaries::FILM,
            ffi::AVColorPrimaries::AVCOL_PRI_FILM,
            "film",
        ),
        (
            ColorPrimaries::BT2020,
            ffi::AVColorPrimaries::AVCOL_PRI_BT2020,
            "bt2020",
        ),
        (
            ColorPrimaries::SMPTE428,
            ffi::AVColorPrimaries::AVCOL_PRI_SMPTE428,
            "smpte428",
        ),
        (
            ColorPrimaries::SMPTEST428_1,
            ffi::AVColorPrimaries::AVCOL_PRI_SMPTEST428_1,
            "smpte428",
        ),
        (
            ColorPrimaries::SMPTE431,
            ffi::AVColorPrimaries::AVCOL_PRI_SMPTE431,
            "smpte431",
        ),
        (
            ColorPrimaries::SMPTE432,
            ffi::AVColorPrimaries::AVCOL_PRI_SMPTE432,
            "smpte432",
        ),
        (
            ColorPrimaries::EBU3213,
            ffi::AVColorPrimaries::AVCOL_PRI_EBU3213,
            "ebu3213",
        ),
        (
            ColorPrimaries::JEDEC_P22,
            ffi::AVColorPrimaries::AVCOL_PRI_JEDEC_P22,
            "ebu3213",
        ),
        (
            ColorPrimaries::EXT_BASE,
            ffi::AVColorPrimaries::AVCOL_PRI_EXT_BASE,
            "vgamut",
        ),
        (
            ColorPrimaries::V_GAMUT,
            ffi::AVColorPrimaries::AVCOL_PRI_V_GAMUT,
            "vgamut",
        ),
    ];

    /// Every range constant beside its C enumerator and C name.
    const RANGE_NAMED: [(ColorRange, ffi::AVColorRange, &str); 3] = [
        (
            ColorRange::UNSPECIFIED,
            ffi::AVColorRange::AVCOL_RANGE_UNSPECIFIED,
            "unknown",
        ),
        (ColorRange::MPEG, ffi::AVColorRange::AVCOL_RANGE_MPEG, "tv"),
        (ColorRange::JPEG, ffi::AVColorRange::AVCOL_RANGE_JPEG, "pc"),
    ];

    /// Primaries values C may store though it names none of them: the H.273
    /// gap, both `NB` values, and the far ends of the encoding.
    const PRIMARIES_UNNAMED: [c_uint; 9] =
        [13, 17, 21, 23, 255, 257, 1 << 20, 1 << 31, c_uint::MAX];

    /// Range values C may store though it names none of them, `NB` included.
    const RANGE_UNNAMED: [c_uint; 6] = [3, 4, 255, 1 << 20, 1 << 31, c_uint::MAX];

    /// A static C string as `str`, NULL as `None`.
    fn c_str(name: *const core::ffi::c_char) -> Option<&'static str> {
        if name.is_null() {
            return None;
        }
        // SAFETY: a non-NULL result of the pixdesc.c name getters is one of
        // the static string literals in its name tables.
        Some(unsafe { CStr::from_ptr(name) }.to_str().unwrap())
    }

    /// What C calls `p`: `av_color_primaries_name`, NULL as `None`.
    fn c_primaries_name(p: ffi::AVColorPrimaries) -> Option<&'static str> {
        // SAFETY: takes the enum by value; any value is range-checked in C.
        c_str(unsafe { ffi::av_color_primaries_name(p) })
    }

    /// What C calls `r`: `av_color_range_name`, NULL as `None`.
    fn c_range_name(r: ffi::AVColorRange) -> Option<&'static str> {
        // SAFETY: takes the enum by value; any value is range-checked in C.
        c_str(unsafe { ffi::av_color_range_name(r) })
    }

    fn c_primaries_from_name(name: &CStr) -> c_int {
        // SAFETY: `name` is a live NUL-terminated string C only reads.
        unsafe { ffi::av_color_primaries_from_name(name.as_ptr()) }
    }

    fn c_range_from_name(name: &CStr) -> c_int {
        // SAFETY: `name` is a live NUL-terminated string C only reads.
        unsafe { ffi::av_color_range_from_name(name.as_ptr()) }
    }

    fn debug_is_named<T: core::fmt::Debug>(t: T, type_name: &str) -> bool {
        !format!("{t:?}").starts_with(&format!("{type_name}("))
    }

    /// A C block of `len` 4-byte enum values, freed with `av_free` on drop.
    struct CBlock<E>(*mut E, usize);

    impl<E: Copy> CBlock<E> {
        fn zeroed(len: usize) -> Self {
            // SAFETY: `av_calloc` takes its sizes by value.
            let ptr = unsafe { ffi::av_calloc(len, size_of::<E>()) };
            assert!(!ptr.is_null());
            Self(ptr.cast(), len)
        }

        fn get(&self, i: usize) -> E {
            assert!(i < self.1);
            // SAFETY: `i` is in bounds of the live, initialised block.
            unsafe { self.0.add(i).read() }
        }

        fn set(&mut self, i: usize, v: E) {
            assert!(i < self.1);
            // SAFETY: `i` is in bounds of the live block, which we own.
            unsafe { self.0.add(i).write(v) }
        }

        fn values(&self) -> Vec<E> {
            (0..self.1).map(|i| self.get(i)).collect()
        }

        /// Hands the block over, as a C field accessor would hand a Rust
        /// owner an `av_malloc`ed array.
        fn into_raw(self) -> (*mut E, usize) {
            let parts = (self.0, self.1);
            core::mem::forget(self);
            parts
        }
    }

    impl<E> Drop for CBlock<E> {
        fn drop(&mut self) {
            // SAFETY: `self.0` is a live `av_calloc` block we own.
            unsafe { ffi::av_free(self.0.cast::<c_void>()) }
        }
    }

    /// A wrapper and the C enum it is `#[repr(transparent)]` over.
    trait Wrapped: Copy {
        type C: Copy;
    }

    impl Wrapped for ColorPrimaries {
        type C = ffi::AVColorPrimaries;
    }

    impl Wrapped for ColorRange {
        type C = ffi::AVColorRange;
    }

    /// The C values a Rust array holds, read back through its C-typed pointer.
    fn rust_as_c<T: Wrapped>(a: &HeapVec<T>) -> Vec<T::C> {
        let ptr = a.as_ptr().cast::<T::C>();
        (0..a.len())
            // SAFETY: each `Wrapped` implementor is `#[repr(transparent)]` over
            // its `C`, and `i` is in bounds of the live, initialised array `a`
            // borrows.
            .map(|i| unsafe { ptr.add(i).read() })
            .collect()
    }

    /// A frame from `av_frame_alloc`, freed with `av_frame_free` on drop.
    struct CFrame(*mut ffi::AVFrame);

    impl CFrame {
        fn new() -> Self {
            // SAFETY: no arguments; NULL on failure, checked below.
            let f = unsafe { ffi::av_frame_alloc() };
            assert!(!f.is_null());
            Self(f)
        }

        fn primaries(&self) -> ffi::AVColorPrimaries {
            // SAFETY: `self.0` is a live frame; the field is read in place.
            unsafe { addr_of!((*self.0).color_primaries).read() }
        }

        fn range(&self) -> ffi::AVColorRange {
            // SAFETY: `self.0` is a live frame; the field is read in place.
            unsafe { addr_of!((*self.0).color_range).read() }
        }

        fn set(&mut self, p: ffi::AVColorPrimaries, r: ffi::AVColorRange) {
            // SAFETY: `self.0` is a live frame we own; the fields are written
            // in place.
            unsafe {
                addr_of_mut!((*self.0).color_primaries).write(p);
                addr_of_mut!((*self.0).color_range).write(r);
            }
        }
    }

    impl Drop for CFrame {
        fn drop(&mut self) {
            // SAFETY: `self.0` is a live `av_frame_alloc` frame we own;
            // `av_frame_free` frees it and NULLs the slot.
            unsafe { ffi::av_frame_free(&mut self.0) }
        }
    }

    #[test]
    fn primaries_constants_are_the_c_enumerators() {
        for (p, c, name) in PRIMARIES_NAMED {
            assert_eq!(p.into_raw(), c);
            assert_eq!(ColorPrimaries::from_raw(c), p);
            assert_eq!(c_primaries_name(p.into_raw()), Some(name));
        }
    }

    #[test]
    fn range_constants_are_the_c_enumerators() {
        for (r, c, name) in RANGE_NAMED {
            assert_eq!(r.into_raw(), c);
            assert_eq!(ColorRange::from_raw(c), r);
            assert_eq!(c_range_name(r.into_raw()), Some(name));
        }
    }

    #[test]
    fn c_name_lookups_land_on_the_constants() {
        // "reserved" names two primaries; C's lookup returns the first.
        for (p, _, name) in PRIMARIES_NAMED {
            let name = std::ffi::CString::new(name).unwrap();
            let raw = c_primaries_from_name(&name);
            assert!(raw >= 0, "{name:?}");
            let back = ColorPrimaries::from_raw(ffi::AVColorPrimaries(raw as c_uint));
            assert_eq!(
                c_primaries_name(back.into_raw()),
                c_primaries_name(p.into_raw())
            );
            if p != ColorPrimaries::RESERVED {
                assert_eq!(back, p);
            } else {
                assert_eq!(back, ColorPrimaries::RESERVED0);
            }
        }
        for (r, _, name) in RANGE_NAMED {
            let name = std::ffi::CString::new(name).unwrap();
            let raw = c_range_from_name(&name);
            assert_eq!(ColorRange::from_raw(ffi::AVColorRange(raw as c_uint)), r);
        }
        assert!(c_primaries_from_name(c"no-such-primaries") < 0);
        assert!(c_range_from_name(c"no-such-range") < 0);
    }

    #[test]
    fn debug_names_exactly_the_values_c_names() {
        let sweep = (0..=300).chain(PRIMARIES_UNNAMED);
        for raw in sweep.clone() {
            let p = ColorPrimaries::from_raw(ffi::AVColorPrimaries(raw));
            assert_eq!(
                debug_is_named(p, "ColorPrimaries"),
                c_primaries_name(p.into_raw()).is_some(),
                "primaries {raw}"
            );
            assert_eq!(p.into_raw().0, raw);
        }
        for raw in sweep.chain(RANGE_UNNAMED) {
            let r = ColorRange::from_raw(ffi::AVColorRange(raw));
            assert_eq!(
                debug_is_named(r, "ColorRange"),
                c_range_name(r.into_raw()).is_some(),
                "range {raw}"
            );
            assert_eq!(r.into_raw().0, raw);
        }
    }

    #[test]
    fn defaults_are_what_c_initialises_a_frame_with() {
        let frame = CFrame::new();
        assert_eq!(ColorPrimaries::default().into_raw(), frame.primaries());
        assert_eq!(ColorRange::default().into_raw(), frame.range());
        assert_eq!(
            c_primaries_name(ColorPrimaries::default().into_raw()),
            Some("unknown")
        );
        assert_eq!(
            c_range_name(ColorRange::default().into_raw()),
            Some("unknown")
        );
    }

    #[test]
    fn frame_fields_carry_rust_values_to_c_after_every_step() {
        let mut frame = CFrame::new();
        let primaries: Vec<ColorPrimaries> = PRIMARIES_NAMED
            .map(|(p, _, _)| p)
            .into_iter()
            .chain(PRIMARIES_UNNAMED.map(|r| ffi::AVColorPrimaries(r).into()))
            .collect();
        let ranges: Vec<ColorRange> = RANGE_NAMED
            .map(|(r, _, _)| r)
            .into_iter()
            .chain(RANGE_UNNAMED.map(|r| ffi::AVColorRange(r).into()))
            .collect();
        for (step, &p) in primaries.iter().enumerate() {
            let r = ranges[step % ranges.len()];
            frame.set(p.into(), r.into());
            assert_eq!(ColorPrimaries::from(frame.primaries()), p, "step {step}");
            assert_eq!(ColorRange::from(frame.range()), r, "step {step}");
            assert_eq!(
                c_primaries_name(frame.primaries()),
                c_primaries_name(p.into_raw())
            );
            assert_eq!(c_range_name(frame.range()), c_range_name(r.into_raw()));
        }
    }

    #[test]
    fn zeroed_storage_reads_as_c_zeroed_storage() {
        const LEN: usize = 8;
        let p = alloc_array_zeroed::<ColorPrimaries>(LEN).unwrap();
        let cp = CBlock::<ffi::AVColorPrimaries>::zeroed(LEN);
        assert_eq!(rust_as_c(&p), cp.values());
        // A zero-filled C struct's primaries are `AVCOL_PRI_RESERVED0`, not
        // the `UNSPECIFIED` C's initialisers write.
        assert!(p.as_slice().iter().all(|&v| v == ColorPrimaries::RESERVED0));
        let r = alloc_array_zeroed::<ColorRange>(LEN).unwrap();
        let cr = CBlock::<ffi::AVColorRange>::zeroed(LEN);
        assert_eq!(rust_as_c(&r), cr.values());
        assert!(r.as_slice().iter().all(|&v| v == ColorRange::default()));
    }

    #[test]
    fn writes_through_each_side_agree_after_every_step() {
        let values: Vec<ffi::AVColorPrimaries> = PRIMARIES_NAMED
            .iter()
            .map(|&(_, c, _)| c)
            .chain(PRIMARIES_UNNAMED.map(ffi::AVColorPrimaries))
            .collect();
        let ranges: Vec<ffi::AVColorRange> = RANGE_NAMED
            .iter()
            .map(|&(_, c, _)| c)
            .chain(RANGE_UNNAMED.map(ffi::AVColorRange))
            .collect();
        let len = values.len();
        let mut rp = alloc_array_zeroed::<ColorPrimaries>(len).unwrap();
        let mut cp = CBlock::zeroed(len);
        let mut rr = alloc_array_zeroed::<ColorRange>(len).unwrap();
        let mut cr = CBlock::zeroed(len);
        for (step, &v) in values.iter().enumerate() {
            // Each step writes slot `step` and rotates an earlier one.
            rp.as_mut_slice()[step] = ColorPrimaries::from_raw(v);
            cp.set(step, v);
            let back = (step * 7) % len;
            let w = values[(step + 5) % len];
            rp.as_mut_slice()[back] = w.into();
            cp.set(back, w);
            let r = ranges[step % ranges.len()];
            rr.as_mut_slice()[step] = r.into();
            cr.set(step, r);
            assert_eq!(rust_as_c(&rp), cp.values(), "step {step}");
            assert_eq!(rust_as_c(&rr), cr.values(), "step {step}");
            for i in 0..len {
                assert_eq!(
                    c_primaries_name(rp.as_slice()[i].into_raw()),
                    c_primaries_name(cp.get(i)),
                    "step {step} slot {i}"
                );
                assert_eq!(
                    c_range_name(rr.as_slice()[i].into_raw()),
                    c_range_name(cr.get(i))
                );
            }
        }
    }

    #[test]
    fn values_c_writes_are_read_back_unchanged() {
        let values: Vec<_> = PRIMARIES_NAMED
            .map(|(_, c, _)| c)
            .into_iter()
            .chain(PRIMARIES_UNNAMED.map(ffi::AVColorPrimaries))
            .collect();
        let mut c = CBlock::zeroed(values.len());
        for (i, &v) in values.iter().enumerate() {
            c.set(i, v);
        }
        let (ptr, len) = c.into_raw();
        // SAFETY: `ptr` is a live `av_calloc` block of `len` initialised
        // `ffi::AVColorPrimaries`, whose ownership `into_raw` gave up;
        // `ColorPrimaries` is layout-compatible with it, and `av_free`
        // releases it.
        let adopted = unsafe { HeapVec::<ColorPrimaries>::from_raw_parts(ptr.cast(), len) };
        let adopted = adopted.unwrap();
        for (i, &v) in values.iter().enumerate() {
            assert_eq!(adopted.as_slice()[i], ColorPrimaries::from_raw(v));
            assert_eq!(
                debug_is_named(adopted.as_slice()[i], "ColorPrimaries"),
                c_primaries_name(v).is_some()
            );
        }

        let ranges: Vec<_> = RANGE_NAMED
            .map(|(_, c, _)| c)
            .into_iter()
            .chain(RANGE_UNNAMED.map(ffi::AVColorRange))
            .collect();
        let mut c = CBlock::zeroed(ranges.len());
        for (i, &v) in ranges.iter().enumerate() {
            c.set(i, v);
        }
        let (ptr, len) = c.into_raw();
        // SAFETY: `ptr` is a live `av_calloc` block of `len` initialised
        // `ffi::AVColorRange`, whose ownership `into_raw` gave up; `ColorRange`
        // is layout-compatible with it, and `av_free` releases it.
        let adopted = unsafe { HeapVec::<ColorRange>::from_raw_parts(ptr.cast(), len) };
        let adopted = adopted.unwrap();
        for (i, &v) in ranges.iter().enumerate() {
            assert_eq!(adopted.as_slice()[i], ColorRange::from_raw(v));
        }
    }

    #[test]
    fn clone_copies_the_c_encoding() {
        let src: Vec<ColorPrimaries> = PRIMARIES_NAMED
            .map(|(p, _, _)| p)
            .into_iter()
            .chain(PRIMARIES_UNNAMED.map(|r| ffi::AVColorPrimaries(r).into()))
            .collect();
        let rust = HeapVec::from_slice(&src).unwrap();
        let copy = rust.clone();
        let mut c = CBlock::zeroed(src.len());
        for (i, p) in src.iter().enumerate() {
            c.set(i, p.into_raw());
        }
        assert_eq!(rust_as_c(&copy), c.values());
        assert_eq!(rust_as_c(&rust), rust_as_c(&copy));

        let src: Vec<ColorRange> = RANGE_NAMED
            .map(|(r, _, _)| r)
            .into_iter()
            .chain(RANGE_UNNAMED.map(|r| ffi::AVColorRange(r).into()))
            .collect();
        let copy = HeapVec::from_slice(&src).unwrap().clone();
        let mut c = CBlock::zeroed(src.len());
        for (i, r) in src.iter().enumerate() {
            c.set(i, r.into_raw());
        }
        assert_eq!(rust_as_c(&copy), c.values());
    }
}
