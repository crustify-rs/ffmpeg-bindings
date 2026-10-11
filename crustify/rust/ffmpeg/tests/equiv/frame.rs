//! Equivalence of `libavutil/frame.h`'s wrapped types with C, as C observes
//! them.

/// [`SideDataType`] against C's `enum AVFrameSideDataType`, as C observes it:
/// through `av_frame_side_data_name`'s descriptor table, and as the
/// `AVFrameSideData.type` a frame's side-data list stores, looks up and
/// removes by (buffer-frame wave 0, batch 2).
///
/// [`SideDataType`]: ffmpeg::libavutil::frame::SideDataType
mod side_data_type {
    use core::ffi::CStr;
    use core::ptr;

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::frame::SideDataType;
    use ffmpeg::libavutil::mem::{HeapVec, alloc_array_zeroed};

    use ffi::AVFrameSideDataType as C;

    /// Every named constant beside its C enumerator, in declaration order.
    const NAMED: [(SideDataType, C); 38] = [
        (SideDataType::PAN_SCAN, C::AV_FRAME_DATA_PANSCAN),
        (SideDataType::A53_CC, C::AV_FRAME_DATA_A53_CC),
        (SideDataType::STEREO_3D, C::AV_FRAME_DATA_STEREO3D),
        (
            SideDataType::MATRIX_ENCODING,
            C::AV_FRAME_DATA_MATRIXENCODING,
        ),
        (SideDataType::DOWNMIX_INFO, C::AV_FRAME_DATA_DOWNMIX_INFO),
        (SideDataType::REPLAY_GAIN, C::AV_FRAME_DATA_REPLAYGAIN),
        (SideDataType::DISPLAY_MATRIX, C::AV_FRAME_DATA_DISPLAYMATRIX),
        (SideDataType::AFD, C::AV_FRAME_DATA_AFD),
        (
            SideDataType::MOTION_VECTORS,
            C::AV_FRAME_DATA_MOTION_VECTORS,
        ),
        (SideDataType::SKIP_SAMPLES, C::AV_FRAME_DATA_SKIP_SAMPLES),
        (
            SideDataType::AUDIO_SERVICE_TYPE,
            C::AV_FRAME_DATA_AUDIO_SERVICE_TYPE,
        ),
        (
            SideDataType::MASTERING_DISPLAY_METADATA,
            C::AV_FRAME_DATA_MASTERING_DISPLAY_METADATA,
        ),
        (SideDataType::GOP_TIMECODE, C::AV_FRAME_DATA_GOP_TIMECODE),
        (SideDataType::SPHERICAL, C::AV_FRAME_DATA_SPHERICAL),
        (
            SideDataType::CONTENT_LIGHT_LEVEL,
            C::AV_FRAME_DATA_CONTENT_LIGHT_LEVEL,
        ),
        (SideDataType::ICC_PROFILE, C::AV_FRAME_DATA_ICC_PROFILE),
        (SideDataType::S12M_TIMECODE, C::AV_FRAME_DATA_S12M_TIMECODE),
        (
            SideDataType::DYNAMIC_HDR_PLUS,
            C::AV_FRAME_DATA_DYNAMIC_HDR_PLUS,
        ),
        (
            SideDataType::REGIONS_OF_INTEREST,
            C::AV_FRAME_DATA_REGIONS_OF_INTEREST,
        ),
        (
            SideDataType::VIDEO_ENC_PARAMS,
            C::AV_FRAME_DATA_VIDEO_ENC_PARAMS,
        ),
        (
            SideDataType::SEI_UNREGISTERED,
            C::AV_FRAME_DATA_SEI_UNREGISTERED,
        ),
        (
            SideDataType::FILM_GRAIN_PARAMS,
            C::AV_FRAME_DATA_FILM_GRAIN_PARAMS,
        ),
        (
            SideDataType::DETECTION_BBOXES,
            C::AV_FRAME_DATA_DETECTION_BBOXES,
        ),
        (
            SideDataType::DOVI_RPU_BUFFER,
            C::AV_FRAME_DATA_DOVI_RPU_BUFFER,
        ),
        (SideDataType::DOVI_METADATA, C::AV_FRAME_DATA_DOVI_METADATA),
        (
            SideDataType::DYNAMIC_HDR_VIVID,
            C::AV_FRAME_DATA_DYNAMIC_HDR_VIVID,
        ),
        (
            SideDataType::AMBIENT_VIEWING_ENVIRONMENT,
            C::AV_FRAME_DATA_AMBIENT_VIEWING_ENVIRONMENT,
        ),
        (SideDataType::VIDEO_HINT, C::AV_FRAME_DATA_VIDEO_HINT),
        (SideDataType::LCEVC, C::AV_FRAME_DATA_LCEVC),
        (SideDataType::VIEW_ID, C::AV_FRAME_DATA_VIEW_ID),
        (
            SideDataType::REFERENCE_DISPLAYS_3D,
            C::AV_FRAME_DATA_3D_REFERENCE_DISPLAYS,
        ),
        (SideDataType::EXIF, C::AV_FRAME_DATA_EXIF),
        (
            SideDataType::DYNAMIC_HDR_SMPTE_2094_APP5,
            C::AV_FRAME_DATA_DYNAMIC_HDR_SMPTE_2094_APP5,
        ),
        (
            SideDataType::IAMF_MIX_GAIN_PARAM,
            C::AV_FRAME_DATA_IAMF_MIX_GAIN_PARAM,
        ),
        (
            SideDataType::IAMF_DEMIXING_INFO_PARAM,
            C::AV_FRAME_DATA_IAMF_DEMIXING_INFO_PARAM,
        ),
        (
            SideDataType::IAMF_RECON_GAIN_INFO_PARAM,
            C::AV_FRAME_DATA_IAMF_RECON_GAIN_INFO_PARAM,
        ),
        (
            SideDataType::RAW_COLOR_PARAMS,
            C::AV_FRAME_DATA_RAW_COLOR_PARAMS,
        ),
        (
            SideDataType::DOWNMIX_MATRIX,
            C::AV_FRAME_DATA_DOWNMIX_MATRIX,
        ),
    ];

    /// Values a user may store in `AVFrameSideData.type` though C names none
    /// of them: just past the table, and the `unsigned` extremes a negative
    /// `int` written there reads back as.
    const UNNAMED: [u32; 7] = [
        38,
        39,
        64,
        1 << 20,
        i32::MAX as u32,
        i32::MIN as u32,
        u32::MAX,
    ];

    /// What C calls `t`: `av_frame_side_data_name`, NULL as `None`.
    fn c_name(t: C) -> Option<&'static CStr> {
        // SAFETY: `av_frame_side_data_name` takes the enum by value and
        // bounds-checks it against its descriptor table.
        let name = unsafe { ffi::av_frame_side_data_name(t) };
        // SAFETY: a non-NULL result is a string literal in `side_data.c`'s
        // static `sd_props` table.
        (!name.is_null()).then(|| unsafe { CStr::from_ptr(name) })
    }

    /// Whether the wrapper names `t`: its `Debug` is a constant's name, not
    /// the raw tuple form.
    fn rust_names(t: SideDataType) -> bool {
        !format!("{t:?}").starts_with("SideDataType(")
    }

    #[test]
    fn constants_are_the_c_enumerators() {
        for (t, c) in NAMED {
            assert_eq!(t.into_raw(), c);
            assert_eq!(SideDataType::from_raw(c), t);
            assert_eq!(SideDataType::from(c), t);
            assert_eq!(C::from(t), c);
            assert!(c_name(t.into_raw()).is_some(), "{t:?}");
        }
    }

    #[test]
    fn the_wrapper_names_exactly_what_c_names() {
        for raw in (0..=96).chain(UNNAMED) {
            let t = SideDataType::from_raw(C(raw));
            assert_eq!(rust_names(t), c_name(C(raw)).is_some(), "raw {raw}");
            assert_eq!(t.into_raw().0, raw);
        }
    }

    /// An `av_frame_alloc` frame, freed with `av_frame_free` on drop.
    struct Frame(*mut ffi::AVFrame);

    impl Frame {
        fn new() -> Self {
            // SAFETY: `av_frame_alloc` takes no arguments.
            let f = unsafe { ffi::av_frame_alloc() };
            assert!(!f.is_null());
            Self(f)
        }

        /// The types of the frame's side data, in C's list order.
        fn types(&self) -> Vec<SideDataType> {
            // SAFETY: `self.0` is a live frame; its side-data list holds
            // `nb_side_data` live entries, which only the C calls below
            // change, none of which runs while this borrow reads them.
            unsafe {
                let n = ptr::addr_of!((*self.0).nb_side_data).read();
                let list = ptr::addr_of!((*self.0).side_data).read();
                (0..usize::try_from(n).unwrap())
                    .map(|i| {
                        let sd = list.add(i).read();
                        SideDataType::from_raw(ptr::addr_of!((*sd).type_).read())
                    })
                    .collect()
            }
        }

        fn add(&mut self, t: SideDataType, size: usize) -> bool {
            // SAFETY: `self.0` is a live frame we own exclusively.
            let sd = unsafe { ffi::av_frame_new_side_data(self.0, t.into_raw(), size) };
            if sd.is_null() {
                return false;
            }
            // SAFETY: a non-NULL result is the entry C just appended.
            let stored = unsafe { ptr::addr_of!((*sd).type_).read() };
            assert_eq!(SideDataType::from_raw(stored), t);
            true
        }

        /// The type of the first entry C finds for `t`, if any.
        fn get(&self, t: SideDataType) -> Option<SideDataType> {
            // SAFETY: `self.0` is a live frame.
            let sd = unsafe { ffi::av_frame_get_side_data(self.0, t.into_raw()) };
            if sd.is_null() {
                return None;
            }
            // SAFETY: a non-NULL result is one of the frame's live entries.
            let raw = unsafe { ptr::addr_of!((*sd).type_).read() };
            Some(SideDataType::from_raw(raw))
        }

        fn remove(&mut self, t: SideDataType) {
            // SAFETY: `self.0` is a live frame we own exclusively.
            unsafe { ffi::av_frame_remove_side_data(self.0, t.into_raw()) }
        }
    }

    impl Drop for Frame {
        fn drop(&mut self) {
            // SAFETY: `self.0` is a live `av_frame_alloc` frame we own.
            unsafe { ffi::av_frame_free(&mut self.0) }
        }
    }

    /// C's removal, modelled on the Rust values: walk backwards, moving the
    /// last entry into each removed slot.
    fn model_remove(list: &mut Vec<SideDataType>, t: SideDataType) {
        for i in (0..list.len()).rev() {
            if list[i] == t {
                list.swap_remove(i);
            }
        }
    }

    #[test]
    fn frame_side_data_lists_store_find_and_remove_rust_values_as_c() {
        let values: Vec<SideDataType> = NAMED
            .iter()
            .map(|&(t, _)| t)
            .chain(UNNAMED.map(|r| SideDataType::from_raw(C(r))))
            .collect();
        let mut frame = Frame::new();
        let mut model: Vec<SideDataType> = Vec::new();
        for (step, &t) in values.iter().enumerate() {
            // Each type is added once, every third one twice (C keeps
            // duplicates), and every fifth step removes an earlier type.
            let copies = if step % 3 == 0 { 2 } else { 1 };
            for _ in 0..copies {
                assert!(frame.add(t, step % 7));
                model.push(t);
            }
            if step % 5 == 4 {
                let gone = values[step / 2];
                frame.remove(gone);
                model_remove(&mut model, gone);
                assert_eq!(frame.get(gone), None, "step {step}");
            }
            assert_eq!(frame.types(), model, "step {step}");
            for &u in &values {
                assert_eq!(
                    frame.get(u),
                    model.contains(&u).then_some(u),
                    "step {step} {u:?}"
                );
            }
        }
        // Removing every type, named or not, empties the list.
        for &t in &values {
            frame.remove(t);
            model_remove(&mut model, t);
            assert_eq!(frame.types(), model);
        }
        assert!(model.is_empty());
    }

    #[test]
    fn values_c_stores_read_back_through_rust_arrays() {
        // The path a field accessor over a C array of the enum takes: a
        // zeroed `av_malloc`-family array reads as C's zero, `PAN_SCAN`,
        // and every value written through Rust is the C encoding.
        let mut a: HeapVec<SideDataType> = alloc_array_zeroed(NAMED.len() + UNNAMED.len()).unwrap();
        assert!(
            a.as_slice()
                .iter()
                .all(|&t| t.into_raw() == C::AV_FRAME_DATA_PANSCAN)
        );
        let values: Vec<C> = NAMED
            .iter()
            .map(|&(_, c)| c)
            .chain(UNNAMED.map(C))
            .collect();
        for (slot, &c) in a.as_mut_slice().iter_mut().zip(&values) {
            *slot = c.into();
        }
        let ptr = a.as_ptr().cast::<C>();
        for (i, &c) in values.iter().enumerate() {
            // SAFETY: `SideDataType` is `#[repr(transparent)]` over `C`, and
            // `i` is in bounds of the live, initialised array.
            let raw = unsafe { ptr.add(i).read() };
            assert_eq!(raw, c);
            assert_eq!(c_name(raw).is_some(), rust_names(a.as_slice()[i]));
        }
    }
}

/// [`ActiveFormatDescription`] against `AVActiveFormatDescription`
/// (buffer-frame wave 0, batch 0): C's enumerator values, and the
/// `AV_FRAME_DATA_AFD` side-data byte C frames store, look up and copy
/// through `av_frame_copy_props`. No C routine reads or writes the enum itself.
///
/// [`ActiveFormatDescription`]: ffmpeg::libavutil::frame::ActiveFormatDescription
mod active_format_description {
    use core::ptr::addr_of;

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::frame::ActiveFormatDescription as Afd;

    type CAfd = ffi::AVActiveFormatDescription;

    /// Every named constant beside its C enumerator.
    const NAMED: [(Afd, CAfd); 7] = [
        (Afd::SAME, CAfd::AV_AFD_SAME),
        (Afd::ASPECT_4_3, CAfd::AV_AFD_4_3),
        (Afd::ASPECT_16_9, CAfd::AV_AFD_16_9),
        (Afd::ASPECT_14_9, CAfd::AV_AFD_14_9),
        (Afd::ASPECT_4_3_SP_14_9, CAfd::AV_AFD_4_3_SP_14_9),
        (Afd::ASPECT_16_9_SP_14_9, CAfd::AV_AFD_16_9_SP_14_9),
        (Afd::SP_4_3, CAfd::AV_AFD_SP_4_3),
    ];

    /// A frame from `av_frame_alloc`, freed with `av_frame_free` on drop.
    struct CFrame(*mut ffi::AVFrame);

    impl CFrame {
        fn new() -> Self {
            // SAFETY: no arguments; NULL on failure, checked below.
            let f = unsafe { ffi::av_frame_alloc() };
            assert!(!f.is_null());
            Self(f)
        }

        /// A new frame holding a copy of this frame's properties, its side
        /// data included, as `av_frame_copy_props` deep-copies them.
        fn copy_props_c(&self) -> Self {
            let copy = Self::new();
            // SAFETY: both frames are live and distinct; `copy.0` is ours.
            assert_eq!(unsafe { ffi::av_frame_copy_props(copy.0, self.0) }, 0);
            copy
        }

        /// Adds one byte of AFD side data, as decoders do, and stores `byte`.
        fn add_afd(&mut self, byte: u8) {
            // SAFETY: `self.0` is a live frame we own; NULL on failure,
            // checked below.
            let sd = unsafe {
                ffi::av_frame_new_side_data(self.0, ffi::AVFrameSideDataType::AV_FRAME_DATA_AFD, 1)
            };
            assert!(!sd.is_null());
            // SAFETY: `sd` is the live entry C just added to our frame; its
            // `data` is a writable `av_buffer_alloc` block of `size` bytes.
            unsafe {
                assert_eq!(addr_of!((*sd).size).read(), 1);
                addr_of!((*sd).data).read().write(byte);
            }
        }

