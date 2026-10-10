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
