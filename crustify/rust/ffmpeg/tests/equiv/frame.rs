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
            // SAFETY: a non-NULL result is one of the frame's live entries.
            (!sd.is_null())
                .then(|| SideDataType::from_raw(unsafe { ptr::addr_of!((*sd).type_).read() }))
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