        /// Overwrites the AFD byte C finds on this frame.
        fn set_afd(&mut self, byte: u8) {
            let sd = self.afd_entry();
            // SAFETY: `sd` is a live entry of our frame with at least one
            // byte of `data`, which no Rust reference covers.
            unsafe { addr_of!((*sd).data).read().write(byte) }
        }

        /// The AFD byte C finds on this frame.
        fn afd(&self) -> u8 {
            let sd = self.afd_entry();
            // SAFETY: `sd` is a live entry of our frame with at least one
            // byte of `data`.
            unsafe { addr_of!((*sd).data).read().read() }
        }

        /// C's own view of the AFD byte: the payload converted to the enum,
        /// as `(enum AVActiveFormatDescription)sd->data[0]` reads it.
        fn afd_as_c_enum(&self) -> CAfd {
            ffi::AVActiveFormatDescription(self.afd().into())
        }

        fn afd_entry(&self) -> *mut ffi::AVFrameSideData {
            // SAFETY: `self.0` is a live frame.
            let sd = unsafe {
                ffi::av_frame_get_side_data(self.0, ffi::AVFrameSideDataType::AV_FRAME_DATA_AFD)
            };
            assert!(!sd.is_null());
            // SAFETY: `sd` is a live entry of our frame.
            assert!(unsafe { addr_of!((*sd).size).read() } >= 1);
            sd
        }
    }

    impl Drop for CFrame {
        fn drop(&mut self) {
            // SAFETY: `self.0` is a live frame we own; `av_frame_free`
            // releases it with its side data and NULLs the slot.
            unsafe { ffi::av_frame_free(&mut self.0) }
        }
    }

    #[test]
    fn constants_are_the_c_enumerators() {
        for (afd, c) in NAMED {
            assert_eq!(afd.into_raw(), c);
            assert_eq!(Afd::from_raw(c), afd);
            assert_eq!(Afd::from(c), afd);
            // C's implicit `uint8_t` conversion of each enumerator, as
            // mpeg12dec stores `*sd->data = s1->afd`.
            assert_eq!(afd.to_byte(), Some(c.0 as u8));
            assert_eq!(Afd::from_byte(c.0 as u8), afd);
        }
    }

    #[test]
    fn named_constants_round_trip_through_c_side_data() {
        for (afd, c) in NAMED {
            let mut frame = CFrame::new();
            frame.add_afd(afd.to_byte().unwrap());
            assert_eq!(frame.afd_as_c_enum(), c);
            assert_eq!(Afd::from_byte(frame.afd()), afd);
            let copy = frame.copy_props_c();
            assert_eq!(copy.afd_as_c_enum(), c);
            assert_eq!(Afd::from_byte(copy.afd()), afd);
        }
    }

    /// Every payload byte, reserved codes and out-of-nibble values included,
    /// through a sequence of C stores, lookups and copies, comparing Rust's
    /// view with C's after each step.
    #[test]
    fn every_payload_byte_reads_as_c_reads_it() {
        for byte in 0..=u8::MAX {
            let mut frame = CFrame::new();
            frame.add_afd(Afd::from_byte(byte).to_byte().unwrap());
            assert_eq!(frame.afd(), byte);
            assert_eq!(
                Afd::from_byte(frame.afd()).into_raw(),
                frame.afd_as_c_enum()
            );

            // The copy holds its own byte: a later store through the original
            // changes only the original.
            let mut copy = frame.copy_props_c();
            assert_eq!(copy.afd(), byte);
            assert_eq!(Afd::from_byte(copy.afd()).into_raw(), copy.afd_as_c_enum());
            let nibble = byte & 0xf;
            frame.set_afd(Afd::from_byte(nibble).to_byte().unwrap());
            for (f, held) in [(&frame, nibble), (&copy, byte)] {
                assert_eq!(f.afd(), held);
                assert_eq!(Afd::from_byte(f.afd()), Afd::from_raw(f.afd_as_c_enum()));
                assert_eq!(Afd::from_byte(f.afd()).to_byte(), Some(held));
            }

            // The copy outlives the original, and a copy of a copy carries
            // its latest store.
            drop(frame);
            copy.set_afd(Afd::SP_4_3.to_byte().unwrap());
            let again = copy.copy_props_c();
            drop(copy);
            assert_eq!(
                again.afd_as_c_enum(),
                ffi::AVActiveFormatDescription::AV_AFD_SP_4_3
            );
            assert_eq!(Afd::from_byte(again.afd()), Afd::SP_4_3);
        }
    }
}

/// `SideDataDescriptor` and `SideDataProps` against C's descriptor table and
/// the C routines that read a descriptor's `props`: the getters read what C's
/// table stores and what `av_frame_side_data_name` returns, descriptors
/// rebuilt through the setters encode as C's entries, and the property sets
/// decide what `av_frame_side_data_remove_by_props` removes and whether
/// `av_frame_side_data_new` appends a second entry of a type, step by step
/// (buffer-frame wave 0, batch 4).
mod side_data_descriptor {
    use core::ffi::{CStr, c_int, c_uint};
    use core::ptr;

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::frame::{SideDataDescriptor, SideDataProps};

    /// Side data type values to look up: every enumerator, the first values
    /// past them, and values that only fit as `unsigned`.
    fn types() -> impl Iterator<Item = c_uint> {
        (0..64).chain([
            c_int::MAX as c_uint - 1,
            c_int::MAX as c_uint,
            c_int::MAX as c_uint + 1,
            c_uint::MAX,
        ])
    }

    /// C's table entry for `t`, read by value.
    fn c_entry(t: c_uint) -> Option<ffi::AVSideDataDescriptor> {
        // SAFETY: `av_frame_side_data_desc` takes any value and returns NULL
        // or a pointer into C's static, never-written table.
        let p = unsafe { ffi::av_frame_side_data_desc(ffi::AVFrameSideDataType(t)) };
        // SAFETY: a non-null `p` points to a live entry of that table.
        (!p.is_null()).then(|| unsafe { p.read() })
    }

    /// C's table entry for `t` as the Rust value.
    fn rust_entry(t: c_uint) -> Option<SideDataDescriptor> {
        // SAFETY: C's table entries' names are string literals.
        c_entry(t).map(|r| unsafe { SideDataDescriptor::from_raw(r) })
    }

    /// C's name for `t`, through the routine that reads it from the table.
    fn c_name(t: c_uint) -> Option<&'static CStr> {
        // SAFETY: takes any value; returns NULL or a table string literal.
        let p = unsafe { ffi::av_frame_side_data_name(ffi::AVFrameSideDataType(t)) };
        // SAFETY: a non-null `p` is a literal in C's static table.
        (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) })
    }

    /// The highest described type plus one: C's table has no gap below it.
    fn described() -> c_uint {
        types().take_while(|&t| c_entry(t).is_some()).count() as c_uint
    }

    #[test]
    fn getters_read_what_c_stores_for_every_type() {
        let n = described();
        assert_eq!(
            n,
            ffi::AVFrameSideDataType::AV_FRAME_DATA_DOWNMIX_MATRIX.0 + 1
        );
        for t in types() {
            let (c, r) = (c_entry(t), rust_entry(t));
            assert_eq!(c.is_some(), t < n, "type {t}");
            let (Some(c), Some(r)) = (c, r) else {
                assert_eq!(c_name(t), None);
                continue;
            };
            let h = r.as_ref();
            assert_eq!(
                h.name().map_or(ptr::null(), CStr::as_ptr),
                c.name,
                "type {t}"
            );
            assert_eq!(h.name(), c_name(t), "type {t}");
            assert!(!h.name().unwrap().is_empty());
            assert_eq!(h.props().bits(), c.props, "type {t}");
            // Only named bits in C's table.
            assert_eq!(SideDataProps::from_bits(c.props), Some(h.props()));
        }
    }

    #[test]
    fn descriptors_rebuilt_through_setters_encode_as_c_s_entries() {
        for t in 0..described() {
            let c = c_entry(t).unwrap();
            let adopted = rust_entry(t).unwrap();
            // The properties as a union of named constants.
            let mut props = SideDataProps::empty();
            for flag in [
                SideDataProps::GLOBAL,
                SideDataProps::MULTI,
                SideDataProps::SIZE_DEPENDENT,
                SideDataProps::COLOR_DEPENDENT,
                SideDataProps::CHANNEL_DEPENDENT,
            ] {
                props.set(flag, adopted.as_ref().props().contains(flag));
            }
            let mut built = SideDataDescriptor::new();
            built.as_mut().set_name(adopted.as_ref().name());
            built.as_mut().set_props(props);
            let raw = built.into_raw();
            assert_eq!((raw.name, raw.props), (c.name, c.props), "type {t}");
            assert_eq!(built, adopted);
            let named = SideDataDescriptor::named(adopted.as_ref().name().unwrap(), props);
            assert_eq!(named, adopted);
            assert_eq!(adopted.as_ref().get().into_raw().name, c.name);
        }
    }

    /// An `AVFrameSideData *` array C grows and shrinks, freed on drop.
    struct Array {
        sd: *mut *mut ffi::AVFrameSideData,
        nb: c_int,
    }

    impl Array {
        fn new() -> Self {
            Self {
                sd: ptr::null_mut(),
                nb: 0,
            }
        }

        /// `av_frame_side_data_new` with no flag; whether C returned an entry.
        fn add(&mut self, t: c_uint) -> bool {
            // SAFETY: `sd` and `nb` are an array C built (or NULL and 0).
            let e = unsafe {
                ffi::av_frame_side_data_new(
                    &raw mut self.sd,
                    &raw mut self.nb,
                    ffi::AVFrameSideDataType(t),
                    4,
                    0,
                )
            };
            !e.is_null()
        }

        fn remove_by_props(&mut self, props: SideDataProps) {
            // SAFETY: as above; C reinterprets the bits as `int`, as its
            // callers' `int` masks are.
            unsafe {
                ffi::av_frame_side_data_remove_by_props(
                    &raw mut self.sd,
                    &raw mut self.nb,
                    props.bits() as c_int,
                )
            }
        }

        /// The entries' types, sorted.
        fn types(&self) -> Vec<c_uint> {
            let mut v: Vec<c_uint> = (0..self.nb as usize)
                .map(|i| {
                    // SAFETY: `i < nb`, so `sd[i]` is a live entry C owns.
                    unsafe { (*self.sd.add(i)).read().type_.0 }
                })
                .collect();
            v.sort_unstable();
            v
        }
    }

    impl Drop for Array {
        fn drop(&mut self) {
            // SAFETY: frees the array and every entry, as C built them.
            unsafe { ffi::av_frame_side_data_free(&raw mut self.sd, &raw mut self.nb) }
        }
    }

    /// Whether Rust's reading of `t`'s descriptor keeps it under `mask`, as
    /// `av_frame_side_data_remove_by_props` decides: no descriptor, or no
    /// shared bit.
    fn kept(t: c_uint, mask: SideDataProps) -> bool {
        rust_entry(t).is_none_or(|d| !d.as_ref().props().intersects(mask))
    }

    /// One entry of every described type and three undescribed ones.
    fn full() -> (Array, Vec<c_uint>) {
        let mut a = Array::new();
        let mut model: Vec<c_uint> = (0..described()).chain([38, 1000, c_uint::MAX]).collect();
        for &t in &model {
            assert!(a.add(t), "type {t}");
        }
        model.sort_unstable();
        assert_eq!(a.types(), model);
        (a, model)
    }

    #[test]
    fn every_mask_removes_what_the_rust_props_intersect() {
        let masks = (0..=SideDataProps::all().bits())
            .map(SideDataProps::from_bits_retain)
            .chain([
                SideDataProps::from_bits_retain(0x20),
                SideDataProps::from_bits_retain(0x8000_0000),
                SideDataProps::from_bits_retain(c_uint::MAX),
                SideDataProps::MULTI | SideDataProps::from_bits_retain(0x4000),
            ]);
        for mask in masks {
            let (mut a, mut model) = full();
            a.remove_by_props(mask);
            model.retain(|&t| kept(t, mask));
            assert_eq!(a.types(), model, "{mask:?}");
        }
    }

    #[test]
    fn removal_sequences_match_after_each_step() {
        let steps = [
            SideDataProps::empty(),
            SideDataProps::from_bits_retain(0x40),
            SideDataProps::MULTI,
            SideDataProps::SIZE_DEPENDENT | SideDataProps::CHANNEL_DEPENDENT,
            SideDataProps::COLOR_DEPENDENT - SideDataProps::COLOR_DEPENDENT,
            SideDataProps::COLOR_DEPENDENT,
            SideDataProps::all() - SideDataProps::GLOBAL,
            SideDataProps::GLOBAL,
            SideDataProps::all(),
        ];
        let (mut a, mut model) = full();
        for mask in steps {
            a.remove_by_props(mask);
            model.retain(|&t| kept(t, mask));
            assert_eq!(a.types(), model, "{mask:?}");
            // Re-adding a removed described type and removing it again.
            if let Some(t) = (0..described()).find(|&t| !kept(t, mask)) {
                assert!(a.add(t));
                model.push(t);
                model.sort_unstable();
                assert_eq!(a.types(), model);
                a.remove_by_props(mask);
                model.retain(|&u| kept(u, mask));
                assert_eq!(a.types(), model, "{mask:?} again");
            }
        }
        // The full mask leaves only undescribed types and those without a
        // property, such as `AV_FRAME_DATA_A53_CC`.
        let bare: Vec<c_uint> = (0..described())
            .filter(|&t| rust_entry(t).unwrap().as_ref().props().is_empty())
            .chain([38, 1000, c_uint::MAX])
            .collect();
        assert!(bare.contains(&ffi::AVFrameSideDataType::AV_FRAME_DATA_A53_CC.0));
        assert_eq!(model, bare);
    }

    #[test]
    fn multi_decides_whether_c_appends_a_second_entry() {
        let mut multi = 0;
        for t in types() {
            let rust_multi =
                rust_entry(t).is_some_and(|d| d.as_ref().props().contains(SideDataProps::MULTI));
            multi += usize::from(rust_multi);
            let mut a = Array::new();
            assert!(a.add(t));
            assert_eq!(a.types(), [t]);
            // Without `AV_FRAME_SIDE_DATA_FLAG_REPLACE`, C refuses a second
            // entry of a single-instance type and keeps the first.
            assert_eq!(a.add(t), rust_multi, "type {t}");
            let expect = if rust_multi { vec![t, t] } else { vec![t] };
            assert_eq!(a.types(), expect, "type {t}");
            assert_eq!(a.add(t), rust_multi, "type {t}");
            assert_eq!(a.types().len(), if rust_multi { 3 } else { 1 });
        }
        assert_eq!(multi, 1);
    }
}

/// `SideDataType::descriptor` and `SideDataType::name` against
/// `av_frame_side_data_desc` and `av_frame_side_data_name` over every value
/// up to past the table's end and at the edges of `unsigned`: the same
/// entries, the same name pointers, the same properties (buffer-frame wave 1,
/// batch 1).
mod side_data_lookups {
    use core::ffi::CStr;

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::frame::SideDataType;

    /// Every value through one past the largest named type, plus the edges.
    fn values() -> impl Iterator<Item = u32> {
        (0..=64).chain([
            1000,
            i32::MAX as u32,
            i32::MAX as u32 + 1,
            u32::MAX - 1,
            u32::MAX,
        ])
    }

