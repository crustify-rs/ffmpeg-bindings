//! Equivalence of `libavutil/frame.h`'s wrapped types with C, as C observes
//! them.
#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "test inputs are chosen within range, and the casts mirror the C conversions under comparison"
)]
#![expect(
    clippy::many_single_char_names,
    reason = "names follow the C fields, formulas and fixtures they mirror"
)]
#![expect(
    clippy::too_many_lines,
    reason = "each test is one scenario, its helpers and steps kept where they are used"
)]
#![expect(
    clippy::assert_is_empty,
    reason = "assertions keep the form of the C behaviour they check"
)]

/// [`SideDataType`] against C's `enum AVFrameSideDataType`, as C observes it:
/// through `av_frame_side_data_name`'s descriptor table, and as the
/// `AVFrameSideData.type` a frame's side-data list stores, looks up and
/// removes by (buffer-frame wave 0, batch 2).
///
/// [`SideDataType`]: ffmpeg_bindings::libavutil::libavutil::frame::SideDataType
mod side_data_type {
    use core::ffi::CStr;

    use ffi::AVFrameSideDataType as C;
    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::frame::SideDataType;
    use ffmpeg_bindings::libavutil::libavutil::mem::{HeapVec, alloc_array_zeroed};

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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
            // For the unsafe operations below: `self.0` is a live frame; its side-data list holds
            // `nb_side_data` live entries, which only the C calls below
            // change, none of which runs while this borrow reads them.
            {
                // SAFETY: as the comment above this block states.
                let n = unsafe { (*self.0).nb_side_data };
                // SAFETY: as the comment above this block states.
                let list = unsafe { (*self.0).side_data };
                (0..usize::try_from(n).unwrap())
                    .map(|i| {
                        // SAFETY: as the comment above this block states, for entry `i < n`.
                        let at = unsafe { list.add(i) };
                        // SAFETY: as above; the entry is initialised.
                        let sd = unsafe { at.read() };
                        // SAFETY: as above; `sd` is a live entry.
                        SideDataType::from_raw(unsafe { (*sd).type_ })
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
            let stored = unsafe { (*sd).type_ };
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
            let raw = unsafe { (*sd).type_ };
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
            unsafe { ffi::av_frame_free(&raw mut self.0) }
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
            // SAFETY: as the comment below states, for this part of the operation.
            let ptr_add = unsafe { ptr.add(i) };
            // SAFETY: `SideDataType` is `#[repr(transparent)]` over `C`, and
            // `i` is in bounds of the live, initialised array.
            let raw = unsafe { ptr_add.read() };
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
/// [`ActiveFormatDescription`]: ffmpeg_bindings::libavutil::libavutil::frame::ActiveFormatDescription
mod active_format_description {
    use core::ptr::addr_of;

    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::frame::ActiveFormatDescription as Afd;

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
            // For the unsafe operations below: `sd` is the live entry C just added to our frame;
            // its `data` is a writable `av_buffer_alloc` block of `size` bytes.
            {
                // SAFETY: as the comment above this block states.
                unsafe { assert_eq!((*sd).size, 1) };
                // SAFETY: as the comment below states, for this part of the operation.
                let data_4 = unsafe { (*sd).data };
                // SAFETY: as the comment above this block states.
                unsafe { data_4.write(byte) };
            }
        }

        /// Overwrites the AFD byte C finds on this frame.
        fn set_afd(&mut self, byte: u8) {
            let sd = self.afd_entry();
            // SAFETY: as the comment below states, for this part of the operation.
            let data_3 = unsafe { (*sd).data };
            // SAFETY: `sd` is a live entry of our frame with at least one
            // byte of `data`, which no Rust reference covers.
            unsafe { data_3.write(byte) }
        }

        /// The AFD byte C finds on this frame.
        fn afd(&self) -> u8 {
            let sd = self.afd_entry();
            // SAFETY: as the comment below states, for this part of the operation.
            let data_2 = unsafe { (*sd).data };
            // SAFETY: `sd` is a live entry of our frame with at least one
            // byte of `data`.
            unsafe { data_2.read() }
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
            // SAFETY: `sd` is to a live object, as the access below relies on,
            // so the place of its `size` field is in bounds; nothing is accessed here.
            let size_ptr = unsafe { addr_of!((*sd).size) };
            // SAFETY: `sd` is a live entry of our frame.
            assert!(unsafe { size_ptr.read() } >= 1);
            sd
        }
    }

    impl Drop for CFrame {
        fn drop(&mut self) {
            // SAFETY: `self.0` is a live frame we own; `av_frame_free`
            // releases it with its side data and NULLs the slot.
            unsafe { ffi::av_frame_free(&raw mut self.0) }
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
            let nibble = byte & 0xF;
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

    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::frame::{SideDataDescriptor, SideDataProps};

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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
                );
            }
        }

        /// The entries' types, sorted.
        fn types(&self) -> Vec<c_uint> {
            let mut v: Vec<c_uint> = (0..self.nb as usize)
                .map(|i| {
                    // SAFETY: as the comment below states, for this part of the operation.
                    let sd_add = unsafe { self.sd.add(i) };
                    // SAFETY: `i < nb`, so `sd[i]` is an initialised entry.
                    let sd = unsafe { sd_add.read() };
                    // SAFETY: as above; `sd` is a live entry C owns.
                    unsafe { (*sd).type_.0 }
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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

    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::frame::{RegionOfInterest, RegionOfInterestRef};
    use ffmpeg_bindings::libavutil::libavutil::rational::Rational;

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
        let mut x = 0x5EED_u64;
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
    #[cfg_attr(miri, ignore = "calls C")]
    fn side_data_arrays_count_and_read_as_c_readers_do() {
        let mut x = 0xA11CE_u64;
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
            // For the unsafe operations below: the entry's payload holds `size` bytes, room for `n`
            // elements at stride `SELF_SIZE`, written unaligned as C's
            // readers read them.
            {
                // SAFETY: as the comment above this block states.
                let data = unsafe { (*entry).data };
                // SAFETY: as the comment above this block states.
                unsafe { assert_eq!((*entry).size, size) };
                for (i, r) in regions.iter().enumerate() {
                    // SAFETY: as the comment above this block states: element `i < n` at
                    // stride `SELF_SIZE` is within the payload.
                    let at = unsafe { data.add(i * RegionOfInterest::SELF_SIZE as usize) };
                    // SAFETY: as above, written unaligned.
                    unsafe {
                        at.cast::<ffi::AVRegionOfInterest>()
                            .write_unaligned(r.into_raw());
                    };
                }
                // What `libx264.c` and `vf_showinfo.c` compute.
                // SAFETY: as the comment above this block states.
                let first = unsafe { data.cast::<ffi::AVRegionOfInterest>().read_unaligned() };
                let stride = first.self_size as usize;
                // SAFETY: as the comment above this block states.
                unsafe { assert_eq!((*entry).size / stride, n) };
                for (i, r) in regions.iter().enumerate() {
                    // SAFETY: as the comment above this block states, at C's stride.
                    let at = unsafe { data.add(i * stride) };
                    // SAFETY: as above; the element was written above.
                    let c = unsafe { at.cast::<ffi::AVRegionOfInterest>().read_unaligned() };
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
            let mut x = seed * 0x9E37_79B9;
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
    #[cfg_attr(miri, ignore = "calls C")]
    fn equality_agrees_with_av_cmp_q_on_offsets() {
        let mut x = 0xE0_u64;
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
/// [`Frame`]: ffmpeg_bindings::libavutil::libavutil::frame::Frame
mod frame_type {
    use core::ffi::{CStr, c_int};
    use core::ptr;

    use ffmpeg_bindings::libavutil::libavutil::buffer::{BufferReference, BufferReferenceBox};
    use ffmpeg_bindings::libavutil::libavutil::channel_layout::ChannelLayout;
    use ffmpeg_bindings::libavutil::libavutil::dict::Dict;
    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::frame::{
        DecodeErrorFlags, Frame, FrameBox, FrameFlags, FrameRef, NUM_DATA_POINTERS, PlaneError,
        SideDataType,
    };
    use ffmpeg_bindings::libavutil::libavutil::pixdesc::{
        AlphaMode, ChromaLocation, ColorPrimaries, ColorRange, ColorSpace, ColorTransfer,
    };
    use ffmpeg_bindings::libavutil::libavutil::rational::Rational;
    use ffmpeg_bindings::libavutil::libavutil::side_data::SideDataFlags;
    use ffmpeg_bindings::libavutil::libavutil::utils::PictureType;

    /// Where a plane pointer points: NULL, inside unified buffer `index`
    /// (`buf`, then `extended_buf`) at `offset`, or nowhere C allocated.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(super) enum At {
        Null,
        In(usize, usize),
        Stray,
    }

    /// What C sees of a frame.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub(super) struct Seen {
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
            // SAFETY: as the comment below states, for this part of the operation.
            let key_2 = unsafe { (*e).key };
            // SAFETY: as the comment below states, for this part of the operation.
            let from_ptr_2 = unsafe { CStr::from_ptr(key_2) };
            // SAFETY: as the comment below states, for this part of the operation.
            let value_2 = unsafe { (*e).value };
            // SAFETY: a live entry, whose key and value are C strings.
            let (k, v) = unsafe { (from_ptr_2, CStr::from_ptr(value_2)) };
            out.push((k.to_bytes().into(), v.to_bytes().into()));
        }
    }

    /// C's reading of a reference: bytes, count, writability.
    fn c_buf(b: *const ffi::AVBufferRef) -> (Vec<u8>, c_int, bool) {
        // For the unsafe operations below: a live reference whose window is initialised bytes.
        {
            // SAFETY: as the comment above this block states: reads of its window.
            let (data, size) = (unsafe { (*b).data }, unsafe { (*b).size });
            let bytes = if size == 0 {
                Vec::new()
            } else {
                // SAFETY: as the comment above this block states: `size` initialised bytes.
                unsafe { std::slice::from_raw_parts(data, size) }.to_vec()
            };
            // SAFETY: as the comment above this block states.
            (
                bytes,
                unsafe { ffi::av_buffer_get_ref_count(b) },
                unsafe { ffi::av_buffer_is_writable(b) } != 0,
            )
        }
    }

    /// The unified buffer list of a frame: `buf`, then `extended_buf`.
    ///
    /// # Safety
    ///
    /// `f` must be a live frame.
    pub(super) unsafe fn c_buffers(f: *const ffi::AVFrame) -> Vec<*mut ffi::AVBufferRef> {
        // SAFETY: the caller's live frame: a copy of its `buf` array.
        let mut v: Vec<_> = unsafe { (*f).buf }.to_vec();
        // SAFETY: as above: reads of its extra block and its count.
        let (ext, n) = (unsafe { (*f).extended_buf }, unsafe {
            (*f).nb_extended_buf
        });
        for i in 0..n.max(0) as usize {
            // SAFETY: entry `i < nb_extended_buf` of the frame's extra block.
            let at = unsafe { ext.add(i) };
            // SAFETY: as above; the entry is initialised.
            v.push(unsafe { *at });
        }
        v
    }

    /// Where `p` points among `bufs`.
    pub(super) fn c_at(bufs: &[*mut ffi::AVBufferRef], p: *const u8) -> At {
        if p.is_null() {
            return At::Null;
        }
        for (i, &b) in bufs.iter().enumerate() {
            if b.is_null() {
                continue;
            }
            // SAFETY: a live reference.
            let (data, size) = (unsafe { (*b).data }, unsafe { (*b).size });
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
    pub(super) unsafe fn c_seen(f: *const ffi::AVFrame) -> Seen {
        // For the unsafe operations below: the caller's live frame, every pointer of which is NULL
        // or live.
        {
            // SAFETY: as the comment above this block states.
            let bufs = unsafe { c_buffers(f) };
            // SAFETY: as the comment above this block states.
            let x = unsafe { &*f };
            // SAFETY: as the comment above this block states.
            let separate = unsafe { x.extended_data.cast_const() != (&raw const (*f).data).cast() };
            let extended = separate.then(|| {
                (0..x.ch_layout.nb_channels.max(0) as usize)
                    .map(|i| {
                        // SAFETY: as the comment above this block states: a separate
                        // `extended_data` holds one entry per channel.
                        let at = unsafe { x.extended_data.add(i) };
                        // SAFETY: as above; the entry is initialised.
                        c_at(&bufs, unsafe { *at })
                    })
                    .collect()
            });
            let side_data = (0..x.nb_side_data.max(0) as usize)
                .map(|i| {
                    // SAFETY: as the comment above this block states, for entry `i <
                    // nb_side_data`.
                    let at = unsafe { x.side_data.add(i) };
                    // SAFETY: as above; the entry is a live side data entry.
                    let e = unsafe { *at };
                    // SAFETY: as above: reads of the entry's fields.
                    let (kind, data, size, metadata) = (
                        unsafe { (*e).type_.0 },
                        unsafe { (*e).data },
                        unsafe { (*e).size },
                        unsafe { (*e).metadata },
                    );
                    let bytes = if size == 0 {
                        Vec::new()
                    } else {
                        // SAFETY: as above; `size` initialised bytes at `data`.
                        unsafe { std::slice::from_raw_parts(data, size) }.to_vec()
                    };
                    (kind, bytes, c_pairs(metadata))
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
    pub(super) fn rust_seen(f: &FrameBox) -> Seen {
        // SAFETY: the box's live frame, whose buffers hold initialised
        // bytes, read only.
        unsafe { c_seen(f.as_c_ptr()) }
    }

    /// The Rust getters' reading of a frame, in [`Seen`]'s terms.
    pub(super) fn getters(r: FrameRef<'_>) -> Seen {
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
        let pairs = |m: Option<ffmpeg_bindings::libavutil::libavutil::dict::DictionaryRef<'_>>| {
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
    pub(super) struct CFrame(pub(super) *mut ffi::AVFrame);

    impl CFrame {
        pub(super) fn new() -> Self {
            // SAFETY: no precondition.
            let f = unsafe { ffi::av_frame_alloc() };
            assert!(!f.is_null());
            Self(f)
        }

        pub(super) fn seen(&self) -> Seen {
            // SAFETY: the frame C built, whose buffers this test filled.
            unsafe { c_seen(self.0) }
        }

        pub(super) fn bufs(&self) -> Vec<*mut ffi::AVBufferRef> {
            // SAFETY: a live frame.
            unsafe { c_buffers(self.0) }
        }

        /// The C frame's `extended_data` entries this test may touch, and
        /// whether the array is separate.
        fn extended(&self) -> (*mut *mut u8, usize, bool) {
            // For the unsafe operations below: a live frame.
            {
                let f = self.0;
                // SAFETY: as the comment above this block states.
                let own = unsafe { (&raw mut (*f).data).cast::<*mut u8>() };
                // SAFETY: as the comment above this block states.
                let ext = unsafe { (*f).extended_data };
                // SAFETY: as the comment above this block states.
                unsafe {
                    if ext == own {
                        (ext, NUM_DATA_POINTERS, false)
                    } else {
                        (ext, (*f).ch_layout.nb_channels.max(0) as usize, true)
                    }
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
            for i in 0..NUM_DATA_POINTERS {
                // SAFETY: a live frame: a read of its plane `i`.
                if !keep(unsafe { (*self.0).data[i] }) {
                    // SAFETY: as above: the plane pointer is cleared.
                    unsafe { (*self.0).data[i] = ptr::null_mut() };
                }
            }
            let (ext, n, separate) = self.extended();
            if separate {
                for i in 0..n {
                    // SAFETY: entry `i < n` of the frame's own separate `extended_data`.
                    let at = unsafe { ext.add(i) };
                    // SAFETY: as above; the entry is initialised.
                    if !keep(unsafe { *at }) {
                        // SAFETY: as above: the entry is cleared.
                        unsafe { *at = ptr::null_mut() };
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
            let (data, size) = (unsafe { (*buf).data }, unsafe { (*buf).size });
            if offset >= size {
                return Err(PlaneError::OutOfBounds);
            }
            Ok(data.wrapping_add(offset))
        }
    }

    impl Drop for CFrame {
        fn drop(&mut self) {
            // SAFETY: frees the frame and everything it holds.
            unsafe { ffi::av_frame_free(&raw mut self.0) }
        }
    }

    /// `len` bytes from `seed`.
    pub(super) fn pattern(len: usize, seed: u8) -> Vec<u8> {
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
        // For the unsafe operations below: a fresh reference to `len` bytes, filled from `bytes`.
        {
            // SAFETY: as the comment above this block states.
            let b = unsafe { ffi::av_buffer_alloc(bytes.len()) };
            assert!(!b.is_null());
            // SAFETY: as the comment below states, for this part of the operation.
            let data_2 = unsafe { (*b).data };
            // SAFETY: as the comment above this block states.
            unsafe { ptr::copy_nonoverlapping(bytes.as_ptr(), data_2, bytes.len()) };
            b
        }
    }

    /// A C dictionary of `pairs`.
    pub(super) fn c_dict(pairs: &[(&CStr, &CStr)]) -> *mut ffi::AVDictionary {
        let mut m = ptr::null_mut();
        for (k, v) in pairs {
            // SAFETY: C copies the strings into `m`.
            let r = unsafe {
                ffi::av_dict_set(
                    &raw mut m,
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
            self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
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
                let (x, y) = (
                    unsafe { ffi::av_frame_get_plane_buffer(rc.as_c_ptr(), i as c_int) },
                    unsafe { ffi::av_frame_get_plane_buffer(cc.0, i as c_int) },
                );
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
                let (data, size) = (unsafe { (*pb).data }, unsafe { (*pb).size });
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
    #[cfg_attr(miri, ignore = "calls C")]
    fn new_frames_match_av_frame_alloc() {
        let r = Frame::new().unwrap();
        let c = CFrame::new();
        assert_eq!(compare(&r, &c, "new"), (0, false));
        assert!(r.as_ref().try_clone().is_none());
        // For the unsafe operations below: both live; C's defaults put `extended_data` at `data`.
        {
            let (x, y) = (r.as_c_ptr(), c.0);
            // SAFETY: `x` is to a live object, as the block below relies on, so the
            // place of its `data` field is in bounds; nothing is accessed here.
            let data_ptr = unsafe { &raw mut (*x).data };
            // SAFETY: as the comment above this block states.
            unsafe { assert_eq!((*x).extended_data, (data_ptr).cast()) };
            // SAFETY: `y` is to a live object, as the block below relies on, so the
            // place of its `data` field is in bounds; nothing is accessed here.
            let data_ptr = unsafe { &raw mut (*y).data };
            // SAFETY: as the comment above this block states.
            unsafe { assert_eq!((*y).extended_data, (data_ptr).cast()) };
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
                // For the unsafe operations below: the frame's slot, which takes the new reference;
                // the old one is released after the planes are pruned, as
                // the Rust side's drop of the reference it returns is.
                let mut was = {
                    // SAFETY: as the comment above this block states.
                    let was = unsafe { (*f).buf[i] };
                    // SAFETY: as the comment above this block states.
                    unsafe {
                        (*f).buf[i] = if none {
                            ptr::null_mut()
                        } else {
                            c_buffer(&bytes)
                        }
                    };
                    was
                };
                c.prune();
                drop(old);
                // SAFETY: the reference the frame held.
                unsafe { ffi::av_buffer_unref(&raw mut was) };
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
                // For the unsafe operations below: the frame's extra block, grown by one reference.
                {
                    // SAFETY: as the comment above this block states.
                    let n = unsafe { (*f).nb_extended_buf as usize };
                    // SAFETY: as the comment above this block states: a read of the block.
                    let block = unsafe { (*f).extended_buf };
                    // SAFETY: as the comment above this block states: the block grows by one.
                    let grown =
                        unsafe { ffi::av_realloc_array(block.cast(), n + 1, size_of_ptr()) };
                    assert!(!grown.is_null());
                    // SAFETY: as the comment above this block states.
                    unsafe { (*f).extended_buf = grown.cast() };
                    // SAFETY: as the comment below states, for this part of the operation.
                    let extended_buf_2 = unsafe { (*f).extended_buf };
                    // SAFETY: as the comment below states, for this part of the operation.
                    let extended_buf_2_add = unsafe { extended_buf_2.add(n) };
                    // SAFETY: as the comment above this block states.
                    unsafe { *extended_buf_2_add = c_buffer(&bytes) };
                    // SAFETY: as the comment above this block states.
                    unsafe { (*f).nb_extended_buf += 1 };
                }
                "push extended".into()
            }
            5 => {
                let x = r.as_mut().pop_extended_buffer();
                // For the unsafe operations below: the frame's extra block, shrunk by one; the
                // reference taken out is released after the planes are
                // pruned, as the Rust side's is.
                let mut y = {
                    // SAFETY: as the comment above this block states.
                    let n = unsafe { (*f).nb_extended_buf as usize };
                    // SAFETY: `f` is to a live object, as the block below relies on, so the
                    // place of its `extended_buf` field is in bounds; nothing is accessed here.
                    let extended_buf_ptr = unsafe { &raw mut (*f).extended_buf };
                    if n == 0 {
                        ptr::null_mut()
                    } else {
                        // SAFETY: as the comment above this block states: a read of the block.
                        let block = unsafe { (*f).extended_buf };
                        // SAFETY: as above, its last entry, `n - 1 < n`.
                        let at = unsafe { block.add(n - 1) };
                        // SAFETY: as above; the entry is initialised.
                        let last = unsafe { *at };
                        // SAFETY: as above: the count drops by one.
                        unsafe { (*f).nb_extended_buf -= 1 };
                        if n == 1 {
                            // SAFETY: as above: the emptied block is freed and NULLed.
                            unsafe { ffi::av_freep(extended_buf_ptr.cast()) };
                        }
                        last
                    }
                };
                c.prune();
                assert_eq!(x.is_some(), !y.is_null());
                drop(x);
                // SAFETY: the reference taken out, or NULL.
                unsafe { ffi::av_buffer_unref(&raw mut y) };
                "pop extended".into()
            }
            6 => {
                let i = rng.below(4);
                let bytes = pattern(rng.below(16) + 1, 3);
                let x = r.as_mut().replace_extended_buffer(i, rust_buffer(&bytes));
                // SAFETY: a live frame: reads of its extra block and count.
                let (block, n) = (unsafe { (*f).extended_buf }, unsafe {
                    (*f).nb_extended_buf
                });
                // The old reference is released after the planes are pruned.
                let mut y = if i < n as usize {
                    // SAFETY: entry `i < nb_extended_buf` of the frame's extra block.
                    let slot = unsafe { block.add(i) };
                    // SAFETY: as above; the entry is initialised, and takes the new reference.
                    unsafe { slot.replace(c_buffer(&bytes)) }
                } else {
                    ptr::null_mut()
                };
                c.prune();
                assert_eq!(x.is_ok(), !y.is_null());
                drop(x);
                // SAFETY: the reference taken out, or NULL.
                unsafe { ffi::av_buffer_unref(&raw mut y) };
                format!("replace extended {i}")
            }
            7 => {
                let n = rng.below(12) as i32;
                let x = r.as_mut().set_channel_layout(ChannelLayout::unspecified(n));
                let (_, _, separate) = c.extended();
                // SAFETY: `f` is to a live object, as the block below relies on, so the
                // place of its `ch_layout` field is in bounds; nothing is accessed here.
                let ch_layout_ptr = unsafe { &raw mut (*f).ch_layout };
                // SAFETY: a read of the live frame's channel count.
                let channels = unsafe { (*f).ch_layout.nb_channels };
                let y = if separate && n > channels {
                    false
                } else {
                    // SAFETY: the frame's layout, unspecified order (no map).
                    unsafe { ffi::av_channel_layout_uninit(ch_layout_ptr) };
                    // SAFETY: as above, a write of its count.
                    unsafe { (*f).ch_layout.nb_channels = n };
                    true
                };
                assert_eq!(x.is_ok(), y);
                format!("layout {n}")
            }
            8 => {
                let x = r.as_mut().alloc_extended_data();
                let (ext, _, separate) = c.extended();
                // For the unsafe operations below: the frame's layout and `extended_data`, which
                // takes a fresh block copying `data` when `buf[0]` is set.
                let y = {
                    // SAFETY: as the comment above this block states.
                    let n = unsafe { (*f).ch_layout.nb_channels };
                    if n <= 0 {
                        Err(-(ffi::EINVAL as c_int))
                    } else {
                        // SAFETY: as the comment above this block states: a fresh block.
                        let block =
                            unsafe { ffi::av_calloc(n as usize, size_of_ptr()) }.cast::<*mut u8>();
                        // SAFETY: as above: a read of `buf[0]`.
                        let counted = !unsafe { (*f).buf[0] }.is_null();
                        for i in 0..(n as usize).min(NUM_DATA_POINTERS) {
                            if counted {
                                // SAFETY: as above: entry `i < n` of the fresh block.
                                let at = unsafe { block.add(i) };
                                // SAFETY: as above: a read of plane `i` of the frame.
                                let plane = unsafe { (*f).data[i] };
                                // SAFETY: as above: the plane, copied in.
                                unsafe { *at = plane };
                            }
                        }
                        if separate {
                            // SAFETY: as above: the frame's separate block, which it owns.
                            unsafe { ffi::av_free(ext.cast()) };
                        }
                        // SAFETY: as above: the frame takes the fresh block.
                        unsafe { (*f).extended_data = block };
                        Ok(())
                    }
                };
                assert_eq!(x, y);
                "alloc extended data".into()
            }
            9 => {
                r.as_mut().reset_extended_data();
                let (ext, _, separate) = c.extended();
                // For the unsafe operations below: the frame's own block, freed, and its `data`.
                {
                    // SAFETY: as the comment above this block states.
                    unsafe {
                        if separate {
                            ffi::av_free(ext.cast());
                        }
                    }
                    // SAFETY: `f` is to a live object, as the block below relies on, so the
                    // place of its `data` field is in bounds; nothing is accessed here.
                    let data_ptr = unsafe { &raw mut (*f).data };
                    // SAFETY: as the comment above this block states.
                    unsafe { (*f).extended_data = (data_ptr).cast() };
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
                        // SAFETY: as the comment below states, for this part of the operation.
                        let ext_add = unsafe { ext.add(i) };
                        // SAFETY: entry `i < n` of `extended_data`.
                        unsafe { *ext_add = t };
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
                // For the unsafe operations below: the frame's side data variables and a reference
                // C takes on success.
                let y = {
                    // SAFETY: `f` is to a live object, as the block below relies on, so the
                    // place of its `nb_side_data` field is in bounds; nothing is accessed here.
                    let nb_side_data_ptr = unsafe { &raw mut (*f).nb_side_data };
                    // SAFETY: `f` is to a live object, as the block below relies on, so the
                    // place of its `side_data` field is in bounds; nothing is accessed here.
                    let side_data_ptr = unsafe { &raw mut (*f).side_data };
                    // SAFETY: as the comment above this block states.
                    let e = unsafe {
                        ffi::av_frame_side_data_add(
                            side_data_ptr,
                            nb_side_data_ptr,
                            ffi::AVFrameSideDataType(kind),
                            &raw mut slot,
                            0,
                        )
                    };
                    // SAFETY: as the comment above this block states.
                    unsafe { ffi::av_buffer_unref(&raw mut slot) };
                    !e.is_null()
                };
                assert_eq!(x, y);
                format!("side data {kind}")
            }
            12 => {
                let n = rng.below(4);
                r.as_mut()
                    .set_metadata(Dict::from_entries(PAIRS[..n].iter().copied()).unwrap());
                // For the unsafe operations below: the frame's dictionary, replaced.
                {
                    // SAFETY: `f` is to a live object, as the block below relies on, so the
                    // place of its `metadata` field is in bounds; nothing is accessed here.
                    let metadata_ptr = unsafe { &raw mut (*f).metadata };
                    // SAFETY: as the comment above this block states.
                    unsafe { ffi::av_dict_free(metadata_ptr) };
                    // SAFETY: as the comment above this block states.
                    unsafe { (*f).metadata = c_dict(&PAIRS[..n]) };
                }
                format!("metadata {n}")
            }
            13 => {
                let none = rng.below(3) == 0;
                let bytes = pattern(5, 4);
                r.as_mut()
                    .set_opaque_ref((!none).then(|| rust_buffer(&bytes)));
                // For the unsafe operations below: the frame's user data reference, replaced.
                {
                    // SAFETY: `f` is to a live object, as the block below relies on, so the
                    // place of its `opaque_ref` field is in bounds; nothing is accessed here.
                    let opaque_ref_ptr = unsafe { &raw mut (*f).opaque_ref };
                    // SAFETY: as the comment above this block states.
                    unsafe { ffi::av_buffer_unref(opaque_ref_ptr) };
                    // SAFETY: as the comment above this block states.
                    unsafe {
                        if !none {
                            (*f).opaque_ref = c_buffer(&bytes);
                        }
                    }
                }
                format!("opaque_ref {none}")
            }
            14 => {
                let p = rng.below(NUM_DATA_POINTERS as u64);
                let byte = rng.next() as u8;
                let x = r.as_mut().plane_mut(p).map(|v| v.fill(byte)).is_some();
                let bufs = c.bufs();
                // SAFETY: a read of plane `p` of the live frame.
                let plane = unsafe { (*f).data[p] };
                // The plane is written as `plane_mut` does when its buffer is writable.
                let y = match c_at(&bufs, plane) {
                    // SAFETY: `bufs[b]` is a live reference of the frame.
                    At::In(b, off)
                        if !bufs[0].is_null()
                            // SAFETY: as the comment above this statement states.
                            && unsafe { ffi::av_buffer_is_writable(bufs[b]) } != 0 =>
                    {
                        // SAFETY: as above: a read of its size.
                        let size = unsafe { (*bufs[b]).size };
                        // SAFETY: the plane lies `off` bytes into the writable reference.
                        unsafe { ptr::write_bytes(plane, byte, size - off) };
                        true
                    }
                    _ => false,
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
                m.set_crop_right(v as usize & 0xFFFF);
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
                    x.crop_right = v as usize & 0xFFFF;
                    x.duration = v >> 4;
                    x.alpha_mode = ffi::AVAlphaMode(w as u32 % 3);
                    x.linesize[w as usize & 7] = w;
                    x.opaque = ptr::without_provenance_mut(v as usize);
                }
                "scalars".into()
            }
            16 => {
                let mut m = r.as_mut();
                m.set_pixel_format(
                    ffmpeg_bindings::libavutil::libavutil::pixdesc::PixelFormat::GRAY8,
                );
                m.set_width(4);
                m.set_height(2);
                m.set_sample_count(0);
                m.set_linesize(0, 4);
                // For the unsafe operations below: plain fields of the live frame.
                {
                    // SAFETY: as the comment above this block states.
                    unsafe { (*f).format = ffi::AVPixelFormat::AV_PIX_FMT_GRAY8.0 };
                    // SAFETY: as the comment above this block states.
                    unsafe { (*f).width = 4 };
                    // SAFETY: as the comment above this block states.
                    unsafe { (*f).height = 2 };
                    // SAFETY: as the comment above this block states.
                    unsafe { (*f).nb_samples = 0 };
                    // SAFETY: as the comment above this block states.
                    unsafe { (*f).linesize[0] = 4 };
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
    pub(super) fn shape(mut seen: Seen) -> Seen {
        for b in seen.buffers.iter_mut().flatten() {
            b.0 = (b.0.len() as u64).to_le_bytes().to_vec();
        }
        seen
    }

    /// The size of a pointer, which C's arrays of planes and references
    /// hold.
    fn size_of_ptr() -> usize {
        size_of::<*mut u8>()
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn random_sequences_read_alike_after_every_step() {
        // How many steps saw planes placed in buffers, and clones made, so
        // that the sequences are known to reach both.
        let (mut placed, mut cloned) = (0, 0);
        for seed in 1..=40u64 {
            let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
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
                unsafe { ffi::av_buffer_unref(&raw mut y) };
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
    #[cfg_attr(miri, ignore = "calls C")]
    fn clones_copy_external_planes_alike() {
        for (w, h) in [(1, 1), (7, 3), (32, 32)] {
            let mut mem = pattern((w * h) as usize, 5);
            let mut r = Frame::new().unwrap();
            let c = CFrame::new();
            let mut m = r.as_mut();
            m.set_pixel_format(ffmpeg_bindings::libavutil::libavutil::pixdesc::PixelFormat::GRAY8);
            m.set_width(w);
            m.set_height(h);
            m.set_linesize(0, w);
            // SAFETY: the frame holds no buffer; `mem` is the whole plane at
            // stride `w` and outlives both frames; nothing writes it.
            unsafe { m.set_data_ptr(0, mem.as_mut_ptr()) };
            // For the unsafe operations below: the same fields of the C frame.
            {
                // SAFETY: as the comment above this block states.
                unsafe { (*c.0).format = ffi::AVPixelFormat::AV_PIX_FMT_GRAY8.0 };
                // SAFETY: as the comment above this block states.
                unsafe { (*c.0).width = w };
                // SAFETY: as the comment above this block states.
                unsafe { (*c.0).height = h };
                // SAFETY: as the comment above this block states.
                unsafe { (*c.0).linesize[0] = w };
                // SAFETY: as the comment above this block states.
                unsafe { (*c.0).data[0] = mem.as_mut_ptr() };
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
                // SAFETY: as the comment below states, for this part of the operation.
                let c_plane_add = unsafe { c_plane.add(y * line) };
                // SAFETY: line `y` of the C copy, `w` bytes C wrote.
                let c_line = unsafe { std::slice::from_raw_parts(c_plane_add, w) };
                assert_eq!(c_line, &plane[y * line..y * line + w]);
                let pad = &plane[y * line + w..(y + 1) * line];
                assert_eq!(pad, &vec![0; pad.len()][..]);
            }
            let rest = &plane[h as usize * line..];
            assert_eq!(rest, &vec![0; rest.len()][..]);
            drop((r, c));
        }
    }

    /// Planar audio whose channels are external memory, lent in `data` (up
    /// to [`NUM_DATA_POINTERS`] channels) or in an `extended_data` array of
    /// its own: both clones copy every channel into buffers of their own,
    /// alike. (With more channels and no array of its own, C reads plane
    /// pointers past `data`, and the Rust clone is refused: see
    /// `ub_unsafe/frame.rs::frame_type::clones_refuse_channels_past_data`.)
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn clones_copy_external_channels_alike() {
        use ffmpeg_bindings::libavutil::libavutil::samplefmt::SampleFormat;
        for channels in [1usize, 2, NUM_DATA_POINTERS, NUM_DATA_POINTERS + 1, 13] {
            for samples in [1usize, 47] {
                let mut mems: Vec<Vec<u8>> = (0..channels)
                    .map(|c| pattern(samples, c as u8 * 3 + 1))
                    .collect();
                let separate = channels > NUM_DATA_POINTERS;
                let mut r = Frame::new().unwrap();
                let c = CFrame::new();
                let mut m = r.as_mut();
                m.set_sample_format(SampleFormat::U8P);
                m.set_sample_count(samples as c_int);
                m.set_linesize(0, samples as c_int);
                m.set_channel_layout(ChannelLayout::unspecified(channels as c_int))
                    .unwrap();
                if separate {
                    m.alloc_extended_data().unwrap();
                }
                for (i, mem) in mems.iter_mut().enumerate() {
                    // SAFETY: the frame holds no buffer; `mem` holds the
                    // channel's samples and outlives both frames; nothing
                    // writes it.
                    unsafe { m.set_extended_data_ptr(i, mem.as_mut_ptr()) };
                }
                // For the unsafe operations below: the same fields of the C frame; its separate
                // array is C's to free, as `get_audio_buffer` allocates one.
                {
                    let f = c.0;
                    // SAFETY: as the comment above this block states.
                    unsafe { (*f).format = ffi::AVSampleFormat::AV_SAMPLE_FMT_U8P.0 };
                    // SAFETY: as the comment above this block states.
                    unsafe { (*f).nb_samples = samples as c_int };
                    // SAFETY: as the comment above this block states.
                    unsafe { (*f).linesize[0] = samples as c_int };
                    // SAFETY: as the comment above this block states.
                    unsafe { (*f).ch_layout.nb_channels = channels as c_int };
                    if separate {
                        // SAFETY: as the comment above this block states: a fresh block.
                        let block = unsafe { ffi::av_calloc(channels, size_of_ptr()) };
                        assert!(!block.is_null());
                        // SAFETY: as above: the frame takes it.
                        unsafe { (*f).extended_data = block.cast() };
                    }
                    // SAFETY: as the comment above this block states: a read of the array.
                    let ext = unsafe { (*f).extended_data };
                    for (i, mem) in mems.iter_mut().enumerate() {
                        // SAFETY: as above: entry `i` of one per channel.
                        let at = unsafe { ext.add(i) };
                        // SAFETY: as above.
                        unsafe { *at = mem.as_mut_ptr() };
                    }
                }
                let what = format!("{channels} channels of {samples}");
                assert_eq!(rust_seen(&r), c.seen(), "{what}");
                let rc = r.as_ref().try_clone().expect("C copies the channels");
                // SAFETY: a live frame, read only.
                let cc = CFrame(unsafe { ffi::av_frame_clone(c.0) });
                assert!(!cc.0.is_null(), "{what}");
                assert_eq!(shape(rust_seen(&rc)), shape(cc.seen()), "{what}");
                for (i, mem) in mems.iter().enumerate() {
                    let plane = rc.as_ref().extended_plane(i).unwrap();
                    // SAFETY: as the comment below states, for this part of the operation.
                    let extended_data_2 = unsafe { (*cc.0).extended_data };
                    // SAFETY: as the comment below states, for this part of the operation.
                    let extended_data_2_add = unsafe { extended_data_2.add(i) };
                    // SAFETY: as below, entry `i` of the copy's array, initialised.
                    let channel = unsafe { *extended_data_2_add };
                    // SAFETY: channel `i` of the C copy, `samples` bytes C
                    // wrote.
                    let c_plane = unsafe { std::slice::from_raw_parts(channel, samples) };
                    assert_eq!(&plane[..samples], &mem[..], "{what} ({i})");
                    assert_eq!(c_plane, &mem[..], "{what} ({i})");
                    let rest = &plane[samples..];
                    assert_eq!(rest, &vec![0; rest.len()][..], "{what} ({i})");
                }
                drop((r, c));
            }
        }
    }
}

/// [`Frame`]'s routines against C's (buffer-frame wave 4, batch 0), on
/// independently owned frames built alike, compared after every step by
/// C's own reading of both frames (every field, where each plane points
/// among the buffers, the buffers' bytes, counts and writability, side data
/// and metadata) and by the Rust getters: buffer allocation against
/// `av_frame_get_buffer` (whose bytes the C side zeroes, as the wrapper
/// does), plane copies against `av_frame_copy`, property copies against
/// `av_frame_copy_props`, references, replacements and moves against
/// `av_frame_ref`, `av_frame_replace` and `av_frame_move_ref` (the C
/// destination reset first, as the wrappers reset theirs), writability
/// against `av_frame_make_writable` and `av_frame_is_writable`, cropping
/// against `av_frame_apply_cropping`, plane buffer lookups against
/// `av_frame_get_plane_buffer`, and side data additions, lookups and
/// removals against `av_frame_new_side_data`,
/// `av_frame_new_side_data_from_buf`, `av_frame_get_side_data` and
/// `av_frame_remove_side_data`.
///
/// [`Frame`]: ffmpeg_bindings::libavutil::libavutil::frame::Frame
mod frame_routines {
    use core::ffi::{CStr, c_int};
    use core::ptr;

    use ffmpeg_bindings::libavutil::libavutil::buffer::BufferReference;
    use ffmpeg_bindings::libavutil::libavutil::channel_layout::ChannelLayout;
    use ffmpeg_bindings::libavutil::libavutil::dict::Dict;
    use ffmpeg_bindings::libavutil::libavutil::frame::{
        CropFlags, Frame, FrameBox, FrameRef, SideDataType,
    };
    use ffmpeg_bindings::libavutil::libavutil::pixdesc::PixelFormat;
    use ffmpeg_bindings::libavutil::libavutil::samplefmt::SampleFormat;
    use ffmpeg_bindings::libavutil::libavutil::{ffi, imgutils};

    use super::frame_type::{At, CFrame, c_at, c_dict, getters, pattern, rust_seen, shape};

    const EINVAL: c_int = -22;

    /// A constructor of a [`Pair`].
    type Make = fn() -> Pair;

    /// A Rust frame and a C frame, built alike.
    struct Pair {
        r: FrameBox,
        c: CFrame,
    }

    impl Pair {
        /// Two new frames.
        fn new() -> Self {
            Self {
                r: Frame::new().unwrap(),
                c: CFrame::new(),
            }
        }

        /// Two video frames of `format`, `w` x `h`, without buffers.
        fn video(format: PixelFormat, w: c_int, h: c_int) -> Self {
            let mut p = Self::new();
            {
                let mut m = p.r.as_mut();
                m.set_pixel_format(format);
                m.set_width(w);
                m.set_height(h);
            }
            // For the unsafe operations below: the same fields of the live C frame.
            {
                // SAFETY: as the comment above this block states.
                unsafe { (*p.c.0).format = format.into_raw().0 };
                // SAFETY: as the comment above this block states.
                unsafe { (*p.c.0).width = w };
                // SAFETY: as the comment above this block states.
                unsafe { (*p.c.0).height = h };
            }
            p
        }

        /// Two audio frames of `format`, `channels` x `samples` (an
        /// unspecified layout), without buffers.
        fn audio(format: SampleFormat, channels: c_int, samples: c_int) -> Self {
            let mut p = Self::new();
            let mut m = p.r.as_mut();
            m.set_sample_format(format);
            m.set_sample_count(samples);
            m.set_channel_layout(ChannelLayout::unspecified(channels))
                .unwrap();
            // For the unsafe operations below: the same fields of the live C frame; an unspecified
            // layout has no map.
            {
                // SAFETY: as the comment above this block states.
                unsafe { (*p.c.0).format = format.into_raw().0 };
                // SAFETY: as the comment above this block states.
                unsafe { (*p.c.0).nb_samples = samples };
                // SAFETY: as the comment above this block states.
                unsafe { (*p.c.0).ch_layout.order = ffi::AVChannelOrder::AV_CHANNEL_ORDER_UNSPEC };
                // SAFETY: as the comment above this block states.
                unsafe { (*p.c.0).ch_layout.nb_channels = channels };
            }
            p.check("audio");
            p
        }

        /// Allocates both frames' buffers at `align`, zeroing the C ones as
        /// the wrapper zeroes its own; both results.
        fn alloc(&mut self, align: usize) -> (Result<(), c_int>, c_int) {
            let r = self.r.as_mut().alloc_buffers(align);
            // SAFETY: a live frame holding no planes.
            let c = unsafe { ffi::av_frame_get_buffer(self.c.0, align as c_int) };
            if c >= 0 {
                c_zero(&self.c);
            }
            (r, c)
        }

        /// Allocates both frames' buffers at `align`, which both succeed in.
        fn allocated(&mut self, align: usize) {
            assert_eq!(self.alloc(align), (Ok(()), 0));
        }

        /// Fills, plane by plane in `extended_data` order, the bytes from each
        /// plane to the end of its buffer with `pattern(.., seed + i)`, on
        /// both sides.
        fn fill(&mut self, seed: u8) {
            let mut m = self.r.as_mut();
            for i in 0..m.as_ref().extended_plane_count() {
                if let Some(v) = m.extended_plane_mut(i) {
                    v.copy_from_slice(&pattern(v.len(), seed.wrapping_add(i as u8)));
                }
            }
            let bufs = self.c.bufs();
            let (ext, n) = c_extended(&self.c);
            for i in 0..n {
                // SAFETY: as the comment below states, for this part of the operation.
                let ext_add = unsafe { ext.add(i) };
                // SAFETY: entry `i` of the frame's `extended_data`, which
                // holds `n`.
                let p = unsafe { *ext_add };
                if let At::In(b, off) = c_at(&bufs, p) {
                    // For the unsafe operations below: a live reference the frame holds alone;
                    // `off` is inside its window.
                    {
                        // SAFETY: as the comment above this block states.
                        let size = unsafe { (*bufs[b]).size - off };
                        let bytes = pattern(size, seed.wrapping_add(i as u8));
                        // SAFETY: as the comment below states, for this part of the operation.
                        let data_3 = unsafe { (*bufs[b]).data };
                        // SAFETY: as the comment below states, for this part of the operation.
                        let data_3_add = unsafe { data_3.add(off) };
                        // SAFETY: as the comment above this block states.
                        unsafe { ptr::copy_nonoverlapping(bytes.as_ptr(), data_3_add, size) };
                    }
                }
            }
        }

        /// Asserts both frames read the same, to C and through the getters.
        fn check(&self, step: &str) {
            let seen = self.c.seen();
            assert_eq!(rust_seen(&self.r), seen, "{step}");
            assert_eq!(getters(self.r.as_ref()), seen, "{step} (getters)");
        }
    }

    /// The `extended_data` entries of a C frame, and how many it holds.
    fn c_extended(c: &CFrame) -> (*mut *mut u8, usize) {
        // For the unsafe operations below: a live frame.
        {
            let f = c.0;
            // SAFETY: as the comment above this block states.
            let own = unsafe { (&raw mut (*f).data).cast::<*mut u8>() };
            // SAFETY: as the comment above this block states.
            let ext = unsafe { (*f).extended_data };
            // SAFETY: as the comment above this block states.
            unsafe {
                if ext == own {
                    (ext, ffi::AV_NUM_DATA_POINTERS as usize)
                } else {
                    (ext, (*f).ch_layout.nb_channels.max(0) as usize)
                }
            }
        }
    }

    /// Zeroes every buffer of a C frame.
    fn c_zero(c: &CFrame) {
        for b in c.bufs().into_iter().filter(|b| !b.is_null()) {
            // SAFETY: as the comment below states, for this part of the operation.
            let data_2 = unsafe { (*b).data };
            // SAFETY: as the comment below states, for this part of the operation.
            let size_2 = unsafe { (*b).size };
            // SAFETY: a fresh reference the frame holds alone.
            unsafe { ptr::write_bytes(data_2, 0, size_2) };
        }
    }

    const VIDEO: [PixelFormat; 10] = [
        PixelFormat::GRAY8,
        PixelFormat::YUV420P,
        PixelFormat::YUV422P10,
        PixelFormat::NV12,
        PixelFormat::RGB24,
        PixelFormat::BGRA,
        PixelFormat::PAL8,
        PixelFormat::MONOWHITE,
        PixelFormat::YUVA444P16,
        PixelFormat::UYVY422,
    ];

    const AUDIO: [SampleFormat; 5] = [
        SampleFormat::U8,
        SampleFormat::S16P,
        SampleFormat::S32,
        SampleFormat::FLTP,
        SampleFormat::DBLP,
    ];

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn allocations_match_av_frame_get_buffer() {
        let mut n = 0;
        for format in VIDEO {
            for (w, h) in [(1, 1), (7, 3), (33, 17), (640, 2)] {
                for align in [0, 1, 2, 16, 32, 64] {
                    let mut p = Pair::video(format, w, h);
                    let (r, c) = p.alloc(align);
                    assert_eq!(r, Ok(()), "{format:?} {w}x{h} {align}");
                    assert_eq!(c, 0);
                    p.check(&format!("{format:?} {w}x{h} {align}"));
                    n += 1;
                }
                // Line sizes given beforehand.
                let mut p = Pair::video(format, w, h);
                p.r.as_mut().set_linesizes([256, 128, 128, 64, 0, 0, 0, 0]);
                // SAFETY: the same field of the live C frame.
                unsafe { (*p.c.0).linesize = [256, 128, 128, 64, 0, 0, 0, 0] };
                let (r, c) = p.alloc(0);
                assert_eq!(r.err().unwrap_or(0), c);
                if c >= 0 {
                    p.check(&format!("{format:?} {w}x{h} given"));
                }
            }
        }
        for format in AUDIO {
            for channels in [1, 2, 8, 9, 24] {
                for samples in [1, 47, 1000] {
                    for align in [0, 1, 4, 64] {
                        let mut p = Pair::audio(format, channels, samples);
                        let (r, c) = p.alloc(align);
                        assert_eq!((r, c), (Ok(()), 0));
                        p.check(&format!("{format:?} {channels}x{samples} {align}"));
                        n += 1;
                    }
                }
            }
        }
        // C's refusals.
        let refused: [Make; 7] = [
            || Pair::video(PixelFormat::NONE, 4, 4),
            || Pair::video(PixelFormat::from_raw(ffi::AVPixelFormat(100_000)), 4, 4),
            || Pair::video(PixelFormat::VAAPI, 4, 4),
            || Pair::video(PixelFormat::GRAY8, 1 << 20, 1 << 20),
            || Pair::video(PixelFormat::GRAY8, 0, 4),
            || Pair::audio(SampleFormat::U8, 0, 4),
            || Pair::audio(SampleFormat::NONE, 2, 4),
        ];
        for (i, make) in refused.into_iter().enumerate() {
            let mut p = make();
            let (r, c) = p.alloc(0);
            assert!(c < 0, "{i}");
            assert_eq!(r, Err(c), "{i}");
            p.check(&format!("refused {i}"));
        }
        assert!(n > 300);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn plane_copies_match_av_frame_copy() {
        let mut pairs = Vec::new();
        for format in VIDEO {
            for (w, h) in [(1, 1), (9, 5), (64, 3)] {
                pairs.push((Pair::video(format, w, h), Pair::video(format, w, h)));
            }
        }
        for format in AUDIO {
            for channels in [1, 3, 12] {
                pairs.push((
                    Pair::audio(format, channels, 33),
                    Pair::audio(format, channels, 33),
                ));
            }
        }
        for (i, (mut src, mut dst)) in pairs.into_iter().enumerate() {
            assert_eq!(src.alloc(0).1, 0);
            assert_eq!(dst.alloc(1).1, 0);
            src.fill(i as u8);
            let r = dst.r.as_mut().copy_data_from(src.r.as_ref());
            // SAFETY: two live frames; C reads `src`'s planes and writes
            // `dst`'s, which both hold buffers for.
            let c = unsafe { ffi::av_frame_copy(dst.c.0, src.c.0) };
            assert_eq!((r, c), (Ok(()), 0), "{i}");
            dst.check(&format!("copy {i}"));
            src.check(&format!("copy {i} (source)"));
            // Again, over the copy.
            src.fill(i as u8 + 1);
            let r = dst.r.as_mut().copy_data_from(src.r.as_ref());
            // SAFETY: as above.
            let c = unsafe { ffi::av_frame_copy(dst.c.0, src.c.0) };
            assert_eq!((r, c), (Ok(()), 0), "{i}");
            dst.check(&format!("copy {i} again"));
        }
        // C's refusals, which touch nothing.
        let refusals: [(Make, Make); 5] = [
            (
                || Pair::video(PixelFormat::GRAY8, 4, 4),
                || Pair::video(PixelFormat::PAL8, 4, 4),
            ),
            (
                || Pair::video(PixelFormat::GRAY8, 4, 4),
                || Pair::video(PixelFormat::GRAY8, 4, 3),
            ),
            (
                || Pair::audio(SampleFormat::S16, 2, 8),
                || Pair::audio(SampleFormat::S16, 2, 7),
            ),
            (
                || Pair::audio(SampleFormat::S16P, 2, 8),
                || Pair::audio(SampleFormat::S16P, 3, 8),
            ),
            (Pair::new, || Pair::video(PixelFormat::GRAY8, 4, 4)),
        ];
        for (i, (src, dst)) in refusals.into_iter().enumerate() {
            let (mut src, mut dst) = (src(), dst());
            let _ = src.alloc(0);
            let _ = dst.alloc(0);
            src.fill(3);
            let r = dst.r.as_mut().copy_data_from(src.r.as_ref());
            // SAFETY: two live frames C refuses to copy between.
            let c = unsafe { ffi::av_frame_copy(dst.c.0, src.c.0) };
            assert_eq!((r, c), (Err(EINVAL), EINVAL), "{i}");
            dst.check(&format!("refusal {i}"));
        }
        // A destination with a NULL plane.
        let (mut src, mut dst) = (
            Pair::video(PixelFormat::YUV420P, 4, 4),
            Pair::video(PixelFormat::YUV420P, 4, 4),
        );
        src.allocated(0);
        dst.allocated(0);
        dst.r.as_mut().clear_plane(2);
        // SAFETY: the same plane of the live C frame.
        unsafe { (*dst.c.0).data[2] = ptr::null_mut() };
        let r = dst.r.as_mut().copy_data_from(src.r.as_ref());
        // SAFETY: as above.
        let c = unsafe { ffi::av_frame_copy(dst.c.0, src.c.0) };
        assert_eq!((r, c), (Err(EINVAL), EINVAL));
    }

    /// Builds the same properties, metadata, side data and user data on both
    /// frames of `p`, `k` choosing the values.
    fn props(p: &mut Pair, k: i64) {
        let pairs: [(&CStr, &CStr); 2] = if k % 2 == 0 {
            [(c"a", c"1"), (c"B", c"2")]
        } else {
            [(c"b", c"3"), (c"c", c"4")]
        };
        let mut m = p.r.as_mut();
        m.set_pts(k);
        m.set_duration(k * 2);
        m.set_crop_top(k as usize);
        m.set_quality(k as c_int);
        m.set_metadata(Dict::from_entries(pairs).unwrap());
        m.alloc_side_data_from_slice(SideDataType::AFD, &[k as u8])
            .unwrap();
        m.alloc_side_data_from_slice(SideDataType::PAN_SCAN, &pattern(20, k as u8))
            .unwrap();
        m.set_opaque_ref(Some(
            BufferReference::alloc(2)
                .unwrap()
                .copy_from_slice(&[k as u8, 1]),
        ));
        let f = p.c.0;
        // For the unsafe operations below: the same fields of the live C frame; its side data
        // entries are written in full, and the frame takes the dictionary and the
        // reference.
        {
            // SAFETY: as the comment above this block states.
            unsafe { (*f).pts = k };
            // SAFETY: as the comment above this block states.
            unsafe { (*f).duration = k * 2 };
            // SAFETY: as the comment above this block states.
            unsafe { (*f).crop_top = k as usize };
            // SAFETY: as the comment above this block states.
            unsafe { (*f).quality = k as c_int };
            // SAFETY: `f` is to a live object, as the block below relies on, so the
            // place of its `metadata` field is in bounds; nothing is accessed here.
            let metadata_ptr = unsafe { &raw mut (*f).metadata };
            // SAFETY: as the comment above this block states.
            unsafe { ffi::av_dict_free(metadata_ptr) };
            // SAFETY: as the comment above this block states.
            unsafe { (*f).metadata = c_dict(&pairs) };
            // SAFETY: as the comment above this block states.
            let e = unsafe {
                ffi::av_frame_new_side_data(f, ffi::AVFrameSideDataType::AV_FRAME_DATA_AFD, 1)
            };
            // SAFETY: as the comment above this block states: a read of the new entry's
            // payload pointer.
            let payload = unsafe { (*e).data };
            // SAFETY: as the comment above this block states.
            unsafe { *payload = k as u8 };
            // SAFETY: as the comment above this block states.
            let e = unsafe {
                ffi::av_frame_new_side_data(f, ffi::AVFrameSideDataType::AV_FRAME_DATA_PANSCAN, 20)
            };
            // SAFETY: as the comment below states, for this part of the operation.
            let data_5 = unsafe { (*e).data };
            // SAFETY: as the comment above this block states.
            unsafe { ptr::copy_nonoverlapping(pattern(20, k as u8).as_ptr(), data_5, 20) };
            // SAFETY: `f` is to a live object, as the block below relies on, so the
            // place of its `opaque_ref` field is in bounds; nothing is accessed here.
            let opaque_ref_ptr = unsafe { &raw mut (*f).opaque_ref };
            // SAFETY: as the comment above this block states.
            unsafe { ffi::av_buffer_unref(opaque_ref_ptr) };
            // SAFETY: as the comment above this block states.
            let b = unsafe { ffi::av_buffer_alloc(2) };
            // SAFETY: as the comment above this block states: a read of the new reference's
            // two-byte window.
            let data = unsafe { (*b).data };
            // SAFETY: as above.
            unsafe { *data = k as u8 };
            // SAFETY: as above, its second byte.
            let second = unsafe { data.add(1) };
            // SAFETY: as above.
            unsafe { *second = 1 };
            // SAFETY: as the comment above this block states.
            unsafe { (*f).opaque_ref = b };
        }
        p.check(&format!("props {k}"));
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn property_copies_match_av_frame_copy_props() {
        let mut src = Pair::video(PixelFormat::GRAY8, 4, 4);
        props(&mut src, 7);
        for (w, h) in [(4, 4), (4, 5), (0, 0)] {
            let mut dst = Pair::video(PixelFormat::GRAY8, w, h);
            let _ = dst.alloc(0);
            props(&mut dst, 2);
            for step in 0..2 {
                let r = dst.r.as_mut().copy_props_from(src.r.as_ref());
                // SAFETY: two live frames.
                let c = unsafe { ffi::av_frame_copy_props(dst.c.0, src.c.0) };
                assert_eq!((r, c), (Ok(()), 0));
                dst.check(&format!("{w}x{h} copy {step}"));
                src.check(&format!("{w}x{h} copy {step} (source)"));
            }
        }
    }

    /// `av_frame_replace(d, s)`, after releasing `d`'s extra references
    /// when `s` has none, as `assign` does: C's own routine frees them but
    /// keeps their count (see `unit_tests::frame_assign_from_a_frame_without
    /// _extra_references_keeps_no_count`).
    fn c_replace(d: &CFrame, s: &CFrame) -> c_int {
        // For the unsafe operations below: two distinct live frames; the released references are
        // `d`'s own, and the count follows them.
        {
            let f = d.0;
            // SAFETY: `f` is to a live object, as the block below relies on, so the
            // place of its `extended_buf` field is in bounds; nothing is accessed here.
            let extended_buf_ptr = unsafe { &raw mut (*f).extended_buf };
            // SAFETY: as the comment above this block states: reads of the two extra blocks.
            let (src_ext, dst_ext) = (unsafe { (*s.0).extended_buf }, unsafe { (*f).extended_buf });
            if src_ext.is_null() && !dst_ext.is_null() {
                // SAFETY: as above: a read of `d`'s count.
                let n = unsafe { (*f).nb_extended_buf } as usize;
                for i in 0..n {
                    // SAFETY: as above, entry `i < n` of `d`'s block.
                    let at = unsafe { dst_ext.add(i) };
                    // SAFETY: as above; the entry is `d`'s own reference.
                    unsafe { ffi::av_buffer_unref(at) };
                }
                // SAFETY: as above: the emptied block is freed and NULLed.
                unsafe { ffi::av_freep(extended_buf_ptr.cast()) };
                // SAFETY: as above: the count follows the block.
                unsafe { (*f).nb_extended_buf = 0 };
            }
            // SAFETY: as the comment above this block states.
            unsafe { ffi::av_frame_replace(f, s.0) }
        }
    }

    /// Resets the C destination as the wrappers reset theirs.
    fn c_unref(c: &CFrame) {
        // SAFETY: a live frame.
        unsafe { ffi::av_frame_unref(c.0) };
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn references_replacements_and_moves_match_c() {
        let mut a = Pair::video(PixelFormat::YUV420P, 6, 4);
        a.allocated(0);
        a.fill(1);
        props(&mut a, 3);
        let mut b = Pair::audio(SampleFormat::S16P, 12, 5);
        b.allocated(0);
        b.fill(9);
        let mut bare = Pair::new();
        bare.r.as_mut().set_pts(11);
        // SAFETY: the same field of the live C frame.
        unsafe { (*bare.c.0).pts = 11 };
        let mut d = Pair::video(PixelFormat::GRAY8, 2, 2);
        d.allocated(0);
        let all = |a: &Pair, b: &Pair, d: &Pair, step: &str| {
            a.check(&format!("{step} (a)"));
            b.check(&format!("{step} (b)"));
            d.check(&format!("{step} (d)"));
        };
        // References.
        let r = d.r.as_mut().try_clone_from(a.r.as_ref());
        c_unref(&d.c);
        // SAFETY: a reset destination and a live source.
        let c = unsafe { ffi::av_frame_ref(d.c.0, a.c.0) };
        assert_eq!((r, c), (Ok(()), 0));
        all(&a, &b, &d, "ref a");
        // Replacements: the same source, another, a bare one.
        for (src, name) in [(&a, "a"), (&b, "b"), (&a, "a again"), (&bare, "bare")] {
            let r = d.r.as_mut().assign(src.r.as_ref());
            let c = c_replace(&d.c, &src.c);
            assert_eq!((r, c), (Ok(()), 0), "{name}");
            all(&a, &b, &d, &format!("replace {name}"));
        }
        // A reference to a frame without planes fails, resetting.
        let r = d.r.as_mut().try_clone_from(bare.r.as_ref());
        c_unref(&d.c);
        // SAFETY: as above.
        let c = unsafe { ffi::av_frame_ref(d.c.0, bare.c.0) };
        assert_eq!((r, c), (Err(EINVAL), EINVAL));
        all(&a, &b, &d, "ref bare");
        // Moves.
        d.r.as_mut().move_from(b.r.as_mut());
        c_unref(&d.c);
        // SAFETY: a reset destination and a distinct live source.
        unsafe { ffi::av_frame_move_ref(d.c.0, b.c.0) };
        all(&a, &b, &d, "move b");
        let e = Pair {
            r: d.r.as_mut().take().unwrap(),
            c: {
                let e = CFrame::new();
                // SAFETY: as above.
                unsafe { ffi::av_frame_move_ref(e.0, d.c.0) };
                e
            },
        };
        all(&a, &e, &d, "take");
        let r = d.r.as_mut().assign(e.r.as_ref());
        let c = c_replace(&d.c, &e.c);
        assert_eq!((r, c), (Ok(()), 0));
        all(&a, &e, &d, "replace e");
        drop(e);
        all(&a, &b, &d, "drop e");
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn replacements_of_extra_references_match_c() {
        let mut src = Pair::audio(SampleFormat::U8P, 10, 4);
        src.allocated(0);
        src.fill(1);
        let mut dst = Pair::audio(SampleFormat::U8P, 20, 4);
        dst.allocated(0);
        dst.fill(2);
        let r = dst.r.as_mut().assign(src.r.as_ref());
        let c = c_replace(&dst.c, &src.c);
        assert_eq!((r, c), (Ok(()), 0));
        dst.check("fewer extra references");
        src.check("fewer extra references (source)");
        // A separate `extended_data` counting no channel: C refuses it and
        // resets the frame.
        let mut none = Pair::audio(SampleFormat::U8P, 9, 4);
        none.allocated(0);
        none.r
            .as_mut()
            .set_channel_layout(ChannelLayout::unspecified(0))
            .unwrap();
        // SAFETY: the same field of the live C frame, whose separate array
        // stays allocated for nine channels.
        unsafe { (*none.c.0).ch_layout.nb_channels = 0 };
        none.check("no channel");
        let r = dst.r.as_mut().assign(none.r.as_ref());
        let c = c_replace(&dst.c, &none.c);
        assert_eq!((r, c), (Err(EINVAL), EINVAL));
        dst.check("no channel replaced");
        let r = dst.r.as_mut().try_clone_from(none.r.as_ref());
        c_unref(&dst.c);
        // SAFETY: a reset destination and a distinct live source.
        let c = unsafe { ffi::av_frame_ref(dst.c.0, none.c.0) };
        assert_eq!((r, c), (Err(EINVAL), EINVAL));
        dst.check("no channel referenced");
    }

    /// The bytes of every row C copies of each plane of a video frame, for
    /// frames whose padding C leaves uninitialised.
    fn rows_of(f: FrameRef<'_>) -> Vec<Vec<u8>> {
        let format = f.pixel_format();
        let (_, sub) = format.chroma_subsampling().unwrap();
        let mut out = Vec::new();
        for i in 0..format.plane_count().unwrap() {
            let len = imgutils::linesize(format, f.width(), i).unwrap() as usize;
            let ls = f.linesizes()[i] as usize;
            let h = if i == 1 || i == 2 {
                (f.height() as usize).div_ceil(1 << sub)
            } else {
                f.height() as usize
            };
            let plane = f.plane(i).unwrap();
            out.extend((0..h).map(|y| plane[y * ls..y * ls + len].to_vec()));
        }
        out
    }

    /// As [`rows_of`], for C's frame.
    fn c_rows_of(c: &CFrame, format: PixelFormat) -> Vec<Vec<u8>> {
        let (_, sub) = format.chroma_subsampling().unwrap();
        let mut out = Vec::new();
        // For the unsafe operations below: a live frame whose planes hold the rows C copied.
        {
            let f = c.0;
            // SAFETY: as the comment above this block states: reads of the frame's geometry.
            let (width, height) = (unsafe { (*f).width }, unsafe { (*f).height } as usize);
            for i in 0..format.plane_count().unwrap() {
                let len = imgutils::linesize(format, width, i).unwrap() as usize;
                // SAFETY: as above: reads of plane `i` and its line size.
                let (plane, ls) = (unsafe { (*f).data[i] }, unsafe { (*f).linesize[i] }
                    as usize);
                let h = if i == 1 || i == 2 {
                    height.div_ceil(1 << sub)
                } else {
                    height
                };
                for y in 0..h {
                    // SAFETY: as above: row `y < h` of the plane.
                    let at = unsafe { plane.add(y * ls) };
                    // SAFETY: as above: `len` bytes C copied into the row.
                    let row = unsafe { std::slice::from_raw_parts(at, len) };
                    out.push(row.to_vec());
                }
            }
        }
        out
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn make_writable_matches_av_frame_make_writable() {
        for format in [PixelFormat::GRAY8, PixelFormat::YUV420P, PixelFormat::RGB24] {
            let mut p = Pair::video(format, 9, 5);
            p.allocated(0);
            p.fill(4);
            props(&mut p, 1);
            // Writable: nothing to do.
            let r = p.r.as_mut().make_writable();
            // SAFETY: a live frame.
            let c = unsafe { ffi::av_frame_make_writable(p.c.0) };
            assert_eq!((r, c), (Ok(()), 0));
            p.check(&format!("{format:?} writable"));
            // Shared: both copy.
            let keep_r = p.r.as_ref().try_clone().unwrap();
            // SAFETY: a live frame, read only.
            let mut keep_c = unsafe { ffi::av_frame_clone(p.c.0) };
            let r = p.r.as_mut().make_writable();
            // SAFETY: a live frame.
            let c = unsafe { ffi::av_frame_make_writable(p.c.0) };
            assert_eq!((r, c), (Ok(()), 0));
            assert_eq!(shape(rust_seen(&p.r)), shape(p.c.seen()), "{format:?}");
            assert_eq!(rows_of(p.r.as_ref()), c_rows_of(&p.c, format), "{format:?}");
            // SAFETY: a live frame.
            let c = unsafe { ffi::av_frame_is_writable(p.c.0) };
            assert_eq!(p.r.as_ref().is_writable(), c != 0);
            drop(keep_r);
            // SAFETY: the clone this test made.
            unsafe { ffi::av_frame_free(&raw mut keep_c) };
        }
        // Lent planes are copied alike.
        let mut mem = pattern(5 * 3, 2);
        let mut p = Pair::video(PixelFormat::GRAY8, 5, 3);
        let mut m = p.r.as_mut();
        m.set_linesize(0, 5);
        // SAFETY: the frame holds no buffer; `mem` holds the whole plane,
        // outlives both frames and is written by nothing else.
        unsafe { m.set_data_ptr(0, mem.as_mut_ptr()) };
        let r = m.make_writable();
        // For the unsafe operations below: the same fields of the live C frame; `mem` as above.
        let c = {
            // SAFETY: as the comment above this block states.
            unsafe { (*p.c.0).linesize[0] = 5 };
            // SAFETY: as the comment above this block states.
            unsafe { (*p.c.0).data[0] = mem.as_mut_ptr() };
            // SAFETY: as the comment above this block states.
            unsafe { ffi::av_frame_make_writable(p.c.0) }
        };
        assert_eq!((r, c), (Ok(()), 0));
        assert_eq!(shape(rust_seen(&p.r)), shape(p.c.seen()));
        assert_eq!(rows_of(p.r.as_ref()), c_rows_of(&p.c, PixelFormat::GRAY8));
        // Nothing to copy: both fail.
        let mut p = Pair::video(PixelFormat::GRAY8, 4, 4);
        let r = p.r.as_mut().make_writable();
        // SAFETY: a live frame.
        let c = unsafe { ffi::av_frame_make_writable(p.c.0) };
        assert_eq!((r, c), (Err(EINVAL), EINVAL));
        p.check("nothing");
        drop((p, mem));
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn cropping_matches_av_frame_apply_cropping() {
        let crops = [
            (0, 0, 0, 0),
            (1, 0, 0, 0),
            (0, 1, 0, 0),
            (0, 0, 1, 0),
            (0, 0, 0, 1),
            (2, 3, 5, 7),
            (4, 4, 32, 0),
            (0, 0, 33, 30),
            (15, 0, 0, 0),
            (16, 0, 0, 0),
            (0, 0, 64, 0),
            (0, 0, 63, 0),
        ];
        let (mut moved, mut refused) = (0, 0);
        for format in VIDEO {
            for (top, bottom, left, right) in crops {
                for flags in [CropFlags::empty(), CropFlags::UNALIGNED] {
                    let mut p = Pair::video(format, 64, 16);
                    p.allocated(0);
                    let mut m = p.r.as_mut();
                    m.set_crop_top(top);
                    m.set_crop_bottom(bottom);
                    m.set_crop_left(left);
                    m.set_crop_right(right);
                    let r = m.apply_cropping(flags);
                    // For the unsafe operations below: the same fields of the live C frame, whose
                    // planes stay inside its buffer for these crops.
                    let c = {
                        let f = p.c.0;
                        // SAFETY: as the comment above this block states.
                        unsafe { (*f).crop_top = top };
                        // SAFETY: as the comment above the previous write states.
                        unsafe { (*f).crop_bottom = bottom };
                        // SAFETY: as the comment above this block states.
                        unsafe { (*f).crop_left = left };
                        // SAFETY: as the comment above the previous write states.
                        unsafe { (*f).crop_right = right };
                        // SAFETY: as the comment above this block states.
                        unsafe { ffi::av_frame_apply_cropping(f, flags.bits()) }
                    };
                    let what = format!("{format:?} {top} {bottom} {left} {right} {flags:?}");
                    assert_eq!(r.err().unwrap_or(0), c, "{what}");
                    p.check(&what);
                    moved += usize::from(c == 0 && (top | left) != 0);
                    refused += usize::from(c < 0);
                }
            }
        }
        assert!(moved > 50 && refused > 10, "{moved} {refused}");
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn lookups_and_writability_match_c() {
        let mut cases = [
            Pair::video(PixelFormat::YUV420P, 8, 8),
            Pair::video(PixelFormat::NV12, 8, 8),
            Pair::audio(SampleFormat::FLTP, 10, 16),
            Pair::audio(SampleFormat::S16, 3, 16),
        ];
        for (n, p) in cases.iter_mut().enumerate() {
            p.allocated(0);
            let r = p.r.as_ref();
            let bufs = p.c.bufs();
            let mut rbufs: Vec<_> = (0..8).map(|i| r.buffer(i)).collect();
            rbufs.extend((0..r.extended_buffer_count()).map(|i| r.extended_buffer(i)));
            for i in 0..r.extended_plane_count() {
                let found = r.plane_buffer(i).map(|b| {
                    let d = b.data().as_ptr();
                    rbufs
                        .iter()
                        .position(|x| x.is_some_and(|x| x.data().as_ptr() == d))
                        .unwrap()
                });
                // SAFETY: a live frame holding `extended_data[i]`.
                let c = unsafe { ffi::av_frame_get_plane_buffer(p.c.0, i as c_int) };
                let c = (!c.is_null()).then(|| bufs.iter().position(|&b| b == c).unwrap());
                assert_eq!(found, c, "{n} plane {i}");
            }
            // SAFETY: a live frame.
            let c = unsafe { ffi::av_frame_is_writable(p.c.0) } != 0;
            assert!(r.is_writable() && c);
            let keep = r.try_clone().unwrap();
            // SAFETY: a live frame, and its clone.
            let mut keep_c = unsafe { ffi::av_frame_clone(p.c.0) };
            // SAFETY: as above.
            let c = unsafe { ffi::av_frame_is_writable(p.c.0) } != 0;
            assert!(!p.r.as_ref().is_writable() && !c);
            drop(keep);
            // SAFETY: the clone this test made.
            unsafe { ffi::av_frame_free(&raw mut keep_c) };
        }
        let bare = Pair::new();
        // SAFETY: a live frame.
        let c = unsafe { ffi::av_frame_is_writable(bare.c.0) } != 0;
        assert_eq!(bare.r.as_ref().is_writable(), c);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn side_data_routines_match_c() {
        let mut p = Pair::new();
        let kinds = [
            SideDataType::AFD,
            SideDataType::A53_CC,
            SideDataType::AFD,
            SideDataType::SEI_UNREGISTERED,
            SideDataType::SEI_UNREGISTERED,
            SideDataType::DISPLAY_MATRIX,
        ];
        let index = |p: &Pair, kind: SideDataType| {
            let r = p.r.as_ref();
            let found = r.find_side_data(kind).map(|e| {
                let d = e.buffer().data().as_ptr();
                (0..r.side_data().len())
                    .position(|i| r.side_data().get(i).unwrap().buffer().data().as_ptr() == d)
                    .unwrap()
            });
            // SAFETY: a live frame; C returns one of its entries, or NULL.
            let e = unsafe { ffi::av_frame_get_side_data(p.c.0, kind.into_raw()) };
            let c = (!e.is_null()).then(|| {
                // SAFETY: as above: reads of the frame's side data list.
                let (list, n) = (unsafe { (*p.c.0).side_data }, unsafe {
                    (*p.c.0).nb_side_data
                });
                (0..n as usize)
                    .position(|i| {
                        // SAFETY: as above, entry `i < nb_side_data`.
                        let at = unsafe { list.add(i) };
                        // SAFETY: as above; the entry is initialised.
                        unsafe { *at == e }
                    })
                    .unwrap()
            });
            assert_eq!(found, c, "{kind:?}");
        };
        for (k, kind) in kinds.into_iter().enumerate() {
            let bytes = pattern(k * 3, k as u8);
            if k % 2 == 0 {
                p.r.as_mut()
                    .alloc_side_data_from_slice(kind, &bytes)
                    .unwrap();
                // For the unsafe operations below: a live frame; the new entry's payload is written
                // in full.
                {
                    // SAFETY: as the comment above this block states.
                    let e =
                        unsafe { ffi::av_frame_new_side_data(p.c.0, kind.into_raw(), bytes.len()) };
                    assert!(!e.is_null());
                    // SAFETY: as the comment below states, for this part of the operation.
                    let data_4 = unsafe { (*e).data };
                    // SAFETY: as the comment above this block states.
                    unsafe { ptr::copy_nonoverlapping(bytes.as_ptr(), data_4, bytes.len()) };
                }
            } else {
                let buf = BufferReference::alloc(bytes.len())
                    .unwrap()
                    .copy_from_slice(&bytes);
                p.r.as_mut().add_side_data(kind, buf).unwrap();
                // For the unsafe operations below: a live frame, which takes the fresh reference.
                {
                    // SAFETY: as the comment above this block states.
                    let b = unsafe { ffi::av_buffer_alloc(bytes.len()) };
                    // SAFETY: as the comment below states, for this part of the operation.
                    let data_3 = unsafe { (*b).data };
                    // SAFETY: as the comment above this block states.
                    unsafe { ptr::copy_nonoverlapping(bytes.as_ptr(), data_3, bytes.len()) };
                    // SAFETY: as the comment above this block states.
                    unsafe {
                        assert!(
                            !ffi::av_frame_new_side_data_from_buf(p.c.0, kind.into_raw(), b)
                                .is_null()
                        );
                    };
                }
            }
            p.check(&format!("add {k}"));
            for kind in kinds {
                index(&p, kind);
            }
        }
        for kind in [
            SideDataType::AFD,
            SideDataType::GOP_TIMECODE,
            SideDataType::SEI_UNREGISTERED,
        ] {
            p.r.as_mut().remove_side_data(kind);
            // SAFETY: a live frame.
            unsafe { ffi::av_frame_remove_side_data(p.c.0, kind.into_raw()) };
            p.check(&format!("remove {kind:?}"));
            for kind in kinds {
                index(&p, kind);
            }
        }
        // The zeroed form reads as C's payload after a zero fill.
        p.r.as_mut()
            .alloc_side_data_zeroed(SideDataType::AFD, 4)
            .unwrap();
        // For the unsafe operations below: as above.
        {
            // SAFETY: as the comment above this block states.
            let e = unsafe {
                ffi::av_frame_new_side_data(p.c.0, ffi::AVFrameSideDataType::AV_FRAME_DATA_AFD, 4)
            };
            // SAFETY: as the comment below states, for this part of the operation.
            let data_2 = unsafe { (*e).data };
            // SAFETY: as the comment above this block states.
            unsafe { ptr::write_bytes(data_2, 0, 4) };
        }
        p.check("zeroed");
    }
}