    #[test]
    fn descriptors_are_cs_table_entries() {
        let mut named = 0;
        for raw in values() {
            let t = ffi::AVFrameSideDataType(raw);
            // SAFETY: C takes any value and returns NULL or an entry of its
            // static table.
            let c = unsafe { ffi::av_frame_side_data_desc(t) };
            let rust = SideDataType::from_raw(t).descriptor();
            assert_eq!(rust.is_some(), !c.is_null(), "{raw}");
            let Some(d) = rust else { continue };
            named += 1;
            // SAFETY: a non-NULL entry of the static table, whose name is a
            // non-NULL string literal for every entry C returns.
            let (c_name, c_props) = unsafe { ((*c).name, (*c).props) };
            let name = d.name().expect("every returned entry is named");
            assert_eq!(name.as_ptr(), c_name, "{raw}: the same literal");
            assert_eq!(d.props().bits(), c_props, "{raw}");
            let copy = d.get();
            assert_eq!(copy.as_ref().name(), Some(name));
            assert_eq!(copy.into_raw().props, c_props);
        }
        assert!(named >= 38, "{named} named entries");
    }

    #[test]
    fn names_are_cs_strings() {
        for raw in values() {
            let t = ffi::AVFrameSideDataType(raw);
            // SAFETY: C takes any value and returns NULL or a literal.
            let c = unsafe { ffi::av_frame_side_data_name(t) };
            let rust = SideDataType::from_raw(t).name();
            assert_eq!(rust.map_or(core::ptr::null(), CStr::as_ptr), c, "{raw}");
            let desc_name = SideDataType::from_raw(t)
                .descriptor()
                .and_then(|d| d.name());
            assert_eq!(rust, desc_name, "{raw}");
        }
    }
}

/// `RegionOfInterest` against C's `AVRegionOfInterest` as C code builds and
/// reads it: `new` encodes as the designated initializer `vf_addroi` writes,
/// regions stored into `REGIONS_OF_INTEREST` side data from
/// `av_frame_side_data_new` are counted and read back as C's readers do
/// (payload size over the first `self_size`, stride `self_size`), setter
/// sequences match direct field writes after every step, and equality agrees
/// with `av_cmp_q` on the offsets (buffer-frame wave 1, batch 0).
mod region_of_interest {
    use core::mem::size_of;
    use core::ptr;

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::frame::{RegionOfInterest, RegionOfInterestRef};
    use ffmpeg::libavutil::rational::Rational;

    const EDGES: [i32; 7] = [i32::MIN, -100, -1, 0, 1, 1920, i32::MAX];

    fn xorshift(x: &mut u64) -> u64 {
        *x ^= *x << 13;
        *x ^= *x >> 7;
        *x ^= *x << 17;
        *x
    }

    fn edge(x: &mut u64) -> i32 {
        let r = xorshift(x);
        if r.is_multiple_of(3) {
            (r >> 32) as i32
        } else {
            EDGES[(r >> 8) as usize % EDGES.len()]
        }
    }

    fn q(num: i32, den: i32) -> Rational {
        Rational::from_raw(ffi::AVRational { num, den })
    }

    /// The struct's 28 bytes, which hold no padding.
    fn bytes(r: &ffi::AVRegionOfInterest) -> [u8; 28] {
        const { assert!(size_of::<ffi::AVRegionOfInterest>() == 28) };
        // SAFETY: an `AVRegionOfInterest` is 28 bytes of integers with no
        // padding, so every byte is initialised.
        unsafe { ptr::from_ref(r).cast::<[u8; 28]>().read() }
    }

    /// Every field as the Rust getters read it, in C's declaration order.
    fn rust_fields(r: RegionOfInterestRef<'_>) -> (u32, i32, i32, i32, i32, i32, i32) {
        let o = r.qoffset();
        (
            r.self_size(),
            r.top(),
            r.bottom(),
            r.left(),
            r.right(),
            o.num(),
            o.den(),
        )
    }

    fn c_fields(r: &ffi::AVRegionOfInterest) -> (u32, i32, i32, i32, i32, i32, i32) {
        (
            r.self_size,
            r.top,
            r.bottom,
            r.left,
            r.right,
            r.qoffset.num,
            r.qoffset.den,
        )
    }

    #[test]
    fn new_encodes_as_the_c_designated_initializer() {
        let mut x = 0x5eed_u64;
        for _ in 0..2000 {
            let f: [i32; 6] = core::array::from_fn(|_| edge(&mut x));
            let r = RegionOfInterest::new(f[0], f[1], f[2], f[3], q(f[4], f[5]));
            // `vf_addroi`: `(AVRegionOfInterest){ .self_size = sizeof(*roi), ... }`.
            let c = ffi::AVRegionOfInterest {
                self_size: size_of::<ffi::AVRegionOfInterest>() as u32,
                top: f[0],
                bottom: f[1],
                left: f[2],
                right: f[3],
                qoffset: ffi::AVRational {
                    num: f[4],
                    den: f[5],
                },
            };
            assert_eq!(bytes(&r.into_raw()), bytes(&c));
            assert_eq!(rust_fields(r.as_ref()), c_fields(&c));
            assert_eq!(
                rust_fields(RegionOfInterest::from_raw(c).as_ref()),
                c_fields(&c)
            );
        }
        let d = RegionOfInterest::default().into_raw();
        assert_eq!(
            c_fields(&d),
            (RegionOfInterest::SELF_SIZE, 0, 0, 0, 0, 0, 1)
        );
    }

    /// A side data array C built, freed with it.
    struct Array {
        sd: *mut *mut ffi::AVFrameSideData,
        nb: core::ffi::c_int,
    }

    impl Drop for Array {
        fn drop(&mut self) {
            // SAFETY: frees the array and every entry, as C built them.
            unsafe { ffi::av_frame_side_data_free(&raw mut self.sd, &raw mut self.nb) }
        }
    }

    #[test]
    fn side_data_arrays_count_and_read_as_c_readers_do() {
        let mut x = 0xa11ce_u64;
        for n in [1usize, 2, 7, 33] {
            let mut array = Array {
                sd: ptr::null_mut(),
                nb: 0,
            };
            let size = n * RegionOfInterest::SELF_SIZE as usize;
            // SAFETY: an empty array; C appends a fresh `size`-byte entry.
            let entry = unsafe {
                ffi::av_frame_side_data_new(
                    &raw mut array.sd,
                    &raw mut array.nb,
                    ffi::AVFrameSideDataType::AV_FRAME_DATA_REGIONS_OF_INTEREST,
                    size,
                    0,
                )
            };
            assert!(!entry.is_null());
            let regions: Vec<RegionOfInterest> = (0..n)
                .map(|_| {
                    let f: [i32; 6] = core::array::from_fn(|_| edge(&mut x));
                    RegionOfInterest::new(f[0], f[1], f[2], f[3], q(f[4], f[5]))
                })
                .collect();
            // SAFETY: the entry's payload holds `size` bytes, room for `n`
            // elements at stride `SELF_SIZE`, written unaligned as C's
            // readers read them.
            unsafe {
                let data = (*entry).data;
                assert_eq!((*entry).size, size);
                for (i, r) in regions.iter().enumerate() {
                    let at = data.add(i * RegionOfInterest::SELF_SIZE as usize);
                    at.cast::<ffi::AVRegionOfInterest>()
                        .write_unaligned(r.into_raw());
                }
                // What `libx264.c` and `vf_showinfo.c` compute.
                let first = data.cast::<ffi::AVRegionOfInterest>().read_unaligned();
                let stride = first.self_size as usize;
                assert_eq!((*entry).size / stride, n);
                for (i, r) in regions.iter().enumerate() {
                    let c = data
                        .add(i * stride)
                        .cast::<ffi::AVRegionOfInterest>()
                        .read_unaligned();
                    assert_eq!(c_fields(&c), rust_fields(r.as_ref()));
                    // C-written fields read back through the Rust getters.
                    let mut c = c;
                    c.top = c.top.wrapping_add(1);
                    c.qoffset.den = c.qoffset.num.wrapping_neg();
                    let back = RegionOfInterest::from_raw(c);
                    assert_eq!(rust_fields(back.as_ref()), c_fields(&c));
                }
            }
        }
    }

    #[test]
    fn setter_sequences_match_c_field_writes() {
        for seed in 1..=8u64 {
            let mut x = seed * 0x9e37_79b9;
            let mut rust = RegionOfInterest::default();
            let mut c = rust.into_raw();
            for step in 0..300 {
                let (v, w) = (edge(&mut x), edge(&mut x));
                let mut m = rust.as_mut();
                match xorshift(&mut x) % 9 {
                    0 => {
                        m.set_self_size(v as u32);
                        c.self_size = v as u32;
                    }
                    1 => {
                        m.set_top(v);
                        c.top = v;
                    }
                    2 => {
                        m.set_bottom(v);
                        c.bottom = v;
                    }
                    3 => {
                        m.set_left(v);
                        c.left = v;
                    }
                    4 => {
                        m.set_right(v);
                        c.right = v;
                    }
                    5 => {
                        m.set_qoffset(q(v, w));
                        c.qoffset = ffi::AVRational { num: v, den: w };
                    }
                    6 => {
                        m.qoffset_mut().set_num(v);
                        c.qoffset.num = v;
                    }
                    7 => {
                        m.qoffset_mut().set_den(w);
                        c.qoffset.den = w;
                    }
                    _ => {
                        let r = RegionOfInterest::new(v, w, w, v, q(w, v));
                        m.set(r);
                        c = ffi::AVRegionOfInterest {
                            self_size: 28,
                            top: v,
                            bottom: w,
                            left: w,
                            right: v,
                            qoffset: ffi::AVRational { num: w, den: v },
                        };
                    }
                }
                assert_eq!(
                    bytes(&rust.into_raw()),
                    bytes(&c),
                    "seed {seed} step {step}"
                );
                assert_eq!(
                    rust_fields(rust.as_ref()),
                    c_fields(&c),
                    "seed {seed} step {step}"
                );
                assert_eq!(bytes(&rust.as_ref().get().into_raw()), bytes(&c));
            }
        }
    }

    #[test]
    fn equality_agrees_with_av_cmp_q_on_offsets() {
        let mut x = 0xe0_u64;
        let small = |x: &mut u64| (xorshift(x) % 7) as i32 - 3;
        for _ in 0..5000 {
            let (a, b) = (
                ffi::AVRational {
                    num: small(&mut x),
                    den: small(&mut x),
                },
                ffi::AVRational {
                    num: small(&mut x),
                    den: small(&mut x),
                },
            );
            let edge_eq = !xorshift(&mut x).is_multiple_of(4);
            let ra = RegionOfInterest::new(1, 2, 3, 4, Rational::from_raw(a));
            let rb =
                RegionOfInterest::new(1, 2, 3, if edge_eq { 4 } else { 5 }, Rational::from_raw(b));
            // SAFETY: `av_cmp_q` takes any two rationals by value.
            let c_eq = edge_eq && unsafe { ffi::av_cmp_q(a, b) } == 0;
            assert_eq!(ra == rb, c_eq, "{a:?} {b:?} {edge_eq}");
        }
    }
}

/// [`SideData`] and [`SideDataArray`] against C's side data arrays, as C
/// observes them (buffer-frame wave 2, batch 0): a Rust array and a C array
/// driven through the same random sequences of additions and field writes,
/// compared after every step by C's own reading of both (the entries' fields,
/// payload bytes, metadata, buffer counts and writability), by C's
/// `av_frame_side_data_get_c` and `av_frame_side_data_clone` over the Rust
/// array, and by the buffers and dictionaries each side gave back, down to
/// what the arrays' release leaves of them.
///
/// [`SideData`]: ffmpeg::libavutil::frame::SideData
/// [`SideDataArray`]: ffmpeg::libavutil::frame::SideDataArray
mod side_data {
    use core::ffi::{CStr, c_int};
    use core::ptr;

    use ffmpeg::libavutil::buffer::{BufferReference, BufferReferenceBox};
    use ffmpeg::libavutil::dict::Dict;
    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::frame::{SideDataArray, SideDataFlags, SideDataRef, SideDataType};

    use super::side_data_fixture::{SAMPLE, Spec};

    /// What C sees of one entry.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub(super) struct Seen {
        pub(super) kind: u32,
        pub(super) bytes: Vec<u8>,
        pub(super) metadata: Pairs,
        count: c_int,
        writable: bool,
        window: bool,
    }

    /// C's reading of a dictionary, in iteration order.
    pub(super) fn c_pairs(m: *const ffi::AVDictionary) -> Pairs {
        let mut out = Vec::new();
        let mut e = ptr::null();
        loop {
            // SAFETY: `m` is NULL or a live dictionary; `e` is NULL or its
            // previous entry.
            e = unsafe { ffi::av_dict_iterate(m, e) };
            if e.is_null() {
                return out;
            }
            // SAFETY: a live entry, whose key and value are C strings.
            let (k, v) = unsafe { (CStr::from_ptr((*e).key), CStr::from_ptr((*e).value)) };
            out.push((k.to_bytes().into(), v.to_bytes().into()));
        }
    }

    /// C's reading of every entry of the array `sd`/`nb`.
    ///
    /// # Safety
    ///
    /// `sd`/`nb` must be a live array of live entries.
    unsafe fn c_seen(sd: *const *mut ffi::AVFrameSideData, nb: c_int) -> Vec<Seen> {
        (0..nb as usize)
            .map(|i| {
                // SAFETY: entry `i` of the caller's array, and its buffer.
                unsafe {
                    let e = *sd.add(i);
                    let (buf, data, size) = ((*e).buf, (*e).data, (*e).size);
                    Seen {
                        kind: (*e).type_.0,
                        bytes: if size == 0 {
                            Vec::new()
                        } else {
                            std::slice::from_raw_parts(data, size).to_vec()
                        },
                        metadata: c_pairs((*e).metadata),
                        count: ffi::av_buffer_get_ref_count(buf),
                        writable: ffi::av_buffer_is_writable(buf) != 0,
                        window: data == (*buf).data && size == (*buf).size,
                    }
                }
            })
            .collect()
    }

    /// C's reading of the Rust array, which it lends C for the call.
    pub(super) fn rust_seen(a: &mut SideDataArray) -> Vec<Seen> {
        let (sd, nb) = core::mem::take(a).into_raw_parts();
        // SAFETY: the array Rust held, live until it is taken back below.
        let seen = unsafe { c_seen(sd, nb) };
        // SAFETY: the same array, unchanged, given back.
        *a = unsafe { SideDataArray::from_raw_parts(sd, nb) };
        seen
    }

    /// Key/value pairs as bytes, in iteration order.
    pub(super) type Pairs = Vec<(Vec<u8>, Vec<u8>)>;

    /// The Rust getters' reading of an entry, in [`Seen`]'s terms where the
    /// safe API has them.
    pub(super) fn getters(e: SideDataRef<'_>) -> (u32, Vec<u8>, Pairs) {
        let metadata = e.metadata().map_or_else(Vec::new, |m| {
            m.iter()
                .map(|p| (p.key().to_bytes().into(), p.value().to_bytes().into()))
                .collect()
        });
        (e.kind().into_raw().0, e.data().to_vec(), metadata)
    }

    /// An `AVFrameSideData *` array C grows and writes, freed on drop.
    pub(super) struct CArray {
        pub(super) sd: *mut *mut ffi::AVFrameSideData,
        pub(super) nb: c_int,
    }

    impl CArray {
        pub(super) fn new() -> Self {
            Self {
                sd: ptr::null_mut(),
                nb: 0,
            }
        }

        pub(super) fn entry(&self, i: usize) -> *mut ffi::AVFrameSideData {
            assert!(i < self.nb as usize);
            // SAFETY: `i < nb`.
            unsafe { *self.sd.add(i) }
        }

        pub(super) fn seen(&self) -> Vec<Seen> {
            // SAFETY: the array C built.
            unsafe { c_seen(self.sd, self.nb) }
        }

        /// `av_frame_side_data_add` of `buf`, which C takes on success;
        /// whether it added, and the reference it left the caller.
        pub(super) fn add(
            &mut self,
            kind: u32,
            buf: *mut ffi::AVBufferRef,
            flags: SideDataFlags,
        ) -> (bool, *mut ffi::AVBufferRef) {
            let mut slot = buf;
            // SAFETY: the array C built and a live reference.
            let e = unsafe {
                ffi::av_frame_side_data_add(
                    &raw mut self.sd,
                    &raw mut self.nb,
                    ffi::AVFrameSideDataType(kind),
                    &mut slot,
                    flags.bits(),
                )
            };
            (!e.is_null(), slot)
        }

        /// The field writes C callers make to replace entry `i`'s buffer,
        /// as `replace_side_data_from_buf` does without its metadata free;
        /// the old reference is the caller's.
        fn replace_buffer(
            &mut self,
            i: usize,
            buf: *mut ffi::AVBufferRef,
        ) -> *mut ffi::AVBufferRef {
            let e = self.entry(i);
            // SAFETY: a live entry and a live reference it takes over.
            unsafe {
                let old = (*e).buf;
                (*e).buf = buf;
                (*e).data = (*buf).data;
                (*e).size = (*buf).size;
                old
            }
        }

        pub(super) fn replace_metadata(
            &mut self,
            i: usize,
            m: *mut ffi::AVDictionary,
        ) -> *mut ffi::AVDictionary {
            let e = self.entry(i);
            // SAFETY: a live entry, which takes over `m`.
            unsafe { core::mem::replace(&mut (*e).metadata, m) }
        }

        fn set_kind(&mut self, i: usize, kind: u32) {
            // SAFETY: a live entry.
            unsafe { (*self.entry(i)).type_ = ffi::AVFrameSideDataType(kind) }
        }

        /// Writes `byte` over entry `i`'s payload when its buffer is
        /// writable; whether it was.
        fn write(&mut self, i: usize, byte: u8) -> bool {
            let e = self.entry(i);
            // SAFETY: a live entry; its payload is written only when its
            // buffer is sole and not read-only.
            unsafe {
                let ok = ffi::av_buffer_is_writable((*e).buf) != 0;
                if ok {
                    ptr::write_bytes((*e).data, byte, (*e).size);
                }
                ok
            }
        }
    }

    impl Drop for CArray {
        fn drop(&mut self) {
            // SAFETY: frees the array and every entry, as C built them.
            unsafe { ffi::av_frame_side_data_free(&raw mut self.sd, &raw mut self.nb) }
        }
    }

    /// A C reference holding `bytes`.
    pub(super) fn c_buffer(bytes: &[u8]) -> *mut ffi::AVBufferRef {
        // SAFETY: a fresh reference to `len` bytes, filled from `bytes`.
        unsafe {
            let b = ffi::av_buffer_alloc(bytes.len());
            assert!(!b.is_null());
            ptr::copy_nonoverlapping(bytes.as_ptr(), (*b).data, bytes.len());
            b
        }
    }

    /// A C dictionary of `pairs`, inserted as `Dict::from_entries` does.
    pub(super) fn c_dict(pairs: &[(&CStr, &CStr)]) -> *mut ffi::AVDictionary {
        let mut m = ptr::null_mut();
        for (k, v) in pairs {
            // SAFETY: C copies the strings into `m`.
            let r = unsafe {
                ffi::av_dict_set(
                    &mut m,
                    k.as_ptr(),
                    v.as_ptr(),
                    ffi::AV_DICT_MULTIKEY as c_int,
                )
            };
            assert!(r >= 0);
        }
        m
    }

    /// C's reading of a reference the caller holds: bytes and count.
    fn c_buf_seen(b: *const ffi::AVBufferRef) -> (Vec<u8>, c_int) {
        // SAFETY: a live reference.
        unsafe {
            let bytes = if (*b).size == 0 {
                Vec::new()
            } else {
                std::slice::from_raw_parts((*b).data, (*b).size).to_vec()
            };
            (bytes, ffi::av_buffer_get_ref_count(b))
        }
    }

    fn rust_buf_seen(b: &BufferReferenceBox) -> (Vec<u8>, c_int) {
        c_buf_seen(b.as_c_ptr())
    }

    /// Asserts both arrays read alike, through C and through the getters,
    /// and that C's lookups and clones of the Rust array match its own.
    pub(super) fn compare(r: &mut SideDataArray, c: &CArray, step: &str) {
        let (rs, cs) = (rust_seen(r), c.seen());
        assert_eq!(rs, cs, "{step}");
        assert_eq!(r.len(), cs.len(), "{step}");
        for (e, s) in r.as_ref().iter().zip(&cs) {
            assert_eq!(
                getters(e),
                (s.kind, s.bytes.clone(), s.metadata.clone()),
                "{step}"
            );
        }
        let (sd, nb) = core::mem::take(r).into_raw_parts();
        for kind in KINDS {
            // SAFETY: the two live arrays, read for the call.
            let (rf, cf) = unsafe {
                (
                    ffi::av_frame_side_data_get_c(sd.cast(), nb, ffi::AVFrameSideDataType(kind)),
                    ffi::av_frame_side_data_get_c(
                        c.sd.cast(),
                        c.nb,
                        ffi::AVFrameSideDataType(kind),
                    ),
                )
            };
            let index =
                |p: *const ffi::AVFrameSideData, sd: *mut *mut ffi::AVFrameSideData, nb: c_int| {
                    // SAFETY: `i < nb` entries of a live array.
                    (0..nb as usize).position(|i| ptr::eq(unsafe { *sd.add(i) }, p))
                };
            assert_eq!(
                index(rf, sd, nb),
                index(cf, c.sd, c.nb),
                "{step}: get_c({kind})"
            );
        }
        let (mut rc, mut cc) = (CArray::new(), CArray::new());
        for i in 0..nb as usize {
            // SAFETY: entry `i` of each live array, cloned by C into a fresh
            // array; a non-MULTI duplicate is refused alike.
            let (x, y) = unsafe {
                (
                    ffi::av_frame_side_data_clone(&raw mut rc.sd, &raw mut rc.nb, *sd.add(i), 0),
                    ffi::av_frame_side_data_clone(&raw mut cc.sd, &raw mut cc.nb, *c.sd.add(i), 0),
                )
            };
            assert_eq!(x, y, "{step}: clone {i}");
        }
        assert_eq!(rc.seen(), cc.seen(), "{step}: clones");
        drop((rc, cc));
        // SAFETY: the Rust array, unchanged, given back.
        *r = unsafe { SideDataArray::from_raw_parts(sd, nb) };
    }

    /// The kinds the sequences use: `MULTI`, two unique named ones, and one
    /// C does not name.
    pub(super) const KINDS: [u32; 4] = [
        SideDataType::SEI_UNREGISTERED.into_raw().0,
        SideDataType::AFD.into_raw().0,
        SideDataType::ICC_PROFILE.into_raw().0,
        1000,
    ];

    pub(super) fn xorshift(x: &mut u64) -> u64 {
        *x ^= *x << 13;
        *x ^= *x >> 7;
        *x ^= *x << 17;
        *x
    }

    pub(super) const FLAGS: [SideDataFlags; 4] = [
        SideDataFlags::NONE,
        SideDataFlags::UNIQUE,
        SideDataFlags::REPLACE,
        SideDataFlags::REPLACE,
    ];

    pub(super) const PAIRS: [&[(&CStr, &CStr)]; 4] = [
        &[],
        &[(c"k", c"v")],
        &[(c"a", c"1"), (c"b", c"2")],
        &[(c"dup", c"1"), (c"dup", c"2"), (c"z", c"")],
    ];

    /// Random sequences of every operation, compared after each step.
    fn run(seed: u64, steps: usize) {
        let mut x = seed;
        let (mut r, mut c) = (SideDataArray::new(), CArray::new());
        // Buffers each side kept: given back by an addition or a
        // replacement, or a second reference taken to share an entry's.
        let (mut r_kept, mut c_kept): (Vec<BufferReferenceBox>, Vec<*mut ffi::AVBufferRef>) =
            (Vec::new(), Vec::new());
        let mut r_dicts: Vec<Dict> = Vec::new();
        let mut c_dicts: Vec<*mut ffi::AVDictionary> = Vec::new();
        for step in 0..steps {
            let v = xorshift(&mut x);
            let len = c.nb as usize;
            let i = (v >> 8) as usize % len.max(1);
            let payload: Vec<u8> = (0..(v >> 20) % 70).map(|k| (k as u8) ^ (v as u8)).collect();
            let what = if len == 0 { 0 } else { v % 8 };
            let label = format!("seed {seed} step {step} op {what}");
            match what {
                0 => {
                    let kind = KINDS[(v >> 4) as usize % KINDS.len()];
                    let flags = FLAGS[(v >> 6) as usize % FLAGS.len()];
                    let rb = BufferReference::alloc(payload.len())
                        .unwrap()
                        .copy_from_slice(&payload);
                    let kind_r = SideDataType::from_raw(ffi::AVFrameSideDataType(kind));
                    let ra = r.add(kind_r, rb, flags).map(drop);
                    let (ca, cb) = c.add(kind, c_buffer(&payload), flags);
                    assert_eq!(ra.is_ok(), ca, "{label}");
                    match ra {
                        Ok(_) => assert!(cb.is_null()),
                        Err(back) => {
                            assert_eq!(rust_buf_seen(&back), c_buf_seen(cb), "{label}");
                            r_kept.push(back);
                            c_kept.push(cb);
                        }
                    }
                }
                1 => {
                    let kind = KINDS[(v >> 4) as usize % KINDS.len()] ^ ((v >> 40) as u32 & 1);
                    r.as_mut()
                        .get_mut(i)
                        .unwrap()
                        .set_kind(SideDataType::from_raw(ffi::AVFrameSideDataType(kind)));
                    c.set_kind(i, kind);
                }
                2 => {
                    let rb = BufferReference::alloc(payload.len())
                        .unwrap()
                        .copy_from_slice(&payload);
                    let old = r.as_mut().get_mut(i).unwrap().replace_buffer(rb);
                    let cold = c.replace_buffer(i, c_buffer(&payload));
                    assert_eq!(rust_buf_seen(&old), c_buf_seen(cold), "{label}");
                    r_kept.push(old);
                    c_kept.push(cold);
                }
                3 => {
                    let rb = BufferReference::alloc(payload.len())
                        .unwrap()
                        .copy_from_slice(&payload);
                    r.as_mut().get_mut(i).unwrap().set_buffer(rb);
                    let cold = c.replace_buffer(i, c_buffer(&payload));
                    // SAFETY: the old reference, which the caller now holds.
                    unsafe { ffi::av_buffer_unref(&mut { cold }) };
                }
                4 => {
                    let pairs = PAIRS[(v >> 4) as usize % PAIRS.len()];
                    let old = r
                        .as_mut()
                        .get_mut(i)
                        .unwrap()
                        .replace_metadata(Dict::from_entries(pairs.iter().copied()).unwrap());
                    let cold = c.replace_metadata(i, c_dict(pairs));
                    let old_pairs = old
                        .iter()
                        .map(|p| (p.key().to_bytes().to_vec(), p.value().to_bytes().to_vec()))
                        .collect::<Vec<_>>();
                    assert_eq!(old_pairs, c_pairs(cold), "{label}");
                    r_dicts.push(old);
                    c_dicts.push(cold);
                }
                5 => {
                    let taken = r.as_mut().get_mut(i).unwrap().take_metadata();
                    let cold = c.replace_metadata(i, ptr::null_mut());
                    assert_eq!(taken.len(), c_pairs(cold).len(), "{label}");
                    r_dicts.push(taken);
                    c_dicts.push(cold);
                }
                6 => {
                    let byte = (v >> 30) as u8;
                    let mut entries = r.as_mut();
                    let mut e = entries.get_mut(i).unwrap();
                    let wrote = match e.data_mut() {
                        Some(bytes) => {
                            bytes.fill(byte);
                            true
                        }
                        None => false,
                    };
                    assert_eq!(wrote, c.write(i, byte), "{label}");
                }
                _ => {
                    let copy = r.as_ref().get(i).unwrap().buffer().try_clone().unwrap();
                    // SAFETY: a live entry's reference, to which C adds one.
                    let ccopy = unsafe { ffi::av_buffer_ref((*c.entry(i)).buf) };
                    assert_eq!(rust_buf_seen(&copy), c_buf_seen(ccopy), "{label}");
                    r_kept.push(copy);
                    c_kept.push(ccopy);
                }
            }
            compare(&mut r, &c, &label);
            if step % 16 == 15 && !r_kept.is_empty() {
                // Release the oldest kept reference on both sides, which may
                // make an entry writable again.
                drop(r_kept.remove(0));
                let mut b = c_kept.remove(0);
                // SAFETY: the kept reference, released once.
                unsafe { ffi::av_buffer_unref(&mut b) };
                compare(&mut r, &c, &format!("{label} release"));
            }
        }
        // What the arrays' release leaves of the references both sides kept.
        drop(r);
        drop(c);
        for (rb, cb) in r_kept.iter().zip(&c_kept) {
            assert_eq!(
                rust_buf_seen(rb),
                c_buf_seen(*cb),
                "seed {seed}: kept after release"
            );
        }
        for mut b in c_kept {
            // SAFETY: each kept reference, released once.
            unsafe { ffi::av_buffer_unref(&mut b) };
        }
        for mut m in c_dicts {
            // SAFETY: each kept dictionary, freed once.
            unsafe { ffi::av_dict_free(&mut m) };
        }
        drop(r_dicts);
    }

    #[test]
    fn random_sequences_match_c_after_every_step() {
        for seed in 1..=40u64 {
            run(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15), 120);
        }
    }

    /// The sample entries built through the safe API read, look up and clone
    /// as the same entries C builds and fills.
    #[test]
    fn safely_built_entries_read_as_c_built_ones() {
        let mut r = super::side_data_fixture::sample();
        let mut c = CArray::new();
        for spec in &SAMPLE {
            let (ok, slot) = c.add(
                spec.kind.into_raw().0,
                c_buffer(spec.payload),
                SideDataFlags::NONE,
            );
            assert!(ok && slot.is_null());
            let n = c.nb as usize - 1;
            let _ = c.replace_metadata(n, c_dict(spec.metadata));
        }
        compare(&mut r, &c, "sample");
        let specs: Vec<Spec<'_>> = SAMPLE.iter().rev().copied().collect();
        let mut r = super::side_data_fixture::array(&specs);
        let mut c = CArray::new();
        for spec in &specs {
            let (ok, _) = c.add(
                spec.kind.into_raw().0,
                c_buffer(spec.payload),
                SideDataFlags::NONE,
            );
            assert!(ok);
            let n = c.nb as usize - 1;
            let _ = c.replace_metadata(n, c_dict(spec.metadata));
        }
        compare(&mut r, &c, "reversed sample");
    }

    /// C's own adders, given the Rust array, change it as they change C's,
    /// and its release frees what they left.
    #[test]
    fn c_adders_on_the_rust_array_match_c_arrays() {
        let mut r = super::side_data_fixture::sample();
        let mut c = CArray::new();
        for spec in &SAMPLE {
            c.add(
                spec.kind.into_raw().0,
                c_buffer(spec.payload),
                SideDataFlags::NONE,
            );
            let n = c.nb as usize - 1;
            let _ = c.replace_metadata(n, c_dict(spec.metadata));
        }
        for (k, flags) in [(0u32, 0), (8, 2), (8, 1), (13, 3), (36, 0), (1000, 2)] {
            let (mut sd, mut nb) = core::mem::take(&mut r).into_raw_parts();
            // SAFETY: the Rust array, lent to C for the call, and a fresh
            // C buffer of 5 bytes C fills nothing in.
            let e = unsafe {
                ffi::av_frame_side_data_new(&mut sd, &mut nb, ffi::AVFrameSideDataType(k), 5, flags)
            };
            if !e.is_null() {
                // SAFETY: the entry C returned, whose fresh payload is written
                // in full before the array goes back to Rust.
                unsafe { ptr::write_bytes((*e).data, k as u8, 5) };
            }
            // SAFETY: the array C changed, every payload initialised.
            r = unsafe { SideDataArray::from_raw_parts(sd, nb) };
            // SAFETY: as above, on C's own array.
            let ce = unsafe {
                ffi::av_frame_side_data_new(
                    &raw mut c.sd,
                    &raw mut c.nb,
                    ffi::AVFrameSideDataType(k),
                    5,
                    flags,
                )
            };
            assert_eq!(e.is_null(), ce.is_null(), "new({k}, {flags})");
            if !ce.is_null() {
                // SAFETY: as above.
                unsafe { ptr::write_bytes((*ce).data, k as u8, 5) };
            }
            compare(&mut r, &c, &format!("new({k}, {flags})"));
        }
        // C's clones of a third array's entries, which write every field of
        // an entry they replace, into both.
        let mut src = CArray::new();
        for (k, pairs) in [
            (8u32, PAIRS[1]),
            (13, PAIRS[2]),
            (20, PAIRS[3]),
            (1000, PAIRS[0]),
        ] {
            let (ok, _) = src.add(k, c_buffer(&[k as u8; 9]), SideDataFlags::NONE);
            assert!(ok);
            let n = src.nb as usize - 1;
            let _ = src.replace_metadata(n, c_dict(pairs));
        }
        for flags in [0, 2, 1, 3, 2] {
            for i in 0..src.nb as usize {
                let (mut sd, mut nb) = core::mem::take(&mut r).into_raw_parts();
                let entry = src.entry(i);
                // SAFETY: the Rust array, lent to C for the call, and a live
                // source entry, whose buffer C shares and metadata it copies.
                let x = unsafe { ffi::av_frame_side_data_clone(&mut sd, &mut nb, entry, flags) };
                // SAFETY: the array C changed, its entries' fields copied
                // from initialised entries.
                r = unsafe { SideDataArray::from_raw_parts(sd, nb) };
                // SAFETY: as above, on C's own array.
                let y = unsafe {
                    ffi::av_frame_side_data_clone(&raw mut c.sd, &raw mut c.nb, src.entry(i), flags)
                };
                assert_eq!(x, y, "clone({i}, {flags})");
                compare(&mut r, &c, &format!("clone({i}, {flags})"));
            }
        }
        drop(src);
        compare(&mut r, &c, "source freed");
    }
}

/// Side data arrays built through the safe API for the `side_data` tests.
#[path = "../common/side_data.rs"]
mod side_data_fixture;

/// The side data array routines against C's (buffer-frame wave 3, batch 1):
/// `SideDataArrayRef::find` (`av_frame_side_data_get_c`),
/// `SideDataArrayMut::find_mut` (`av_frame_side_data_get`), `remove`,
/// `remove_by_props`, `add_clone` (`av_frame_side_data_clone`) and
/// `alloc_zeroed` / `alloc_from_slice` (`av_frame_side_data_new`, then a
/// fill), with the owner's twins, on a Rust array and a C array driven
/// through the same random sequences, compared after every step by the
/// `side_data` suite's `compare`: C's reading of both arrays, their getters,
/// C's lookups and clones of them. Clones come from a Rust and a C source
/// array, compared too, and replaced halfway so that clones outlive them.
mod side_data_array_routines {
    use core::ffi::c_int;
    use core::ptr;

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::frame::{SideDataArray, SideDataFlags, SideDataProps, SideDataType};

    use super::side_data::{
        CArray, FLAGS, KINDS, PAIRS, c_buffer, c_dict, compare, getters, rust_seen, xorshift,
    };
    use super::side_data_fixture::{SAMPLE, array};

    /// Property filters: none, each named one, combinations, a bit no
    /// descriptor has, and every bit (C's `int` -1).
    const PROPS: [u32; 8] = [
        0,
        SideDataProps::GLOBAL.bits(),
        SideDataProps::MULTI.bits(),
        SideDataProps::SIZE_DEPENDENT.bits() | SideDataProps::COLOR_DEPENDENT.bits(),
        SideDataProps::CHANNEL_DEPENDENT.bits(),
        SideDataProps::all().bits(),
        1 << 30,
        u32::MAX,
    ];

    fn kind(raw: u32) -> SideDataType {
        SideDataType::from_raw(ffi::AVFrameSideDataType(raw))
    }

    /// The index of C's entry `e` in `c`.
    fn index(c: &CArray, e: *const ffi::AVFrameSideData) -> Option<usize> {
        (0..c.nb as usize).find(|&i| ptr::eq(c.entry(i), e))
    }

    /// The source arrays clones come from: [`SAMPLE`] built through the
    /// safe API, and the same entries built by C.
    fn sources() -> (SideDataArray, CArray) {
        let mut c = CArray::new();
        for spec in &SAMPLE {
            let (ok, _) = c.add(
                spec.kind.into_raw().0,
                c_buffer(spec.payload),
                SideDataFlags::NONE,
            );
            assert!(ok);
            let n = c.nb as usize - 1;
            let _ = c.replace_metadata(n, c_dict(spec.metadata));
        }
        (array(&SAMPLE), c)
    }

    /// `av_frame_side_data_new` on C's array, with its payload then written
    /// as `fill` says, as the wrappers do.
    fn c_new(
        c: &mut CArray,
        k: u32,
        size: usize,
        flags: SideDataFlags,
        fill: Option<&[u8]>,
    ) -> Option<usize> {
        // SAFETY: the array C built; the payload of an entry C returns is
        // written in full before anything reads it.
        let e = unsafe {
            let e = ffi::av_frame_side_data_new(
                &raw mut c.sd,
                &raw mut c.nb,
                ffi::AVFrameSideDataType(k),
                size,
                flags.bits(),
            );
            if !e.is_null() {
                match fill {
                    Some(bytes) => ptr::copy_nonoverlapping(bytes.as_ptr(), (*e).data, size),
                    None => ptr::write_bytes((*e).data, 0, size),
                }
            }
            e
        };
        (!e.is_null()).then(|| index(c, e).expect("C returned an entry of the array"))
    }

    /// Random sequences of every routine, compared after each step.
    fn run(seed: u64, steps: usize) {
        let mut x = seed;
        let (mut r, mut c) = (SideDataArray::new(), CArray::new());
        let (mut r_src, mut c_src) = sources();
        for step in 0..steps {
            let v = xorshift(&mut x);
            let k = KINDS[(v >> 4) as usize % KINDS.len()];
            let flags = FLAGS[(v >> 6) as usize % FLAGS.len()];
            let flags = if v >> 50 & 1 == 1 {
                flags | SideDataFlags::UNIQUE
            } else {
                flags
            };
            let owner = v >> 51 & 1 == 1;
            let payload: Vec<u8> = (0..(v >> 20) % 70).map(|n| (n as u8) ^ (v as u8)).collect();
            let what = v % 8;
            let label = format!("seed {seed} step {step} op {what} kind {k} flags {flags:?}");
            match what {
                0 | 1 => {
                    let fill = (what == 1).then_some(&payload[..]);
                    let got = match (fill, owner) {
                        (None, true) => r
                            .alloc_zeroed(kind(k), payload.len(), flags)
                            .map(|e| getters(e.as_ref())),
                        (None, false) => r
                            .as_mut()
                            .alloc_zeroed(kind(k), payload.len(), flags)
                            .map(|e| getters(e.as_ref())),
                        (Some(b), true) => r
                            .alloc_from_slice(kind(k), b, flags)
                            .map(|e| getters(e.as_ref())),
                        (Some(b), false) => r
                            .as_mut()
                            .alloc_from_slice(kind(k), b, flags)
                            .map(|e| getters(e.as_ref())),
                    };
                    let want = c_new(&mut c, k, payload.len(), flags, fill).map(|i| {
                        let s = c.seen().swap_remove(i);
                        (s.kind, s.bytes, s.metadata)
                    });
                    assert_eq!(got, want, "{label}");
                }
                2 => {
                    let j = (v >> 12) as usize % SAMPLE.len();
                    let got = r.as_mut().add_clone(r_src.as_ref().get(j).unwrap(), flags);
                    // SAFETY: C's array and a live entry of C's source.
                    let want = unsafe {
                        ffi::av_frame_side_data_clone(
                            &raw mut c.sd,
                            &raw mut c.nb,
                            c_src.entry(j),
                            flags.bits(),
                        )
                    };
                    assert_eq!(got, if want < 0 { Err(want) } else { Ok(()) }, "{label}");
                }
                3 => {
                    r.as_mut().remove(kind(k));
                    // SAFETY: C's array.
                    unsafe {
                        ffi::av_frame_side_data_remove(
                            &raw mut c.sd,
                            &raw mut c.nb,
                            ffi::AVFrameSideDataType(k),
                        )
                    };
                }
                4 => {
                    let props = PROPS[(v >> 12) as usize % PROPS.len()];
                    r.as_mut()
                        .remove_by_props(SideDataProps::from_bits_retain(props));
                    // SAFETY: C's array.
                    unsafe {
                        ffi::av_frame_side_data_remove_by_props(
                            &raw mut c.sd,
                            &raw mut c.nb,
                            props as c_int,
                        )
                    };
                }
                5 => {
                    let got = r.as_ref().find(kind(k)).map(getters);
                    // SAFETY: C's array, read for the call.
                    let e = unsafe {
                        ffi::av_frame_side_data_get_c(
                            c.sd.cast(),
                            c.nb,
                            ffi::AVFrameSideDataType(k),
                        )
                    };
                    let want = (!e.is_null()).then(|| {
                        let s = c.seen().swap_remove(index(&c, e).unwrap());
                        (s.kind, s.bytes, s.metadata)
                    });
                    assert_eq!(got, want, "{label}");
                }
                6 => {
                    // Marks the entry found with another kind, which tells
                    // in the comparison which one it was.
                    let mark = KINDS[(v >> 12) as usize % KINDS.len()] ^ 2;
                    let found = match owner {
                        true => r.find_mut(kind(k)).map(|mut e| e.set_kind(kind(mark))),
                        false => r
                            .as_mut()
                            .find_mut(kind(k))
                            .map(|mut e| e.set_kind(kind(mark))),
                    };
                    // SAFETY: C's array; the entry C returns is one of its
                    // own, written as the array's holder may.
                    let e = unsafe {
                        ffi::av_frame_side_data_get(c.sd, c.nb, ffi::AVFrameSideDataType(k))
                    };
                    if !e.is_null() {
                        // SAFETY: as above.
                        unsafe { (*e.cast_mut()).type_ = ffi::AVFrameSideDataType(mark) };
                    }
                    assert_eq!(found.is_some(), !e.is_null(), "{label}");
                }
                _ => {
                    let byte = (v >> 30) as u8;
                    let wrote = r
                        .as_mut()
                        .find_mut(kind(k))
                        .and_then(|mut e| e.data_mut().map(|d| d.fill(byte)))
                        .is_some();
                    // SAFETY: C's array; a payload is written only when its
                    // buffer is sole and not read-only.
                    let c_wrote = unsafe {
                        let e =
                            ffi::av_frame_side_data_get(c.sd, c.nb, ffi::AVFrameSideDataType(k));
                        let ok = !e.is_null() && ffi::av_buffer_is_writable((*e).buf) != 0;
                        if ok {
                            ptr::write_bytes((*e).data, byte, (*e).size);
                        }
                        ok
                    };
                    assert_eq!(wrote, c_wrote, "{label}");
                }
            }
            compare(&mut r, &c, &label);
            assert_eq!(rust_seen(&mut r_src), c_src.seen(), "{label}: sources");
            if step == steps / 2 {
                // Clones outlive their sources, whose buffers they share.
                (r_src, c_src) = sources();
                compare(&mut r, &c, &format!("{label}: sources replaced"));
            }
        }
    }

    #[test]
    fn random_sequences_match_c_after_every_step() {
        for seed in 1..=40u64 {
            run(seed.wrapping_mul(0x2545_f491_4f6c_dd1d), 120);
        }
    }

    /// Whether this is the isolated child of test `name`; in the parent, runs
    /// the child, asserts it passed, and returns `false`.
    fn isolated(name: &str) -> bool {
        const ISOLATED: &str = "CRUSTIFY_EQUIV_ISOLATED";
        if std::env::var_os(ISOLATED).is_some() {
            return true;
        }
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", name, "--test-threads=1", "--nocapture"])
            .env(ISOLATED, "1")
            .status()
            .unwrap();
        assert!(status.success(), "isolated run failed: {status}");
        false
    }

    /// Clones and allocations failing at `av_max_alloc` limits, in an
    /// isolated child as the limit is process-wide: each limit fails a
    /// different allocation of C's clone (the new reference, the pointer
    /// block, the entry, the metadata copy, whose failure removes the entry
    /// again) and of its replacement (the metadata copy, the reference), and
    /// the payload of an allocation. Both sides fail alike, with the arrays
    /// compared once the limit is lifted.
    #[test]
    fn failures_at_the_av_max_alloc_limit_match_c() {
        let name = "frame::side_data_array_routines::failures_at_the_av_max_alloc_limit_match_c";
        if !isolated(name) {
            return;
        }
        let set_max = ffmpeg::libavutil::mem::set_max_alloc;
        let key = std::ffi::CString::new(vec![b'k'; 150]).unwrap();
        let long: &[(&core::ffi::CStr, &core::ffi::CStr)] = &[(&key, c"v")];
        // Source entries: a unique kind and a MULTI one, each with and
        // without metadata whose key C copies in one 151-byte block.
        let specs = [
            (SideDataType::AFD, long),
            (SideDataType::AFD, &[][..]),
            (SideDataType::SEI_UNREGISTERED, long),
            (SideDataType::SEI_UNREGISTERED, &[][..]),
        ];
        for limit in [0, 16, 23, 24, 32, 39, 40, 47, 48, 64, 100, 150, 151, 200] {
            for (j, &(k, pairs)) in specs.iter().enumerate() {
                for flags in [SideDataFlags::NONE, SideDataFlags::REPLACE] {
                    let label = format!("limit {limit} source {j} flags {flags:?}");
                    let (mut r_src, mut c_src) = (SideDataArray::new(), CArray::new());
                    let mut e = r_src
                        .alloc_from_slice(k, b"src", SideDataFlags::NONE)
                        .unwrap();
                    e.set_metadata(
                        ffmpeg::libavutil::dict::Dict::from_entries(pairs.iter().copied()).unwrap(),
                    );
                    let i = c_new(
                        &mut c_src,
                        k.into_raw().0,
                        3,
                        SideDataFlags::NONE,
                        Some(b"src"),
                    )
                    .unwrap();
                    let _ = c_src.replace_metadata(i, c_dict(pairs));
                    // Destinations holding an entry of each kind already.
                    let (mut r, mut c) = (SideDataArray::new(), CArray::new());
                    for k2 in [SideDataType::AFD, SideDataType::SEI_UNREGISTERED] {
                        r.alloc_from_slice(k2, b"old", SideDataFlags::NONE).unwrap();
                        c_new(
                            &mut c,
                            k2.into_raw().0,
                            3,
                            SideDataFlags::NONE,
                            Some(b"old"),
                        )
                        .unwrap();
                    }
                    set_max(limit);
                    let got = r.as_mut().add_clone(r_src.as_ref().get(0).unwrap(), flags);
                    // SAFETY: C's array and a live entry of C's source.
                    let want = unsafe {
                        ffi::av_frame_side_data_clone(
                            &raw mut c.sd,
                            &raw mut c.nb,
                            c_src.entry(0),
                            flags.bits(),
                        )
                    };
                    set_max(i32::MAX as usize);
                    assert_eq!(
                        got,
                        if want < 0 { Err(want) } else { Ok(()) },
                        "{label}: clone"
                    );
                    compare(&mut r, &c, &format!("{label}: clone"));
                    // An allocation with no entry to replace, which C
                    // reaches itself.
                    let size = limit / 2 + 8;
                    set_max(limit);
                    let got = r
                        .alloc_zeroed(SideDataType::ICC_PROFILE, size, SideDataFlags::NONE)
                        .is_some();
                    let want = c_new(
                        &mut c,
                        SideDataType::ICC_PROFILE.into_raw().0,
                        size,
                        SideDataFlags::NONE,
                        None,
                    )
                    .is_some();
                    set_max(i32::MAX as usize);
                    assert_eq!(got, want, "{label}: alloc");
                    compare(&mut r, &c, &format!("{label}: alloc"));
                    assert_eq!(rust_seen(&mut r_src), c_src.seen(), "{label}: source");
                }
            }
        }
    }

    /// Clones replacing an entry in place, which the wrapper runs itself
    /// rather than through C's leaking path (buffer-frame wave-3 symbol
    /// review): sources whose metadata C's flags-0 copy merges (keys equal
    /// but for case, exact duplicates) or keeps, over entries with and
    /// without metadata, sharing the source's buffer or not, and of a kind
    /// with no name; then, in an isolated child, every `av_max_alloc` limit
    /// up to a full copy for sources whose copy C cannot leave part way
    /// (one entry once merged), so that both fail at the metadata copy or
    /// the new reference alike.
    #[test]
    fn in_place_clones_match_c() {
        let sources: [&[(&core::ffi::CStr, &core::ffi::CStr)]; 5] = [
            &[],
            &[(c"Key", c"1"), (c"KEY", c"2"), (c"x", c"y")],
            &[(c"dup", c"1"), (c"dup", c"2")],
            &[(c"a", c"1"), (c"b", c"2"), (c"c", c"")],
            &[(c"k", c"v"), (c"K", c"w")],
        ];
        let limited = std::env::var_os("CRUSTIFY_EQUIV_ISOLATED").is_some();
        let name = "frame::side_data_array_routines::in_place_clones_match_c";
        let limits: Vec<usize> = if limited {
            (0..=64).collect()
        } else {
            vec![i32::MAX as usize]
        };
        for &k in &[SideDataType::AFD.into_raw().0, 1000] {
            for (j, pairs) in sources.iter().enumerate() {
                // Distinct entries copy one by one, so a limit could fail C
                // part way; those run unlimited only.
                let merged = c_pairs_count(pairs) <= 1;
                for &limit in limits.iter().filter(|_| merged || !limited) {
                    for (shared, old) in [(false, &PAIRS[0]), (false, &PAIRS[2]), (true, &PAIRS[1])]
                    {
                        let label = format!("kind {k} source {j} limit {limit} shared {shared}");
                        let (mut r_src, mut c_src) = (SideDataArray::new(), CArray::new());
                        let mut e = r_src
                            .alloc_from_slice(kind(k), b"source", SideDataFlags::NONE)
                            .unwrap();
                        e.set_metadata(
                            ffmpeg::libavutil::dict::Dict::from_entries(pairs.iter().copied())
                                .unwrap(),
                        );
                        let i =
                            c_new(&mut c_src, k, 6, SideDataFlags::NONE, Some(b"source")).unwrap();
                        let _ = c_src.replace_metadata(i, c_dict(pairs));
                        let (mut r, mut c) = (SideDataArray::new(), CArray::new());
                        let mut e = r
                            .alloc_from_slice(kind(k), b"old", SideDataFlags::NONE)
                            .unwrap();
                        e.set_metadata(
                            ffmpeg::libavutil::dict::Dict::from_entries(old.iter().copied())
                                .unwrap(),
                        );
                        if shared {
                            let b = r_src.as_ref().get(0).unwrap().buffer().try_clone().unwrap();
                            e.set_buffer(b);
                        }
                        let i = c_new(&mut c, k, 3, SideDataFlags::NONE, Some(b"old")).unwrap();
                        let _ = c.replace_metadata(i, c_dict(old));
                        if shared {
                            // SAFETY: a live reference of C's source.
                            let b = unsafe { ffi::av_buffer_ref((*c_src.entry(0)).buf) };
                            assert!(!b.is_null());
                            let e = c.entry(i);
                            // SAFETY: a live entry of C's array, which takes
                            // over `b` and its window, as `set_buffer` does.
                            unsafe {
                                ffi::av_buffer_unref(&raw mut (*e).buf);
                                (*e).buf = b;
                                (*e).data = (*b).data;
                                (*e).size = (*b).size;
                            }
                        }
                        compare(&mut r, &c, &label);
                        let set_max = ffmpeg::libavutil::mem::set_max_alloc;
                        set_max(limit);
                        let got = r
                            .as_mut()
                            .add_clone(r_src.as_ref().get(0).unwrap(), SideDataFlags::REPLACE);
                        // SAFETY: C's array and a live entry of C's source.
                        let want = unsafe {
                            ffi::av_frame_side_data_clone(
                                &raw mut c.sd,
                                &raw mut c.nb,
                                c_src.entry(0),
                                SideDataFlags::REPLACE.bits(),
                            )
                        };
                        set_max(i32::MAX as usize);
                        assert_eq!(got, if want < 0 { Err(want) } else { Ok(()) }, "{label}");
                        compare(&mut r, &c, &format!("{label}: clone"));
                        assert_eq!(rust_seen(&mut r_src), c_src.seen(), "{label}: source");
                        drop((r_src, c_src));
                        compare(&mut r, &c, &format!("{label}: source gone"));
                    }
                }
            }
        }
        if !limited {
            assert!(!isolated(name));
        }
    }

    /// The number of entries C's flags-0 copy of `pairs` keeps: keys equal
    /// but for ASCII case merge.
    fn c_pairs_count(pairs: &[(&core::ffi::CStr, &core::ffi::CStr)]) -> usize {
        let mut keys: Vec<Vec<u8>> = pairs
            .iter()
            .map(|(k, _)| k.to_bytes().to_ascii_uppercase())
            .collect();
        keys.sort();
        keys.dedup();
        keys.len()
    }

    /// Each flag combination of `alloc_*` and `add_clone` over an array
    /// that holds an entry of every kind the sequences use, each with
    /// metadata, against the same calls in C.
    #[test]
    fn every_flag_over_present_kinds_matches_c() {
        let all = [
            SideDataFlags::NONE,
            SideDataFlags::UNIQUE,
            SideDataFlags::REPLACE,
            SideDataFlags::UNIQUE | SideDataFlags::REPLACE,
        ];
        let (r_src, c_src) = sources();
        for flags in all {
            for (n, &k) in KINDS.iter().enumerate() {
                let (mut r, mut c) = (SideDataArray::new(), CArray::new());
                for (m, &k2) in KINDS.iter().enumerate() {
                    let pairs = PAIRS[m % PAIRS.len()];
                    let mut e = r
                        .alloc_from_slice(kind(k2), &[m as u8; 3], SideDataFlags::NONE)
                        .unwrap();
                    e.set_metadata(
                        ffmpeg::libavutil::dict::Dict::from_entries(pairs.iter().copied()).unwrap(),
                    );
                    let i = c_new(&mut c, k2, 3, SideDataFlags::NONE, Some(&[m as u8; 3])).unwrap();
                    let _ = c.replace_metadata(i, c_dict(pairs));
                }
                let label = format!("kind {k} flags {flags:?}");
                compare(&mut r, &c, &label);
                let got = r
                    .alloc_zeroed(kind(k), 5, flags)
                    .map(|e| getters(e.as_ref()));
                let want = c_new(&mut c, k, 5, flags, None).map(|i| {
                    let s = c.seen().swap_remove(i);
                    (s.kind, s.bytes, s.metadata)
                });
                assert_eq!(got, want, "{label}: alloc");
                compare(&mut r, &c, &format!("{label}: alloc"));
                for j in [n, SAMPLE.len() - 1 - n] {
                    let got = r.as_mut().add_clone(r_src.as_ref().get(j).unwrap(), flags);
                    // SAFETY: C's array and a live entry of C's source.
                    let want = unsafe {
                        ffi::av_frame_side_data_clone(
                            &raw mut c.sd,
                            &raw mut c.nb,
                            c_src.entry(j),
                            flags.bits(),
                        )
                    };
                    assert_eq!(
                        got,
                        if want < 0 { Err(want) } else { Ok(()) },
                        "{label}: clone {j}"
                    );
                    compare(&mut r, &c, &format!("{label}: clone {j}"));
                }
            }
        }
    }
}

/// [`Frame`] against C's `AVFrame` (buffer-frame wave 3, batch 0): a new
/// frame against `av_frame_alloc`'s; a Rust frame and a C frame driven
/// through the same random sequences of field, buffer, plane, extra buffer,
/// `extended_data`, side data, metadata and channel layout changes, compared
/// after every step by C's own reading of both (every field, where each
/// plane points among the buffers, the buffers' bytes, counts and
/// writability, side data and metadata) and by the Rust getters; every
/// step's clone against `av_frame_clone`'s and the reset against
/// `av_frame_unref`; planes against `av_frame_get_plane_buffer` and
/// writability against `av_buffer_is_writable`; and frames whose planes are
/// external memory, copied by both clones.
///
/// [`Frame`]: ffmpeg::libavutil::frame::Frame
mod frame_type {
    use core::ffi::{CStr, c_int};
    use core::ptr;

    use ffmpeg::libavutil::avutil::PictureType;
    use ffmpeg::libavutil::buffer::{BufferReference, BufferReferenceBox};
    use ffmpeg::libavutil::channel_layout::ChannelLayout;
    use ffmpeg::libavutil::dict::Dict;
    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::frame::{
        DecodeErrorFlags, Frame, FrameBox, FrameFlags, FrameRef, NUM_DATA_POINTERS, PlaneError,
        SideDataFlags, SideDataType,
    };
    use ffmpeg::libavutil::pixfmt::{
        AlphaMode, ChromaLocation, ColorPrimaries, ColorRange, ColorSpace, ColorTransfer,
    };
    use ffmpeg::libavutil::rational::Rational;

    /// Where a plane pointer points: NULL, inside unified buffer `index`
    /// (`buf`, then `extended_buf`) at `offset`, or nowhere C allocated.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum At {
        Null,
        In(usize, usize),
        Stray,
    }

    /// What C sees of a frame.
    #[derive(Clone, Debug, PartialEq, Eq)]
    struct Seen {
        ints: Vec<i64>,
        linesize: [c_int; NUM_DATA_POINTERS],
        layout: (u32, c_int),
        opaque: usize,
        planes: Vec<At>,
        extended: Option<Vec<At>>,
        buffers: Vec<Option<(Vec<u8>, c_int, bool)>>,
        side_data: Vec<(u32, Vec<u8>, Pairs)>,
        metadata: Pairs,
        opaque_ref: Option<(Vec<u8>, c_int)>,
        hw: bool,
        private: bool,
    }

    /// Key/value pairs as bytes, in iteration order.
    type Pairs = Vec<(Vec<u8>, Vec<u8>)>;

    /// C's reading of a dictionary, in iteration order.
    fn c_pairs(m: *const ffi::AVDictionary) -> Pairs {
        let mut out = Vec::new();
        let mut e = ptr::null();
        loop {
            // SAFETY: `m` is NULL or a live dictionary; `e` is NULL or its
            // previous entry.
            e = unsafe { ffi::av_dict_iterate(m, e) };
            if e.is_null() {
                return out;
            }
            // SAFETY: a live entry, whose key and value are C strings.
            let (k, v) = unsafe { (CStr::from_ptr((*e).key), CStr::from_ptr((*e).value)) };
            out.push((k.to_bytes().into(), v.to_bytes().into()));
        }
    }

    /// C's reading of a reference: bytes, count, writability.
    fn c_buf(b: *const ffi::AVBufferRef) -> (Vec<u8>, c_int, bool) {
        // SAFETY: a live reference whose window is initialised bytes.
        unsafe {
            let bytes = if (*b).size == 0 {
                Vec::new()
            } else {
                std::slice::from_raw_parts((*b).data, (*b).size).to_vec()
            };
            (
                bytes,
                ffi::av_buffer_get_ref_count(b),
                ffi::av_buffer_is_writable(b) != 0,
            )
        }
    }

    /// The unified buffer list of a frame: `buf`, then `extended_buf`.
    ///
    /// # Safety
    ///
    /// `f` must be a live frame.
    unsafe fn c_buffers(f: *const ffi::AVFrame) -> Vec<*mut ffi::AVBufferRef> {
        // SAFETY: the caller's live frame and its extra block.
        unsafe {
            let mut v: Vec<_> = (*f).buf.to_vec();
            for i in 0..(*f).nb_extended_buf.max(0) as usize {
                v.push(*(*f).extended_buf.add(i));
            }
            v
        }
    }

    /// Where `p` points among `bufs`.
    fn c_at(bufs: &[*mut ffi::AVBufferRef], p: *const u8) -> At {
        if p.is_null() {
            return At::Null;
        }
        for (i, &b) in bufs.iter().enumerate() {
            if b.is_null() {
                continue;
            }
            // SAFETY: a live reference.
            let (data, size) = unsafe { ((*b).data, (*b).size) };
            let off = p.addr().wrapping_sub(data.addr());
            if p.addr() >= data.addr() && off < size {
                return At::In(i, off);
            }
        }
        At::Stray
    }

    /// C's reading of the frame at `f`.
    ///
    /// # Safety
    ///
    /// `f` must be a live frame whose buffers hold initialised bytes.
    unsafe fn c_seen(f: *const ffi::AVFrame) -> Seen {
        // SAFETY: the caller's live frame, every pointer of which is NULL or
        // live.
        unsafe {
            let bufs = c_buffers(f);
            let x = &*f;
            let separate = x.extended_data.cast_const() != (&raw const (*f).data).cast();
            let extended = separate.then(|| {
                (0..x.ch_layout.nb_channels.max(0) as usize)
                    .map(|i| c_at(&bufs, *x.extended_data.add(i)))
                    .collect()
            });
            let side_data = (0..x.nb_side_data.max(0) as usize)
                .map(|i| {
                    let e = *x.side_data.add(i);
                    (
                        (*e).type_.0,
                        if (*e).size == 0 {
                            Vec::new()
                        } else {
                            std::slice::from_raw_parts((*e).data, (*e).size).to_vec()
                        },
                        c_pairs((*e).metadata),
                    )
                })
                .collect();
            Seen {
                ints: vec![
                    x.width.into(),
                    x.height.into(),
                    x.nb_samples.into(),
                    x.format.into(),
                    x.pict_type.0.into(),
                    x.sample_aspect_ratio.num.into(),
                    x.sample_aspect_ratio.den.into(),
                    x.pts,
                    x.pkt_dts,
                    x.time_base.num.into(),
                    x.time_base.den.into(),
                    x.quality.into(),
                    x.repeat_pict.into(),
                    x.sample_rate.into(),
                    x.flags.into(),
                    x.color_range.0.into(),
                    x.color_primaries.0.into(),
                    x.color_trc.0.into(),
                    x.colorspace.0.into(),
                    x.chroma_location.0.into(),
                    x.best_effort_timestamp,
                    x.decode_error_flags.into(),
                    x.crop_top as i64,
                    x.crop_bottom as i64,
                    x.crop_left as i64,
                    x.crop_right as i64,
                    x.duration,
                    x.alpha_mode.0.into(),
                ],
                linesize: x.linesize,
                layout: (x.ch_layout.order.0, x.ch_layout.nb_channels),
                opaque: x.opaque.addr(),
                planes: x.data.iter().map(|&p| c_at(&bufs, p)).collect(),
                extended,
                buffers: bufs
                    .iter()
                    .map(|&b| (!b.is_null()).then(|| c_buf(b)))
                    .collect(),
                side_data,
                metadata: c_pairs(x.metadata),
                opaque_ref: (!x.opaque_ref.is_null()).then(|| {
                    let (bytes, count, _) = c_buf(x.opaque_ref);
                    (bytes, count)
                }),
                hw: !x.hw_frames_ctx.is_null(),
                private: !x.private_ref.is_null(),
            }
        }
    }

    /// C's reading of a Rust frame, which C reads through its pointer.
    fn rust_seen(f: &FrameBox) -> Seen {
        // SAFETY: the box's live frame, whose buffers hold initialised
        // bytes, read only.
        unsafe { c_seen(f.as_c_ptr()) }
    }

    /// The Rust getters' reading of a frame, in [`Seen`]'s terms.
    fn getters(r: FrameRef<'_>) -> Seen {
        let mut bufs: Vec<_> = (0..NUM_DATA_POINTERS).map(|i| r.buffer(i)).collect();
        bufs.extend((0..r.extended_buffer_count()).map(|i| r.extended_buffer(i)));
        // Where a plane view starts among the buffers' bytes; Rust gives no
        // view of a pointer outside them.
        let at = |view: Option<&[u8]>| match view {
            None => At::Null,
            Some(v) => bufs
                .iter()
                .enumerate()
                .find_map(|(i, b)| {
                    let d = b.as_ref()?.data();
                    let off = v.as_ptr().addr().wrapping_sub(d.as_ptr().addr());
                    (off < d.len() && v.len() == d.len() - off).then_some(At::In(i, off))
                })
                .unwrap_or(At::Stray),
        };
        let pairs = |m: Option<ffmpeg::libavutil::dict::DictionaryRef<'_>>| {
            m.map_or_else(Vec::new, |m| {
                m.iter()
                    .map(|p| (p.key().to_bytes().into(), p.value().to_bytes().into()))
                    .collect()
            })
        };
        let sar = r.sample_aspect_ratio().get().into_raw();
        let tb = r.time_base().get().into_raw();
        let layout = r.channel_layout();
        Seen {
            ints: vec![
                r.width().into(),
                r.height().into(),
                r.sample_count().into(),
                r.format().into(),
                r.picture_type().into_raw().0.into(),
                sar.num.into(),
                sar.den.into(),
                r.pts(),
                r.packet_dts(),
                tb.num.into(),
                tb.den.into(),
                r.quality().into(),
                r.repeat_pict().into(),
                r.sample_rate().into(),
                r.flags().bits().into(),
                r.color_range().into_raw().0.into(),
                r.color_primaries().into_raw().0.into(),
                r.color_transfer().into_raw().0.into(),
                r.color_space().into_raw().0.into(),
                r.chroma_location().into_raw().0.into(),
                r.best_effort_timestamp(),
                r.decode_error_flags().bits().into(),
                r.crop_top() as i64,
                r.crop_bottom() as i64,
                r.crop_left() as i64,
                r.crop_right() as i64,
                r.duration(),
                r.alpha_mode().into_raw().0.into(),
            ],
            linesize: r.linesizes(),
            layout: (layout.order().into_raw().0, layout.channel_count()),
            opaque: r.opaque().addr(),
            planes: (0..NUM_DATA_POINTERS).map(|i| at(r.plane(i))).collect(),
            extended: r.has_extended_data().then(|| {
                (0..r.extended_plane_count())
                    .map(|i| at(r.extended_plane(i)))
                    .collect()
            }),
            buffers: bufs
                .iter()
                .map(|b| b.map(|b| (b.data().to_vec(), b.ref_count() as c_int, b.is_writable())))
                .collect(),
            side_data: r
                .side_data()
                .iter()
                .map(|e| {
                    (
                        e.kind().into_raw().0,
                        e.data().to_vec(),
                        pairs(e.metadata()),
                    )
                })
                .collect(),
            metadata: pairs(r.metadata()),
            opaque_ref: r
                .opaque_ref()
                .map(|b| (b.data().to_vec(), b.ref_count() as c_int)),
            hw: r.hw_frames_context().is_some(),
            private: r.has_private_ref(),
        }
    }

    /// A frame C allocates and writes, freed with `av_frame_free`.
    struct CFrame(*mut ffi::AVFrame);

    impl CFrame {
        fn new() -> Self {
            // SAFETY: no precondition.
            let f = unsafe { ffi::av_frame_alloc() };
            assert!(!f.is_null());
            Self(f)
        }

        fn seen(&self) -> Seen {
            // SAFETY: the frame C built, whose buffers this test filled.
            unsafe { c_seen(self.0) }
        }

        fn bufs(&self) -> Vec<*mut ffi::AVBufferRef> {
            // SAFETY: a live frame.
            unsafe { c_buffers(self.0) }
        }

        /// The C frame's `extended_data` entries this test may touch, and
        /// whether the array is separate.
        fn extended(&self) -> (*mut *mut u8, usize, bool) {
            // SAFETY: a live frame.
            unsafe {
                let f = self.0;
                let own = (&raw mut (*f).data).cast::<*mut u8>();
                let ext = (*f).extended_data;
                if ext == own {
                    (ext, NUM_DATA_POINTERS, false)
                } else {
                    (ext, (*f).ch_layout.nb_channels.max(0) as usize, true)
                }
            }
        }

        /// Clears the plane pointers a well-behaved C user must clear after
        /// a buffer change: every one outside the buffers, and all of them
        /// without a buffer at index 0.
        fn prune(&mut self) {
            let bufs = self.bufs();
            let counted = !bufs[0].is_null();
            let keep = |p: *mut u8| counted && matches!(c_at(&bufs, p), At::In(..)) || p.is_null();
            // SAFETY: a live frame and its own `extended_data` entries.
            unsafe {
                for i in 0..NUM_DATA_POINTERS {
                    if !keep((*self.0).data[i]) {
                        (*self.0).data[i] = ptr::null_mut();
                    }
                }
                let (ext, n, separate) = self.extended();
                if separate {
                    for i in 0..n {
                        if !keep(*ext.add(i)) {
                            *ext.add(i) = ptr::null_mut();
                        }
                    }
                }
            }
        }

        /// The pointer `offset` bytes into unified buffer `b`, as
        /// `set_plane` checks it.
        fn target(&self, b: usize, offset: usize) -> Result<*mut u8, PlaneError> {
            let bufs = self.bufs();
            if bufs[0].is_null() {
                return Err(PlaneError::NoBuffer);
            }
            let buf = *bufs.get(b).ok_or(PlaneError::NoBuffer)?;
            if buf.is_null() {
                return Err(PlaneError::NoBuffer);
            }
            // SAFETY: a live reference.
            let (data, size) = unsafe { ((*buf).data, (*buf).size) };
            if offset >= size {
                return Err(PlaneError::OutOfBounds);
            }
            Ok(data.wrapping_add(offset))
        }
    }

    impl Drop for CFrame {
        fn drop(&mut self) {
            // SAFETY: frees the frame and everything it holds.
            unsafe { ffi::av_frame_free(&mut self.0) }
        }
    }

    /// `len` bytes from `seed`.
    fn pattern(len: usize, seed: u8) -> Vec<u8> {
        (0..len)
            .map(|i| (i as u8).wrapping_mul(7).wrapping_add(seed))
            .collect()
    }

    /// A Rust reference holding `bytes`.
    fn rust_buffer(bytes: &[u8]) -> BufferReferenceBox {
        BufferReference::alloc(bytes.len())
            .unwrap()
            .copy_from_slice(bytes)
    }

    /// A C reference holding `bytes`.
    fn c_buffer(bytes: &[u8]) -> *mut ffi::AVBufferRef {
        // SAFETY: a fresh reference to `len` bytes, filled from `bytes`.
        unsafe {
            let b = ffi::av_buffer_alloc(bytes.len());
            assert!(!b.is_null());
            ptr::copy_nonoverlapping(bytes.as_ptr(), (*b).data, bytes.len());
            b
        }
    }

    /// A C dictionary of `pairs`.
    fn c_dict(pairs: &[(&CStr, &CStr)]) -> *mut ffi::AVDictionary {
        let mut m = ptr::null_mut();
        for (k, v) in pairs {
            // SAFETY: C copies the strings into `m`.
            let r = unsafe {
                ffi::av_dict_set(
                    &mut m,
                    k.as_ptr(),
                    v.as_ptr(),
                    ffi::AV_DICT_MULTIKEY as c_int,
                )
            };
            assert!(r >= 0);
        }
        m
    }

    /// xorshift64*.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 >> 12;
            self.0 ^= self.0 << 25;
            self.0 ^= self.0 >> 27;
            self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
        }

        fn below(&mut self, n: u64) -> usize {
            (self.next() % n) as usize
        }
    }

    const PAIRS: [(&CStr, &CStr); 3] = [(c"a", c"1"), (c"b", c"2"), (c"a", c"3")];

    /// Asserts both frames read the same to C, and the Rust frame the same
    /// through its getters, and that their clones agree. Returns how many
    /// planes point into buffers, and whether the frames cloned.
    fn compare(r: &FrameBox, c: &CFrame, step: &str) -> (usize, bool) {
        let seen = c.seen();
        assert_eq!(rust_seen(r), seen, "{step}");
        assert_eq!(getters(r.as_ref()), seen, "{step} (getters)");
        assert!(!seen.planes.contains(&At::Stray), "{step}");
        let placed = seen.planes.iter().chain(seen.extended.iter().flatten());
        let placed = placed.filter(|a| matches!(a, At::In(..))).count();
        // SAFETY: a live frame, read only.
        let cc = unsafe { ffi::av_frame_clone(c.0) };
        let rc = r.as_ref().try_clone();
        let cloned = rc.is_some();
        assert_eq!(cloned, !cc.is_null(), "{step} (clone)");
        if let Some(rc) = rc {
            let cc = CFrame(cc);
            assert_eq!(rust_seen(&rc), cc.seen(), "{step} (clones)");
            assert_eq!(rust_seen(r), c.seen(), "{step} (after clone)");
            // C's lookup reads `extended_data[i]` unchecked (for i < 4 on
            // video), so only entries the frame holds are asked for.
            for i in 0..rc.as_ref().extended_plane_count().min(NUM_DATA_POINTERS) {
                // SAFETY: live frames, read only, holding entry `i`.
                let (x, y) = unsafe {
                    (
                        ffi::av_frame_get_plane_buffer(rc.as_c_ptr(), i as c_int),
                        ffi::av_frame_get_plane_buffer(cc.0, i as c_int),
                    )
                };
                assert_eq!(x.is_null(), y.is_null(), "{step} (plane buffer {i})");
            }
        }
        for i in 0..r.as_ref().extended_plane_count().min(NUM_DATA_POINTERS) {
            // SAFETY: a live frame holding `extended_data[i]`, read only; C
            // compares addresses.
            let pb = unsafe { ffi::av_frame_get_plane_buffer(r.as_c_ptr(), i as c_int) };
            let view = r.as_ref().extended_plane(i);
            if !pb.is_null() {
                // SAFETY: the live reference C found.
                let (data, size) = unsafe { ((*pb).data, (*pb).size) };
                let v = view.expect("C found the plane's buffer");
                assert_eq!(
                    v.as_ptr().addr() + v.len(),
                    data.addr() + size,
                    "{step} ({i})"
                );
            }
        }
        // SAFETY: a live frame, read only.
        let writable = unsafe { ffi::av_frame_is_writable(c.0) } != 0;
        // SAFETY: as above.
        let rust_writable = unsafe { ffi::av_frame_is_writable(r.as_c_ptr()) } != 0;
        assert_eq!(rust_writable, writable, "{step}");
        (placed, cloned)
    }

    #[test]
    fn new_frames_match_av_frame_alloc() {
        let r = Frame::new().unwrap();
        let c = CFrame::new();
        assert_eq!(compare(&r, &c, "new"), (0, false));
        assert!(r.as_ref().try_clone().is_none());
        // SAFETY: both live; C's defaults put `extended_data` at `data`.
        unsafe {
            let (x, y) = (r.as_c_ptr(), c.0);
            assert_eq!((*x).extended_data, (&raw mut (*x).data).cast());
            assert_eq!((*y).extended_data, (&raw mut (*y).data).cast());
        }
    }

    /// One random step, applied to both frames.
    fn step(rng: &mut Rng, r: &mut FrameBox, c: &mut CFrame) -> String {
        let f = c.0;
        match rng.below(18) {
            0 => {
                let i = if rng.below(2) == 0 {
                    0
                } else {
                    rng.below(NUM_DATA_POINTERS as u64)
                };
                let len = rng.below(48) + 16;
                let bytes = pattern(len, rng.next() as u8);
                let none = rng.below(6) == 0;
                let old = r
                    .as_mut()
                    .replace_buffer(i, (!none).then(|| rust_buffer(&bytes)));
                // SAFETY: the frame's slot, which takes the new reference;
                // the old one is released after the planes are pruned, as
                // the Rust side's drop of the reference it returns is.
                let mut was = unsafe {
                    let was = (*f).buf[i];
                    (*f).buf[i] = if none {
                        ptr::null_mut()
                    } else {
                        c_buffer(&bytes)
                    };
                    was
                };
                c.prune();
                drop(old);
                // SAFETY: the reference the frame held.
                unsafe { ffi::av_buffer_unref(&mut was) };
                format!("buffer {i} = {}", if none { 0 } else { len })
            }
            1 | 2 => {
                let p = rng.below(NUM_DATA_POINTERS as u64 + 1);
                let b = if rng.below(2) == 0 {
                    0
                } else {
                    rng.below(NUM_DATA_POINTERS as u64 + 3)
                };
                let off = rng.below(24);
                let x = r.as_mut().set_plane(p, b, off);
                let y = if p >= NUM_DATA_POINTERS {
                    Err(PlaneError::NoPlane)
                } else {
                    c.target(b, off).map(|t| {
                        // SAFETY: plane `p` of the live frame.
                        unsafe { (*f).data[p] = t };
                    })
                };
                assert_eq!(x, y);
                format!("plane {p} = {b}+{off}")
            }
            3 => {
                let p = rng.below(NUM_DATA_POINTERS as u64 + 1);
                r.as_mut().clear_plane(p);
                if p < NUM_DATA_POINTERS {
                    // SAFETY: plane `p` of the live frame.
                    unsafe { (*f).data[p] = ptr::null_mut() };
                }
                format!("clear plane {p}")
            }
            4 => {
                let len = rng.below(24) + 1;
                let bytes = pattern(len, 9);
                r.as_mut()
                    .push_extended_buffer(rust_buffer(&bytes))
                    .unwrap();
                // SAFETY: the frame's extra block, grown by one reference.
                unsafe {
                    let n = (*f).nb_extended_buf as usize;
                    let grown =
                        ffi::av_realloc_array((*f).extended_buf.cast(), n + 1, size_of_ptr());
                    assert!(!grown.is_null());
                    (*f).extended_buf = grown.cast();
                    *(*f).extended_buf.add(n) = c_buffer(&bytes);
                    (*f).nb_extended_buf += 1;
                }
                "push extended".into()
            }
            5 => {
                let x = r.as_mut().pop_extended_buffer();
                // SAFETY: the frame's extra block, shrunk by one; the
                // reference taken out is released after the planes are
                // pruned, as the Rust side's is.
                let mut y = unsafe {
                    let n = (*f).nb_extended_buf as usize;
                    if n == 0 {
                        ptr::null_mut()
                    } else {
                        let last = *(*f).extended_buf.add(n - 1);
                        (*f).nb_extended_buf -= 1;
                        if n == 1 {
                            ffi::av_freep((&raw mut (*f).extended_buf).cast());
                        }
                        last
                    }
                };
                c.prune();
                assert_eq!(x.is_some(), !y.is_null());
                drop(x);
                // SAFETY: the reference taken out, or NULL.
                unsafe { ffi::av_buffer_unref(&mut y) };
                "pop extended".into()
            }
            6 => {
                let i = rng.below(4);
                let bytes = pattern(rng.below(16) + 1, 3);
                let x = r.as_mut().replace_extended_buffer(i, rust_buffer(&bytes));
                // SAFETY: entry `i` of the frame's extra block, when present;
                // the old reference is released after the planes are pruned.
                let mut y = unsafe {
                    if i < (*f).nb_extended_buf as usize {
                        let slot = (*f).extended_buf.add(i);
                        core::mem::replace(&mut *slot, c_buffer(&bytes))
                    } else {
                        ptr::null_mut()
                    }
                };
                c.prune();
                assert_eq!(x.is_ok(), !y.is_null());
                drop(x);
                // SAFETY: the reference taken out, or NULL.
                unsafe { ffi::av_buffer_unref(&mut y) };
                format!("replace extended {i}")
            }
            7 => {
                let n = rng.below(12) as i32;
                let x = r.as_mut().set_channel_layout(ChannelLayout::unspecified(n));
                let (_, _, separate) = c.extended();
                // SAFETY: the frame's layout, unspecified order (no map).
                let y = unsafe {
                    if separate && n > (*f).ch_layout.nb_channels {
                        false
                    } else {
                        ffi::av_channel_layout_uninit(&raw mut (*f).ch_layout);
                        (*f).ch_layout.nb_channels = n;
                        true
                    }
                };
                assert_eq!(x.is_ok(), y);
                format!("layout {n}")
            }
            8 => {
                let x = r.as_mut().alloc_extended_data();
                let (ext, _, separate) = c.extended();
                // SAFETY: the frame's layout and `extended_data`, which takes
                // a fresh block copying `data`.
                let y = unsafe {
                    let n = (*f).ch_layout.nb_channels;
                    if n <= 0 {
                        Err(-(ffi::EINVAL as c_int))
                    } else {
                        let block = ffi::av_calloc(n as usize, size_of_ptr()).cast::<*mut u8>();
                        for i in 0..(n as usize).min(NUM_DATA_POINTERS) {
                            *block.add(i) = (*f).data[i];
                        }
                        if separate {
                            ffi::av_free(ext.cast());
                        }
                        (*f).extended_data = block;
                        Ok(())
                    }
                };
                assert_eq!(x, y);
                "alloc extended data".into()
            }
            9 => {
                r.as_mut().reset_extended_data();
                let (ext, _, separate) = c.extended();
                // SAFETY: the frame's own block, freed, and its `data`.
                unsafe {
                    if separate {
                        ffi::av_free(ext.cast());
                    }
                    (*f).extended_data = (&raw mut (*f).data).cast();
                }
                "reset extended data".into()
            }
            10 => {
                let i = rng.below(14);
                let b = if rng.below(2) == 0 { 0 } else { rng.below(12) };
                let off = rng.below(24);
                let x = r.as_mut().set_extended_plane(i, b, off);
                let (ext, n, _) = c.extended();
                let y = if i >= n {
                    Err(PlaneError::NoPlane)
                } else {
                    c.target(b, off).map(|t| {
                        // SAFETY: entry `i < n` of `extended_data`.
                        unsafe { *ext.add(i) = t };
                    })
                };
                assert_eq!(x, y);
                format!("extended plane {i} = {b}+{off}")
            }
            11 => {
                let kind = [7u32, 20, 20, 15][rng.below(4)];
                let bytes = pattern(rng.below(20), 1);
                let x = r
                    .as_mut()
                    .side_data_mut()
                    .add(
                        SideDataType::from_raw(ffi::AVFrameSideDataType(kind)),
                        rust_buffer(&bytes),
                        SideDataFlags::NONE,
                    )
                    .is_ok();
                let mut slot = c_buffer(&bytes);
                // SAFETY: the frame's side data variables and a reference C
                // takes on success.
                let y = unsafe {
                    let e = ffi::av_frame_side_data_add(
                        &raw mut (*f).side_data,
                        &raw mut (*f).nb_side_data,
                        ffi::AVFrameSideDataType(kind),
                        &mut slot,
                        0,
                    );
                    ffi::av_buffer_unref(&mut slot);
                    !e.is_null()
                };
                assert_eq!(x, y);
                format!("side data {kind}")
            }
            12 => {
                let n = rng.below(4);
                r.as_mut()
                    .set_metadata(Dict::from_entries(PAIRS[..n].iter().copied()).unwrap());
                // SAFETY: the frame's dictionary, replaced.
                unsafe {
                    ffi::av_dict_free(&raw mut (*f).metadata);
                    (*f).metadata = c_dict(&PAIRS[..n]);
                }
                format!("metadata {n}")
            }
            13 => {
                let none = rng.below(3) == 0;
                let bytes = pattern(5, 4);
                r.as_mut()
                    .set_opaque_ref((!none).then(|| rust_buffer(&bytes)));
                // SAFETY: the frame's user data reference, replaced.
                unsafe {
                    ffi::av_buffer_unref(&raw mut (*f).opaque_ref);
                    if !none {
                        (*f).opaque_ref = c_buffer(&bytes);
                    }
                }
                format!("opaque_ref {none}")
            }
            14 => {
                let p = rng.below(NUM_DATA_POINTERS as u64);
                let byte = rng.next() as u8;
                let x = r.as_mut().plane_mut(p).map(|v| v.fill(byte)).is_some();
                let bufs = c.bufs();
                // SAFETY: plane `p` of the live frame, written as `plane_mut`
                // does when its buffer is writable.
                let y = unsafe {
                    match c_at(&bufs, (*f).data[p]) {
                        At::In(b, off)
                            if !bufs[0].is_null() && ffi::av_buffer_is_writable(bufs[b]) != 0 =>
                        {
                            let size = (*bufs[b]).size;
                            ptr::write_bytes((*f).data[p], byte, size - off);
                            true
                        }
                        _ => false,
                    }
                };
                assert_eq!(x, y);
                format!("write plane {p}")
            }
            15 => {
                let v = rng.next() as i64;
                let w = v as i32;
                let mut m = r.as_mut();
                m.set_width(w);
                m.set_height(w >> 3);
                m.set_sample_count(w >> 5);
                m.set_format(w % 200);
                m.set_picture_type(PictureType::from_raw(ffi::AVPictureType(w as u32 % 9)));
                m.set_sample_aspect_ratio(Rational::new(w, 7));
                m.set_pts(v);
                m.set_packet_dts(v ^ 1);
                m.time_base_mut().set_num(w ^ 3);
                m.set_quality(w ^ 4);
                m.set_repeat_pict(w & 3);
                m.set_sample_rate(w >> 1);
                m.set_flags(FrameFlags::from_bits_retain(w));
                m.set_color_range(ColorRange::from_raw(ffi::AVColorRange(w as u32 % 4)));
                m.set_color_primaries(ColorPrimaries::from_raw(ffi::AVColorPrimaries(
                    w as u32 % 23,
                )));
                m.set_color_transfer(ColorTransfer::from_raw(ffi::AVColorTransferCharacteristic(
                    w as u32 % 19,
                )));
                m.set_color_space(ColorSpace::from_raw(ffi::AVColorSpace(w as u32 % 18)));
                m.set_chroma_location(ChromaLocation::from_raw(ffi::AVChromaLocation(
                    w as u32 % 7,
                )));
                m.set_best_effort_timestamp(!v);
                m.set_decode_error_flags(DecodeErrorFlags::from_bits_retain(w & 15));
                m.set_crop_top(v as usize);
                m.set_crop_bottom(v as usize >> 2);
                m.set_crop_left(3);
                m.set_crop_right(v as usize & 0xffff);
                m.set_duration(v >> 4);
                m.set_alpha_mode(AlphaMode::from_raw(ffi::AVAlphaMode(w as u32 % 3)));
                m.set_linesize(w as usize & 7, w);
                m.set_opaque(ptr::without_provenance_mut(v as usize));
                // SAFETY: plain fields of the live frame.
                unsafe {
                    let x = &mut *f;
                    x.width = w;
                    x.height = w >> 3;
                    x.nb_samples = w >> 5;
                    x.format = w % 200;
                    x.pict_type = ffi::AVPictureType(w as u32 % 9);
                    x.sample_aspect_ratio = ffi::AVRational { num: w, den: 7 };
                    x.pts = v;
                    x.pkt_dts = v ^ 1;
                    x.time_base.num = w ^ 3;
                    x.quality = w ^ 4;
                    x.repeat_pict = w & 3;
                    x.sample_rate = w >> 1;
                    x.flags = w;
                    x.color_range = ffi::AVColorRange(w as u32 % 4);
                    x.color_primaries = ffi::AVColorPrimaries(w as u32 % 23);
                    x.color_trc = ffi::AVColorTransferCharacteristic(w as u32 % 19);
                    x.colorspace = ffi::AVColorSpace(w as u32 % 18);
                    x.chroma_location = ffi::AVChromaLocation(w as u32 % 7);
                    x.best_effort_timestamp = !v;
                    x.decode_error_flags = w & 15;
                    x.crop_top = v as usize;
                    x.crop_bottom = v as usize >> 2;
                    x.crop_left = 3;
                    x.crop_right = v as usize & 0xffff;
                    x.duration = v >> 4;
                    x.alpha_mode = ffi::AVAlphaMode(w as u32 % 3);
                    x.linesize[w as usize & 7] = w;
                    x.opaque = ptr::without_provenance_mut(v as usize);
                }
                "scalars".into()
            }
            16 => {
                let mut m = r.as_mut();
                m.set_pixel_format(ffmpeg::libavutil::pixfmt::PixelFormat::GRAY8);
                m.set_width(4);
                m.set_height(2);
                m.set_sample_count(0);
                m.set_linesize(0, 4);
                // SAFETY: plain fields of the live frame.
                unsafe {
                    (*f).format = ffi::AVPixelFormat::AV_PIX_FMT_GRAY8.0;
                    (*f).width = 4;
                    (*f).height = 2;
                    (*f).nb_samples = 0;
                    (*f).linesize[0] = 4;
                }
                "gray 4x2".into()
            }
            _ => {
                if rng.below(4) == 0 {
                    r.as_mut().reset();
                    // SAFETY: a live frame.
                    unsafe { ffi::av_frame_unref(f) };
                    "reset".into()
                } else {
                    "nothing".into()
                }
            }
        }
    }

    /// `seen` with each buffer's bytes reduced to their count, for frames
    /// whose buffers' bytes outside the planes C leaves uninitialised.
    fn shape(mut seen: Seen) -> Seen {
        for b in seen.buffers.iter_mut().flatten() {
            b.0 = (b.0.len() as u64).to_le_bytes().to_vec();
        }
        seen
    }

    /// The size of a pointer, which C's arrays of planes and references
    /// hold.
    fn size_of_ptr() -> usize {
        core::mem::size_of::<*mut u8>()
    }

    #[test]
    fn random_sequences_read_alike_after_every_step() {
        // How many steps saw planes placed in buffers, and clones made, so
        // that the sequences are known to reach both.
        let (mut placed, mut cloned) = (0, 0);
        for seed in 1..=40u64 {
            let mut rng = Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
            let mut r = Frame::new().unwrap();
            let mut c = CFrame::new();
            for n in 0..60 {
                let what = step(&mut rng, &mut r, &mut c);
                let (p, cl) = compare(&r, &c, &format!("seed {seed} step {n}: {what}"));
                placed += usize::from(p > 0);
                cloned += usize::from(cl);
            }
            // Released alike: references both sides kept outlive the frames.
            let keep_r: Vec<_> = (0..NUM_DATA_POINTERS)
                .filter_map(|i| r.as_ref().buffer(i).map(|b| b.try_clone().unwrap()))
                .collect();
            let keep_c: Vec<_> = c
                .bufs()
                .into_iter()
                .take(NUM_DATA_POINTERS)
                .filter(|b| !b.is_null())
                // SAFETY: a live reference, copied.
                .map(|b| unsafe { ffi::av_buffer_ref(b) })
                .collect();
            r.as_mut().reset();
            // SAFETY: a live frame.
            unsafe { ffi::av_frame_unref(c.0) };
            compare(&r, &c, &format!("seed {seed} reset"));
            drop((r, c));
            assert_eq!(keep_r.len(), keep_c.len());
            for (x, mut y) in keep_r.into_iter().zip(keep_c) {
                // SAFETY: the reference this test holds.
                assert_eq!(x.as_ref().ref_count() as c_int, unsafe {
                    ffi::av_buffer_get_ref_count(y)
                });
                // SAFETY: released once.
                unsafe { ffi::av_buffer_unref(&mut y) };
            }
        }
        eprintln!("placed {placed}, cloned {cloned} of 2400 steps");
        assert!(
            placed > 200 && cloned > 200,
            "placed {placed}, cloned {cloned}"
        );
    }

    /// Frames whose plane is external memory: both clones copy it into
    /// buffers of their own, alike.
    #[test]
    fn clones_copy_external_planes_alike() {
        for (w, h) in [(1, 1), (7, 3), (32, 32)] {
            let mut mem = pattern((w * h) as usize, 5);
            let mut r = Frame::new().unwrap();
            let c = CFrame::new();
            let mut m = r.as_mut();
            m.set_pixel_format(ffmpeg::libavutil::pixfmt::PixelFormat::GRAY8);
            m.set_width(w);
            m.set_height(h);
            m.set_linesize(0, w);
            // SAFETY: the frame holds no buffer; `mem` is the whole plane at
            // stride `w` and outlives both frames; nothing writes it.
            unsafe { m.set_data_ptr(0, mem.as_mut_ptr()) };
            // SAFETY: the same fields of the C frame.
            unsafe {
                (*c.0).format = ffi::AVPixelFormat::AV_PIX_FMT_GRAY8.0;
                (*c.0).width = w;
                (*c.0).height = h;
                (*c.0).linesize[0] = w;
                (*c.0).data[0] = mem.as_mut_ptr();
            }
            let rc = r.as_ref().try_clone().unwrap();
            // SAFETY: a live frame, read only.
            let cc = CFrame(unsafe { ffi::av_frame_clone(c.0) });
            assert!(!cc.0.is_null());
            // The copies' strides and buffers are C's choice, alike; their
            // lines hold `mem`'s, and the rest, which C leaves uninitialised,
            // is zero on the Rust side.
            assert_eq!(shape(rust_seen(&rc)), shape(cc.seen()));
            let line = rc.as_ref().linesizes()[0] as usize;
            let plane = rc.as_ref().plane(0).unwrap();
            // SAFETY: the C copy's live plane, whose first `h` lines C wrote.
            let c_plane = unsafe { (*cc.0).data[0] };
            for y in 0..h as usize {
                let w = w as usize;
                assert_eq!(&plane[y * line..y * line + w], &mem[y * w..(y + 1) * w]);
                // SAFETY: line `y` of the C copy, `w` bytes C wrote.
                let c_line = unsafe { std::slice::from_raw_parts(c_plane.add(y * line), w) };
                assert_eq!(c_line, &plane[y * line..y * line + w]);
                let pad = &plane[y * line + w..(y + 1) * line];
                assert_eq!(pad, &vec![0; pad.len()][..]);
            }
            let rest = &plane[h as usize * line..];
            assert_eq!(rest, &vec![0; rest.len()][..]);
            drop((r, c));
        }
    }
}
