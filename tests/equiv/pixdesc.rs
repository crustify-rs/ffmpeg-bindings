//! Equivalence of `libavutil/pixdesc.h`'s safe API with the C library.

/// [`ComponentDescriptor`] against C's `AVComponentDescriptor`: the safe
/// getters against C's fields for every component of every pixel format
/// descriptor, and descriptors rebuilt through the safe setters, in
/// `av_malloc`-owned storage, against C's own table, as C's readers
/// (`av_read_image_line2`, `av_get_padded_bits_per_pixel`) observe them
/// after each component is written.
///
/// [`ComponentDescriptor`]: ffmpeg_bindings::libavutil::libavutil::pixdesc::ComponentDescriptor
mod component_descriptor {
    use core::ffi::{CStr, c_int, c_void};
    use core::ptr;

    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::mem::alloc_cell_array_zeroed;
    use ffmpeg_bindings::libavutil::libavutil::pixdesc::{
        ComponentDescriptor, ComponentDescriptorMut, ComponentDescriptorRef,
    };

    /// Pixels read per line.
    const W: usize = 8;
    /// Bytes per image row; wide enough for 8 pixels of a 32-byte step plus
    /// C's largest component offset.
    const LINESIZE: usize = 512;
    /// Rows per plane; C reads row 1, so a negative offset stays in bounds.
    const ROWS: usize = 3;

    /// Copies of every descriptor in C's table, in C's order.
    fn c_table() -> Vec<ffi::AVPixFmtDescriptor> {
        let mut out = Vec::new();
        let mut p = ptr::null();
        loop {
            // SAFETY: `p` is NULL or the previous descriptor
            // `av_pix_fmt_desc_next` returned, a pointer into C's static
            // table, as it requires.
            p = unsafe { ffi::av_pix_fmt_desc_next(p) };
            if p.is_null() {
                return out;
            }
            // SAFETY: `p` points to a live, initialised descriptor in C's
            // static, never-written table; the copy is a plain value whose
            // pointer fields still address that static table.
            out.push(unsafe { p.read() });
        }
    }

    fn name(d: &ffi::AVPixFmtDescriptor) -> String {
        // SAFETY: every table descriptor's `name` is a static NUL-terminated
        // string.
        unsafe { CStr::from_ptr(d.name) }
            .to_string_lossy()
            .into_owned()
    }

    /// Four planes of pseudo-random bytes, so a component read from the
    /// wrong plane, offset, step, shift or depth reads different values.
    fn image() -> [Vec<u8>; 4] {
        let mut x = 0x2545_f491_u32;
        core::array::from_fn(|_| {
            (0..LINESIZE * ROWS)
                .map(|_| {
                    x ^= x << 13;
                    x ^= x >> 17;
                    x ^= x << 5;
                    x as u8
                })
                .collect()
        })
    }

    /// What C observes of component `c` of `desc`: the `W` values
    /// `av_read_image_line2` reads from row 1 of `img`, and the format's
    /// padded bits per pixel.
    fn c_observes(
        desc: &ffi::AVPixFmtDescriptor,
        c: usize,
        img: &[Vec<u8>; 4],
    ) -> ([u32; W], c_int) {
        let mut data: [*const u8; 4] = core::array::from_fn(|i| img[i].as_ptr());
        let linesize = [LINESIZE as c_int; 4];
        let mut dst = [0u32; W];
        // SAFETY: `desc` is a live descriptor whose components are each either C's own
        // table entry for its format, addressing planes 0..4 with C's offsets, steps and
        // depths, or all-zero, whose `depth` of 0 makes C return before reading; every
        // plane holds `ROWS` rows of `LINESIZE` bytes, enough for `W` pixels of row 1 at
        // the table's widest step and offset; `dst` holds `W` 4-byte elements; a palette
        // is never indexed (`read_pal_component` 0); the bit count reads planes 0..4.
        unsafe {
            ffi::av_read_image_line2(
                dst.as_mut_ptr().cast::<c_void>(),
                data.as_mut_ptr(),
                linesize.as_ptr(),
                desc,
                0,
                1,
                c as c_int,
                W as c_int,
                0,
                4,
            );
            (dst, ffi::av_get_padded_bits_per_pixel(desc))
        }
    }

    fn c_fields(d: &ffi::AVComponentDescriptor) -> [i32; 5] {
        [d.plane, d.step, d.offset, d.shift, d.depth]
    }

    fn rust_fields(d: ComponentDescriptorRef<'_>) -> [i32; 5] {
        [d.plane(), d.step(), d.offset(), d.shift(), d.depth()]
    }

    /// Writes `from`'s fields into `to` one setter at a time.
    fn copy_fields(from: ComponentDescriptorRef<'_>, mut to: ComponentDescriptorMut<'_>) {
        to.set_plane(from.plane());
        to.set_step(from.step());
        to.set_offset(from.offset());
        to.set_shift(from.shift());
        to.set_depth(from.depth());
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn getters_read_what_c_stores_for_every_component() {
        let table = c_table();
        assert!(table.len() > 200, "C has {} descriptors", table.len());
        let mut negative_offsets = 0;
        for desc in &table {
            for raw in &desc.comp {
                let d = ComponentDescriptor::from_raw(*raw);
                assert_eq!(rust_fields(d.as_ref()), c_fields(raw), "{}", name(desc));
                assert_eq!(rust_fields(d.as_ref().get().as_ref()), c_fields(raw));
                let back = d.into_raw();
                assert_eq!(c_fields(&back), c_fields(raw));
                negative_offsets += usize::from(raw.offset < 0);
            }
        }
        // C's big-endian 16-bit packed RGB formats store `offset = -1`.
        assert!(negative_offsets > 0);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn components_written_through_setters_read_like_c_s_table() {
        let img = image();
        let mut observed_reads = 0;
        for c_desc in c_table() {
            let nb = usize::from(c_desc.nb_components);
            // C side: C's descriptor with every component cleared, then
            // restored by plain C-layout assignment, one component at a
            // time.
            let mut c_side = c_desc;
            c_side.comp = [ffi::AVComponentDescriptor::default(); 4];
            // Rust side: an independently owned descriptor whose components
            // come from an `av_malloc`-owned array written by the safe
            // setters.
            let mut rust_side = c_desc;
            let mut comps = alloc_cell_array_zeroed::<ComponentDescriptor>(4).unwrap();
            for i in 0..4 {
                rust_side.comp[i] = comps.as_handles().get(i).unwrap().get().into_raw();
            }
            let check = |c_side: &ffi::AVPixFmtDescriptor,
                         rust_side: &ffi::AVPixFmtDescriptor,
                         step: &str| {
                for c in 0..4 {
                    assert_eq!(
                        c_observes(c_side, c, &img),
                        c_observes(rust_side, c, &img),
                        "{} component {c} after {step}",
                        name(&c_desc)
                    );
                }
            };
            check(&c_side, &rust_side, "clearing");
            for c in 0..nb {
                c_side.comp[c] = c_desc.comp[c];
                let source = ComponentDescriptor::from_raw(c_desc.comp[c]);
                {
                    let mut handles = comps.as_handles_mut();
                    copy_fields(source.as_ref(), handles.get_mut(c).unwrap());
                }
                let written = comps.as_handles().get(c).unwrap();
                assert_eq!(rust_fields(written), c_fields(&c_side.comp[c]));
                rust_side.comp[c] = written.get().into_raw();
                check(&c_side, &rust_side, &format!("writing component {c}"));
                observed_reads += usize::from(c_observes(&c_side, c, &img).0 != [0; W]);
            }
            // Finally the whole component array in one `set` each, from
            // the values the Rust array holds.
            let mut whole = c_desc;
            for (i, h) in comps.as_handles().iter().enumerate() {
                let mut v = ComponentDescriptor::new();
                v.as_mut().set(h.get());
                whole.comp[i] = v.into_raw();
            }
            check(&c_side, &whole, "setting whole components");
        }
        // Not vacuous: most components read non-zero pixels.
        assert!(observed_reads > 500, "{observed_reads}");
    }
}

/// The color property lookups by name (`ColorRange::from_name` and its
/// siblings) against `av_color_range_from_name` and its siblings: every name
/// in C's tables, each looked up as every property, and the near misses
/// around them, in repeated sequences.
mod color_names {
    use core::ffi::{CStr, c_char, c_int};
    use std::ffi::CString;

    use ffmpeg_bindings::libavutil::libavutil::ffi;
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

    /// Every table name and the spellings around it: case, padding, prefixes,
    /// suffixes, truncations, the empty name, bytes past ASCII and a long name.
    fn candidates() -> Vec<CString> {
        let mut names: Vec<Vec<u8>> = vec![
            b"".to_vec(),
            b" ".to_vec(),
            b"\xff\xfe".to_vec(),
            vec![b'a'; 4096],
        ];
        for name in RANGES
            .iter()
            .chain(&PRIMARIES)
            .chain(&TRANSFERS)
            .chain(&SPACES)
            .chain(&LOCATIONS)
        {
            let n = name.as_bytes();
            names.push(n.to_vec());
            names.push(name.to_uppercase().into_bytes());
            names.push([n, b" "].concat());
            names.push([b" ", n].concat());
            names.push([n, b"x"].concat());
            names.push(n[..n.len() - 1].to_vec());
            names.push([n, b"\xc3\xa9"].concat());
        }
        names
            .into_iter()
            .map(|n| CString::new(n).unwrap())
            .collect()
    }

    /// A C lookup's result as the Rust one reports it: `None` for its
    /// `AVERROR(EINVAL)`.
    fn c_lookup(f: unsafe extern "C" fn(*const c_char) -> c_int, name: &CStr) -> Option<u32> {
        // SAFETY: `name` is a live NUL-terminated string, only read.
        let ret = unsafe { f(name.as_ptr()) };
        if ret < 0 {
            assert_eq!(ret, -22, "{name:?}");
            None
        } else {
            Some(ret as u32)
        }
    }

    /// Every property's lookup of `name`, Rust's and C's.
    fn lookups(name: &CStr) -> [(Option<u32>, Option<u32>); 5] {
        [
            (
                ColorRange::from_name(name).map(|v| v.into_raw().0),
                c_lookup(ffi::av_color_range_from_name, name),
            ),
            (
                ColorPrimaries::from_name(name).map(|v| v.into_raw().0),
                c_lookup(ffi::av_color_primaries_from_name, name),
            ),
            (
                ColorTransfer::from_name(name).map(|v| v.into_raw().0),
                c_lookup(ffi::av_color_transfer_from_name, name),
            ),
            (
                ColorSpace::from_name(name).map(|v| v.into_raw().0),
                c_lookup(ffi::av_color_space_from_name, name),
            ),
            (
                ChromaLocation::from_name(name).map(|v| v.into_raw().0),
                c_lookup(ffi::av_chroma_location_from_name, name),
            ),
        ]
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn lookups_match_c_for_every_candidate() {
        for name in candidates() {
            for (i, (rust, c)) in lookups(&name).into_iter().enumerate() {
                assert_eq!(rust, c, "property {i}, {name:?}");
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn every_table_name_is_found() {
        let tables: [&[&str]; 5] = [&RANGES, &PRIMARIES, &TRANSFERS[..19], &SPACES, &LOCATIONS];
        for (i, table) in tables.into_iter().enumerate() {
            for name in table {
                let name = CString::new(*name).unwrap();
                let (rust, c) = lookups(&name)[i];
                assert!(rust.is_some(), "property {i}, {name:?}");
                assert_eq!(rust, c);
            }
        }
        // C's tables name no transfer `unspecified`.
        assert_eq!(lookups(c"unspecified")[2], (None, None));
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn repeated_interleaved_lookups_match_c() {
        let names = candidates();
        let first: Vec<_> = names.iter().map(|n| lookups(n)).collect();
        for round in 0..3 {
            for (name, expect) in names.iter().zip(&first).rev().step_by(round + 1) {
                assert_eq!(&lookups(name), expect, "round {round}, {name:?}");
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn shared_names_resolve_as_c_does() {
        // `reserved` names two primaries and two transfers: both sides take the
        // first, value 0.
        assert_eq!(lookups(c"reserved")[1], (Some(0), Some(0)));
        assert_eq!(lookups(c"reserved")[2], (Some(0), Some(0)));
        assert_eq!(
            ColorPrimaries::from_name(c"reserved"),
            Some(ColorPrimaries::RESERVED0)
        );
        assert_eq!(
            ColorTransfer::from_name(c"reserved"),
            Some(ColorTransfer::RESERVED0)
        );
        // The extended ranges start past the plain ones.
        assert_eq!(
            lookups(c"vgamut")[1].0,
            Some(ffi::AVColorPrimaries::AVCOL_PRI_V_GAMUT.0)
        );
        assert_eq!(
            lookups(c"vlog")[2].0,
            Some(ffi::AVColorTransferCharacteristic::AVCOL_TRC_V_LOG.0)
        );
    }
}

/// [`PixelFormat::swap_endianness`] against `av_pix_fmt_swap_endianness`, for
/// every value C names and those around them, swapped once and back.
///
/// [`PixelFormat::swap_endianness`]: ffmpeg_bindings::libavutil::libavutil::pixfmt::PixelFormat::swap_endianness
mod swap_endianness {
    use core::ffi::c_int;

    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::pixfmt::PixelFormat;

    /// `av_pix_fmt_swap_endianness(raw)`.
    fn c_swap(raw: c_int) -> c_int {
        // SAFETY: C takes the enum by value and bounds-checks it.
        unsafe { ffi::av_pix_fmt_swap_endianness(ffi::AVPixelFormat(raw)) }.0
    }

    /// The Rust result as C encodes it: `AV_PIX_FMT_NONE` for `None`.
    fn rust_swap(raw: c_int) -> c_int {
        PixelFormat::from_raw(ffi::AVPixelFormat(raw))
            .swap_endianness()
            .map_or(ffi::AVPixelFormat::AV_PIX_FMT_NONE.0, |f| f.into_raw().0)
    }

    fn candidates() -> impl Iterator<Item = c_int> {
        let nb = ffi::AVPixelFormat::AV_PIX_FMT_NB.0;
        (-3..nb + 3).chain([c_int::MIN, c_int::MIN + 1, 1 << 20, c_int::MAX])
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn every_value_swaps_as_c_swaps_it() {
        let mut swapped = 0;
        for raw in candidates() {
            let (c, rust) = (c_swap(raw), rust_swap(raw));
            assert_eq!(rust, c, "{raw}");
            swapped += usize::from(c != ffi::AVPixelFormat::AV_PIX_FMT_NONE.0);
        }
        // Every byte-ordered format has its twin: C's table lists pairs.
        assert!(swapped > 100 && swapped % 2 == 0, "{swapped}");
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn swapping_twice_returns_where_c_returns() {
        for raw in candidates() {
            let (c1, r1) = (c_swap(raw), rust_swap(raw));
            assert_eq!(r1, c1, "{raw}");
            let (c2, r2) = (c_swap(c1), rust_swap(r1));
            assert_eq!(r2, c2, "{raw}");
            if c1 != ffi::AVPixelFormat::AV_PIX_FMT_NONE.0 {
                assert_eq!((c2, r2), (raw, raw));
            }
        }
    }
}

/// The value-to-name getters (`ColorRange::name` and its siblings,
/// `AlphaMode::name`) against `av_color_range_name` and its siblings for
/// every value up to past the extended ranges and the `unsigned` extremes,
/// the names fed back to both sides' lookups, and `AlphaMode::from_name`
/// against `av_alpha_mode_from_name` on C's names and the near misses around
/// them, in repeated sequences.
mod property_names {
    use core::ffi::{CStr, c_char};
    use std::ffi::CString;

    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::pixfmt::{
        AlphaMode, ChromaLocation, ColorPrimaries, ColorRange, ColorSpace, ColorTransfer,
    };

    /// A C name getter's result as the Rust one reports it.
    fn c_name(p: *const c_char) -> Option<&'static CStr> {
        // SAFETY: every `*_name` getter returns NULL or a string literal
        // from a static table, NUL-terminated and never written.
        (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) })
    }

    /// Every value a name table could index, both extended ranges, and the
    /// `unsigned` extremes, which an `int` comparison would read as
    /// negative.
    fn raws() -> Vec<u32> {
        (0..300)
            .chain([
                511,
                512,
                1 << 16,
                i32::MAX as u32,
                1 << 31,
                u32::MAX - 1,
                u32::MAX,
            ])
            .collect()
    }

    /// Every property's name for `raw`, Rust's and C's.
    fn names(raw: u32) -> [(Option<&'static CStr>, Option<&'static CStr>); 6] {
        // SAFETY: the getters take the enum by value, accept any value, and
        // return NULL or a static string.
        unsafe {
            [
                (
                    ColorRange::from_raw(ffi::AVColorRange(raw)).name(),
                    c_name(ffi::av_color_range_name(ffi::AVColorRange(raw))),
                ),
                (
                    ColorPrimaries::from_raw(ffi::AVColorPrimaries(raw)).name(),
                    c_name(ffi::av_color_primaries_name(ffi::AVColorPrimaries(raw))),
                ),
                (
                    ColorTransfer::from_raw(ffi::AVColorTransferCharacteristic(raw)).name(),
                    c_name(ffi::av_color_transfer_name(
                        ffi::AVColorTransferCharacteristic(raw),
                    )),
                ),
                (
                    ColorSpace::from_raw(ffi::AVColorSpace(raw)).name(),
                    c_name(ffi::av_color_space_name(ffi::AVColorSpace(raw))),
                ),
                (
                    ChromaLocation::from_raw(ffi::AVChromaLocation(raw)).name(),
                    c_name(ffi::av_chroma_location_name(ffi::AVChromaLocation(raw))),
                ),
                (
                    AlphaMode::from_raw(ffi::AVAlphaMode(raw)).name(),
                    c_name(ffi::av_alpha_mode_name(ffi::AVAlphaMode(raw))),
                ),
            ]
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn names_match_c_for_every_value() {
        let mut named = [0usize; 6];
        for raw in raws() {
            for (i, (rust, c)) in names(raw).into_iter().enumerate() {
                assert_eq!(rust, c, "property {i}, value {raw}");
                // C hands out the same static string, not a copy.
                assert_eq!(rust.map(CStr::as_ptr), c.map(CStr::as_ptr));
                named[i] += usize::from(rust.is_some());
            }
        }
        // 3 ranges, 14 + 1 primaries, 19 + 1 transfers, 18 spaces, 7
        // locations, 3 alpha modes.
        assert_eq!(named, [3, 15, 20, 18, 7, 3]);
    }

    /// A lookup's result as a raw value, Rust's (`from_name`) and C's.
    fn lookup(i: usize, name: &CStr) -> (Option<u32>, Option<u32>) {
        let c = |ret: core::ffi::c_int| u32::try_from(ret).ok();
        // SAFETY: `name` is a live NUL-terminated string, which the lookups
        // only read.
        unsafe {
            match i {
                0 => (
                    ColorRange::from_name(name).map(|v| v.into_raw().0),
                    c(ffi::av_color_range_from_name(name.as_ptr())),
                ),
                1 => (
                    ColorPrimaries::from_name(name).map(|v| v.into_raw().0),
                    c(ffi::av_color_primaries_from_name(name.as_ptr())),
                ),
                2 => (
                    ColorTransfer::from_name(name).map(|v| v.into_raw().0),
                    c(ffi::av_color_transfer_from_name(name.as_ptr())),
                ),
                3 => (
                    ColorSpace::from_name(name).map(|v| v.into_raw().0),
                    c(ffi::av_color_space_from_name(name.as_ptr())),
                ),
                4 => (
                    ChromaLocation::from_name(name).map(|v| v.into_raw().0),
                    c(ffi::av_chroma_location_from_name(name.as_ptr())),
                ),
                _ => (
                    AlphaMode::from_name(name).map(|v| v.into_raw().0),
                    c(ffi::av_alpha_mode_from_name(name.as_ptr()).0 as core::ffi::c_int),
                ),
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn names_map_back_through_both_lookups() {
        for raw in raws() {
            for (i, (name, _)) in names(raw).into_iter().enumerate() {
                let Some(name) = name else { continue };
                let (rust, c) = lookup(i, name);
                assert_eq!(rust, c, "property {i}, {name:?}");
                // Each name maps back to its value, but for `reserved`,
                // which two primaries and two transfers share: the first.
                let back = rust.unwrap();
                assert!(back == raw || name == c"reserved" && back == 0, "{name:?}");
            }
        }
    }

    /// C's alpha mode names and the spellings around them.
    fn alpha_candidates() -> Vec<CString> {
        let mut out = vec![CString::default(), c"none".into(), c"unknown".into()];
        for name in ["unspecified", "premultiplied", "straight"] {
            let n = name.as_bytes();
            out.push(CString::new(n).unwrap());
            out.push(CString::new(name.to_uppercase()).unwrap());
            out.push(CString::new([n, b" "].concat()).unwrap());
            out.push(CString::new(&n[1..]).unwrap());
            out.push(CString::new(&n[..n.len() - 1]).unwrap());
            out.push(CString::new([n, b"\xff"].concat()).unwrap());
        }
        out
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn alpha_mode_from_name_matches_c() {
        let mut found = 0;
        for name in alpha_candidates() {
            let (rust, c) = lookup(5, &name);
            assert_eq!(rust, c, "{name:?}");
            found += usize::from(rust.is_some());
        }
        assert_eq!(found, 3);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn repeated_interleaved_calls_match_c() {
        let names_: Vec<CString> = alpha_candidates();
        let values = raws();
        for round in 0..3 {
            for (k, raw) in values.iter().enumerate().rev().step_by(round + 1) {
                for (i, (rust, c)) in names(*raw).into_iter().enumerate() {
                    assert_eq!(rust, c, "round {round}, property {i}, value {raw}");
                }
                let name = &names_[k % names_.len()];
                let (rust, c) = lookup(5, name);
                assert_eq!(rust, c, "round {round}, {name:?}");
            }
        }
    }
}

/// `ChromaLocation::position` and `ChromaLocation::from_position` against
/// `av_chroma_location_enum_to_pos` and `av_chroma_location_pos_to_enum`:
/// every location value, the `unsigned` extremes, every position on and
/// around C's grid, and the round trips between them.
mod chroma_positions {
    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::pixfmt::ChromaLocation;

    /// C's `enum_to_pos` for `raw`: the position when it succeeds, and
    /// whether it left the out-parameters alone when it fails.
    fn c_position(raw: u32) -> Option<(i32, i32)> {
        let (mut x, mut y) = (i32::MIN, i32::MAX);
        // SAFETY: `x` and `y` are live, writable `int`s, written only on
        // success and not kept.
        let ret = unsafe {
            ffi::av_chroma_location_enum_to_pos(&mut x, &mut y, ffi::AVChromaLocation(raw))
        };
        if ret == 0 {
            Some((x, y))
        } else {
            assert_eq!(ret, -22, "value {raw}");
            assert_eq!((x, y), (i32::MIN, i32::MAX), "value {raw} wrote on failure");
            None
        }
    }

    fn c_from_position(x: i32, y: i32) -> u32 {
        // SAFETY: two integers by value.
        unsafe { ffi::av_chroma_location_pos_to_enum(x, y) }.0
    }

    /// Coordinates on C's 0/128/256 grid, their neighbours and the extremes.
    const COORDS: [i32; 13] = [
        i32::MIN,
        -256,
        -128,
        -1,
        0,
        1,
        127,
        128,
        129,
        255,
        256,
        257,
        i32::MAX,
    ];

    fn raws() -> impl Iterator<Item = u32> {
        (0..20).chain([255, 256, i32::MAX as u32, 1 << 31, u32::MAX])
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn position_matches_c_for_every_value() {
        let mut found = 0;
        for raw in raws() {
            let rust = ChromaLocation::from_raw(ffi::AVChromaLocation(raw)).position();
            assert_eq!(rust, c_position(raw), "value {raw}");
            found += usize::from(rust.is_some());
        }
        assert_eq!(found, 6);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn from_position_matches_c_on_and_around_the_grid() {
        let mut found = 0;
        for x in COORDS {
            for y in COORDS {
                let rust = ChromaLocation::from_position(x, y).map(|l| l.into_raw().0);
                let c = c_from_position(x, y);
                // C reports "no location" as `AVCHROMA_LOC_UNSPECIFIED` (0).
                assert_eq!(rust.unwrap_or(0), c, "({x}, {y})");
                assert_ne!(rust, Some(0));
                found += usize::from(rust.is_some());
            }
        }
        assert_eq!(found, 6);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn positions_round_trip_on_both_sides() {
        for raw in raws() {
            let loc = ChromaLocation::from_raw(ffi::AVChromaLocation(raw));
            let Some((x, y)) = loc.position() else {
                continue;
            };
            assert_eq!(ChromaLocation::from_position(x, y), Some(loc));
            assert_eq!(c_from_position(x, y), raw);
            assert_eq!(c_position(c_from_position(x, y)), Some((x, y)));
        }
    }
}

/// `PixelFormat::name` and `PixelFormat::from_name` against
/// `av_get_pix_fmt_name` and `av_get_pix_fmt`: every value from below
/// `NONE` to past `AV_PIX_FMT_NB` and the `int` extremes, every name C
/// gives fed back, and the endianness-less, aliased, case-changed, long and
/// near-miss spellings around them.
mod pixel_format_names {
    use core::ffi::{CStr, c_int};
    use std::ffi::CString;

    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::pixfmt::PixelFormat;

    fn nb() -> c_int {
        ffi::AVPixelFormat::AV_PIX_FMT_NB.0
    }

    fn raws() -> impl Iterator<Item = c_int> {
        (-3..nb() + 3).chain([i32::MIN, i32::MIN + 1, 1 << 16, i32::MAX])
    }

    fn c_name(raw: c_int) -> Option<&'static CStr> {
        // SAFETY: `av_get_pix_fmt_name` accepts any value and returns NULL or
        // a descriptor's static name.
        let p = unsafe { ffi::av_get_pix_fmt_name(ffi::AVPixelFormat(raw)) };
        // SAFETY: a non-NULL result is a string literal, never written.
        (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) })
    }

    fn c_lookup(name: &CStr) -> c_int {
        // SAFETY: `name` is a live NUL-terminated string, only read.
        unsafe { ffi::av_get_pix_fmt(name.as_ptr()) }.0
    }

    /// Both lookups of `name`, as raw values; C's `NONE` is Rust's `None`.
    fn lookups(name: &CStr) -> (c_int, c_int) {
        let rust = PixelFormat::from_name(name).map_or(-1, |f| f.into_raw().0);
        if let Some(f) = PixelFormat::from_name(name) {
            assert_ne!(f, PixelFormat::NONE);
        }
        (rust, c_lookup(name))
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn name_matches_c_for_every_value() {
        let mut named = 0;
        for raw in raws() {
            let rust = PixelFormat::from_raw(ffi::AVPixelFormat(raw)).name();
            let c = c_name(raw);
            assert_eq!(rust, c, "value {raw}");
            assert_eq!(rust.map(CStr::as_ptr), c.map(CStr::as_ptr));
            named += usize::from(rust.is_some());
        }
        assert_eq!(named, nb() as usize);
    }

    /// Every name C gives, with the spellings around it.
    fn candidates() -> Vec<CString> {
        let mut out: Vec<Vec<u8>> = vec![
            b"".to_vec(),
            b"rgb32".to_vec(),
            b"bgr32".to_vec(),
            b"RGB32".to_vec(),
            b"rgb32le".to_vec(),
            b"none".to_vec(),
            b"gray16".to_vec(),
            b"yuv420p10".to_vec(),
            // Aliases C's descriptors list.
            b"y400a".to_vec(),
            b"gray8a".to_vec(),
            b"rgb565".to_vec(),
            // Long enough that C's endianness retry truncates its copy.
            vec![b'x'; 29],
            vec![b'x'; 30],
            vec![b'x'; 31],
            vec![b'x'; 200],
            [b"gray16".as_slice(), &[b' '; 26]].concat(),
            [b"\xff\xfe".as_slice(), b"le"].concat(),
        ];
        for raw in 0..nb() {
            let Some(name) = c_name(raw) else { continue };
            let n = name.to_bytes();
            out.push(n.to_vec());
            out.push(n.to_ascii_uppercase());
            out.push([n, b"le"].concat());
            out.push([n, b"be"].concat());
            out.push(n[..n.len() - 1].to_vec());
            if n.ends_with(b"le") || n.ends_with(b"be") {
                out.push(n[..n.len() - 2].to_vec());
            }
        }
        out.into_iter().map(|n| CString::new(n).unwrap()).collect()
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn from_name_matches_c_for_every_candidate() {
        let mut found = 0;
        for name in candidates() {
            let (rust, c) = lookups(&name);
            assert_eq!(rust, c, "{name:?}");
            found += usize::from(rust >= 0);
        }
        assert!(found > nb() as usize, "{found}");
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn names_map_back_on_both_sides() {
        for raw in 0..nb() {
            let f = PixelFormat::from_raw(ffi::AVPixelFormat(raw));
            let name = f.name().unwrap();
            assert_eq!(lookups(name), (raw, raw), "{name:?}");
            assert_eq!(PixelFormat::from_name(name), Some(f));
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn native_endian_names_resolve_as_c_does() {
        assert_eq!(lookups(c"gray16").0, PixelFormat::GRAY16.into_raw().0);
        assert_eq!(lookups(c"rgb32").0, PixelFormat::RGB32.into_raw().0);
        assert_eq!(lookups(c"bgr32").0, PixelFormat::BGR32.into_raw().0);
        for name in [c"gray16", c"rgb32", c"bgr32", c"yuv420p10"] {
            let (rust, c) = lookups(name);
            assert_eq!(rust, c, "{name:?}");
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn repeated_interleaved_calls_match_c() {
        let names = candidates();
        let values: Vec<c_int> = raws().collect();
        for round in 0..3 {
            for (k, name) in names.iter().enumerate().rev().step_by(round + 2) {
                let raw = values[k % values.len()];
                assert_eq!(
                    PixelFormat::from_raw(ffi::AVPixelFormat(raw)).name(),
                    c_name(raw),
                    "round {round}, value {raw}"
                );
                let (rust, c) = lookups(name);
                assert_eq!(rust, c, "round {round}, {name:?}");
            }
        }
    }
}

/// `PixelFormat::conversion_loss`, `best_of_two` and
/// `best_of_two_with_loss` against `av_get_pix_fmt_loss` and
/// `av_find_best_pix_fmt_of_2`: every ordered pair of values from below
/// `NONE` to past `AV_PIX_FMT_NB`, with and without alpha; candidate pairs
/// for every source, under several ignore sets; and chained choices that
/// feed each answer into the next call.
mod pixel_format_loss {
    use core::ffi::c_int;
    use core::ptr;

    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::pixdesc::Loss;
    use ffmpeg_bindings::libavutil::libavutil::pixfmt::PixelFormat;

    fn nb() -> c_int {
        ffi::AVPixelFormat::AV_PIX_FMT_NB.0
    }

    /// Every value C knows, `NONE`, and unknown values on both sides.
    fn raws() -> Vec<c_int> {
        (-2..nb() + 2).chain([i32::MIN, i32::MAX]).collect()
    }

    fn fmt(raw: c_int) -> PixelFormat {
        PixelFormat::from_raw(ffi::AVPixelFormat(raw))
    }

    fn c_loss(dst: c_int, src: c_int, has_alpha: bool) -> c_int {
        // SAFETY: three integers by value; C bounds-checks both formats.
        unsafe {
            ffi::av_get_pix_fmt_loss(
                ffi::AVPixelFormat(dst),
                ffi::AVPixelFormat(src),
                c_int::from(has_alpha),
            )
        }
    }

    /// A Rust loss result as C's `int`.
    fn as_c(r: Result<Loss, i32>) -> c_int {
        match r {
            Ok(loss) => {
                assert!(loss.bits() >= 0);
                loss.bits()
            }
            Err(e) => {
                assert!(e < 0);
                e
            }
        }
    }

    fn c_best(
        d1: c_int,
        d2: c_int,
        src: c_int,
        has_alpha: bool,
        loss: Option<c_int>,
    ) -> (c_int, Option<c_int>) {
        let mut l = loss.unwrap_or(0);
        let p = if loss.is_some() {
            &raw mut l
        } else {
            ptr::null_mut()
        };
        // SAFETY: the formats and flag by value; `p` is NULL or a live,
        // writable `int`, read and written during the call only.
        let r = unsafe {
            ffi::av_find_best_pix_fmt_of_2(
                ffi::AVPixelFormat(d1),
                ffi::AVPixelFormat(d2),
                ffi::AVPixelFormat(src),
                c_int::from(has_alpha),
                p,
            )
        };
        (r.0, loss.map(|_| l))
    }

    /// A deterministic pseudo-random sequence.
    fn xorshift(mut x: u32) -> impl FnMut() -> u32 {
        move || {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            x
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn conversion_loss_matches_c_for_every_pair() {
        let raws = raws();
        let (mut ok, mut errs) = (0, [0usize; 4]);
        for &src in &raws {
            for &dst in &raws {
                for has_alpha in [false, true] {
                    let rust = fmt(src).conversion_loss(fmt(dst), has_alpha);
                    let c = c_loss(dst, src, has_alpha);
                    assert_eq!(as_c(rust), c, "{src} -> {dst}, alpha {has_alpha}");
                    match rust {
                        Ok(_) => ok += 1,
                        Err(e) => errs[(-e - 1) as usize] += 1,
                    }
                }
            }
        }
        // Every error C reports is reached but `-3`: every software format
        // in C's table has components.
        assert!(
            ok > 0 && errs[0] > 0 && errs[1] > 0 && errs[3] > 0,
            "{ok} {errs:?}"
        );
        assert_eq!(errs[2], 0);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn best_of_two_matches_c_for_every_source() {
        let raws = raws();
        let mut next = xorshift(0x9e37_79b9);
        for &src in &raws {
            for _ in 0..64 {
                let d1 = raws[next() as usize % raws.len()];
                let d2 = raws[next() as usize % raws.len()];
                for has_alpha in [false, true] {
                    let rust = fmt(src).best_of_two(fmt(d1), fmt(d2), has_alpha);
                    let (c, _) = c_best(d1, d2, src, has_alpha, None);
                    assert_eq!(
                        rust.into_raw().0,
                        c,
                        "{src}: {d1} or {d2}, alpha {has_alpha}"
                    );
                }
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn best_of_two_with_loss_matches_c_under_every_ignore_set() {
        let raws = raws();
        let all = (0..8).fold(Loss::empty(), |acc, i| acc | Loss::from_bits_retain(1 << i));
        let ignores = [
            Loss::empty(),
            Loss::ALPHA,
            Loss::DEPTH | Loss::RESOLUTION,
            Loss::COLORSPACE | Loss::CHROMA | Loss::COLORQUANT,
            Loss::EXCESS_DEPTH | Loss::EXCESS_RESOLUTION,
            all,
            Loss::from_bits_retain(-1),
            Loss::from_bits_retain(i32::MIN),
            Loss::from_bits_retain(0x100),
        ];
        let mut next = xorshift(0x1234_5678);
        for &src in &raws {
            for _ in 0..16 {
                let d1 = raws[next() as usize % raws.len()];
                let d2 = raws[next() as usize % raws.len()];
                for ignore in ignores {
                    for has_alpha in [false, true] {
                        let (rust, loss) =
                            fmt(src).best_of_two_with_loss(fmt(d1), fmt(d2), has_alpha, ignore);
                        let (c, c_out) = c_best(d1, d2, src, has_alpha, Some(ignore.bits()));
                        let at = format!("{src}: {d1} or {d2}, alpha {has_alpha}, {ignore:?}");
                        assert_eq!(rust.into_raw().0, c, "{at}");
                        assert_eq!(Some(as_c(loss)), c_out, "{at}");
                        // C reports the chosen format's loss.
                        assert_eq!(as_c(loss), c_loss(c, src, has_alpha), "{at}");
                    }
                }
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn chained_choices_match_c_step_by_step() {
        // Pick the best of every format for a few sources, carrying the
        // answer and its loss into the next call on both sides.
        for src in [
            PixelFormat::YUV420P,
            PixelFormat::RGBA,
            PixelFormat::GRAY16LE,
            PixelFormat::PAL8,
            PixelFormat::YUVA444P16LE,
            PixelFormat::NONE,
        ] {
            let s = src.into_raw().0;
            for has_alpha in [false, true] {
                let (mut rust, mut c) = (PixelFormat::NONE, -1);
                let (mut rust_ignore, mut c_ignore) = (Loss::empty(), 0);
                let (mut rust2, mut c2) = (PixelFormat::NONE, -1);
                for raw in -1..nb() {
                    rust2 = src.best_of_two(rust2, fmt(raw), has_alpha);
                    c2 = c_best(c2, raw, s, has_alpha, None).0;
                    assert_eq!(rust2.into_raw().0, c2, "{src:?} step {raw}");

                    let (r, loss) =
                        src.best_of_two_with_loss(rust, fmt(raw), has_alpha, rust_ignore);
                    let (cr, cl) = c_best(c, raw, s, has_alpha, Some(c_ignore));
                    assert_eq!(
                        (r.into_raw().0, Some(as_c(loss))),
                        (cr, cl),
                        "{src:?} step {raw}"
                    );
                    rust = r;
                    c = cr;
                    // Feed the reported loss back as the next ignore set,
                    // as C's own callers do with one `int`.
                    rust_ignore = Loss::from_bits_retain(as_c(loss));
                    c_ignore = cl.unwrap();
                }
            }
        }
    }
}

/// [`PixelFormatDescriptor`] against C's `AVPixFmtDescriptor`: the safe
/// getters against what C stores and reports for every entry of its table
/// (the empty entries included), and descriptors rebuilt through the safe
/// setters, one field at a time, in a local, an `av_malloc`-owned box and an
/// `av_malloc`-owned array, against a C-side copy assigned field by field in
/// the same order, as C's readers of a whole descriptor
/// (`av_get_bits_per_pixel`, `av_get_padded_bits_per_pixel`,
/// `av_read_image_line2`, `av_write_image_line2`) observe both after every
/// step.
///
/// [`PixelFormatDescriptor`]: ffmpeg_bindings::libavutil::libavutil::pixdesc::PixelFormatDescriptor
mod pixel_format_descriptor {
    use core::ffi::{CStr, c_int, c_void};

    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::mem::{HeapBox, alloc_cell_array_zeroed};
    use ffmpeg_bindings::libavutil::libavutil::pixdesc::{
        ComponentDescriptor, PixelFormatDescriptor, PixelFormatDescriptorMut,
        PixelFormatDescriptorRef, PixelFormatFlags,
    };
    use ffmpeg_bindings::libavutil::libavutil::pixfmt::PixelFormat;

    /// The number of entries in C's table.
    const NB: c_int = ffi::AVPixelFormat::AV_PIX_FMT_NB.0;
    /// Values read or written per line.
    const W: usize = 8;
    /// Bytes per image row: 8 pixels of the table's widest step plus its
    /// largest offset.
    const LINESIZE: usize = 512;
    /// Rows per plane; C works on row 1, so a negative offset stays in
    /// bounds.
    const ROWS: usize = 3;
    /// A plane C wrote nothing into.
    const ZERO_PLANE: [u8; LINESIZE * ROWS] = [0; LINESIZE * ROWS];

    /// C's table entry for `fmt`, and a Rust copy of it, read through the
    /// safe descriptor lookup.
    fn c_entry(fmt: c_int) -> (*const ffi::AVPixFmtDescriptor, PixelFormatDescriptor) {
        // SAFETY: `av_pix_fmt_desc_get` takes any value and returns NULL or
        // a pointer into C's static table.
        let p = unsafe { ffi::av_pix_fmt_desc_get(ffi::AVPixelFormat(fmt)) };
        assert!(!p.is_null(), "{fmt}");
        let d = PixelFormat::from_raw(ffi::AVPixelFormat(fmt)).descriptor();
        (p, d.expect("C has an entry").get())
    }

    /// The name C reports for `fmt`, as a pointer.
    fn c_name(fmt: c_int) -> *const core::ffi::c_char {
        // SAFETY: `av_get_pix_fmt_name` takes any value and returns NULL or
        // a static string.
        unsafe { ffi::av_get_pix_fmt_name(ffi::AVPixelFormat(fmt)) }
    }

    /// The format C finds for `name`.
    fn c_lookup(name: &CStr) -> c_int {
        // SAFETY: `name` is a live NUL-terminated string, only read.
        unsafe { ffi::av_get_pix_fmt(name.as_ptr()) }.0
    }

    /// Four planes of pseudo-random bytes, so a component read from the
    /// wrong plane, offset, step, shift or depth reads different values.
    fn image() -> [Vec<u8>; 4] {
        /// The planes' bytes, one xorshift stream, computed at compile
        /// time, so an image is four block copies: under BSan every
        /// element a slice iterator yields is a retag whose cost grows with
        /// the allocation's history, and a byte-by-byte fill of one image
        /// took almost a second (the tally below compares whole planes for
        /// the same reason).
        const PLANES: [[u8; LINESIZE * ROWS]; 4] = {
            let mut planes = [[0u8; LINESIZE * ROWS]; 4];
            let mut x = 0x9e37_79b9_u32;
            let mut p = 0;
            while p < planes.len() {
                let mut i = 0;
                while i < LINESIZE * ROWS {
                    x ^= x << 13;
                    x ^= x >> 17;
                    x ^= x << 5;
                    planes[p][i] = x as u8;
                    i += 1;
                }
                p += 1;
            }
            planes
        };
        PLANES.map(|plane| plane.to_vec())
    }

    /// What C's readers of a whole descriptor observe of `desc`.
    #[derive(Debug, PartialEq)]
    struct Observed {
        bits_per_pixel: c_int,
        padded_bits_per_pixel: c_int,
        /// The `W` values `av_read_image_line2` reads from row 1 of the
        /// image, per component slot.
        reads: [[u32; W]; 4],
        /// Four zeroed planes after `av_write_image_line2` wrote `W` values
        /// to row 1, per component slot.
        writes: [[Vec<u8>; 4]; 4],
    }

    /// What C observes of `desc`, a descriptor whose components are each
    /// all-zero, partly written in plane-step-offset-shift-depth order, or
    /// C's table entry for some format.
    fn c_observes(desc: &ffi::AVPixFmtDescriptor, img: &[Vec<u8>; 4]) -> Observed {
        let linesize = [LINESIZE as c_int; 4];
        let reads = core::array::from_fn(|c| {
            let mut data: [*const u8; 4] = core::array::from_fn(|i| img[i].as_ptr());
            let mut dst = [0u32; W];
            // SAFETY: component `c` of `desc` is C's own table entry, addressing planes
            // 0..4 with C's offsets, steps and depths, or has `depth == 0` (it is written
            // depth last), on which C returns before reading; every plane holds `ROWS`
            // rows of `LINESIZE` bytes, enough for `W` pixels of row 1 at the table's
            // widest step and offset. `dst` holds `W` 4-byte elements and
            // `read_pal_component` is 0, so `data[1]` is never indexed as a palette.
            unsafe {
                ffi::av_read_image_line2(
                    dst.as_mut_ptr().cast::<c_void>(),
                    data.as_mut_ptr(),
                    linesize.as_ptr(),
                    desc,
                    0,
                    1,
                    c as c_int,
                    W as c_int,
                    0,
                    4,
                );
            }
            dst
        });
        let writes = core::array::from_fn(|c| {
            let mut planes: [Vec<u8>; 4] = core::array::from_fn(|_| vec![0; LINESIZE * ROWS]);
            let mut data: [*mut u8; 4] = core::array::from_fn(|i| planes[i].as_mut_ptr());
            let depth = desc.comp[c].depth;
            let mask = if depth >= 32 {
                u32::MAX
            } else {
                (1u32 << depth.max(0)) - 1
            };
            let src: [u32; W] = core::array::from_fn(|i| (0x5a5a_5a5a_u32 >> i) & mask);
            // SAFETY: as for the reads; every value fits the component's
            // depth, so C's shifts stay within `int` and `unsigned`.
            unsafe {
                ffi::av_write_image_line2(
                    src.as_ptr().cast::<c_void>(),
                    data.as_mut_ptr(),
                    linesize.as_ptr(),
                    desc,
                    0,
                    1,
                    c as c_int,
                    W as c_int,
                    4,
                );
            }
            planes
        });
        // SAFETY: `desc`'s first `nb_components` components are its table
        // entry's, or all-zero, so their planes index `steps[4]` in bounds,
        // and its chroma shifts and depths are the table's, which C's shifts
        // accept.
        let (bits_per_pixel, padded_bits_per_pixel) = unsafe {
            (
                ffi::av_get_bits_per_pixel(desc),
                ffi::av_get_padded_bits_per_pixel(desc),
            )
        };
        Observed {
            bits_per_pixel,
            padded_bits_per_pixel,
            reads,
            writes,
        }
    }

    fn c_fields(d: &ffi::AVComponentDescriptor) -> [i32; 5] {
        [d.plane, d.step, d.offset, d.shift, d.depth]
    }

    fn rust_fields(d: ComponentDescriptor) -> [i32; 5] {
        let r = d.as_ref();
        [r.plane(), r.step(), r.offset(), r.shift(), r.depth()]
    }

    /// Asserts every getter of `d` reads what `raw` stores.
    fn assert_reads(d: PixelFormatDescriptorRef<'_>, raw: &ffi::AVPixFmtDescriptor, what: &str) {
        let ptr_of = |s: Option<&CStr>| s.map_or(core::ptr::null(), CStr::as_ptr);
        assert_eq!(ptr_of(d.name()), raw.name, "{what}: name");
        assert_eq!(ptr_of(d.alias()), raw.alias, "{what}: alias");
        assert_eq!(d.component_count(), raw.nb_components, "{what}");
        assert_eq!(d.log2_chroma_w(), raw.log2_chroma_w, "{what}");
        assert_eq!(d.log2_chroma_h(), raw.log2_chroma_h, "{what}");
        assert_eq!(d.flags().bits(), raw.flags, "{what}");
        let comps: Vec<[i32; 5]> = d
            .components()
            .iter()
            .map(|c| rust_fields(c.get()))
            .collect();
        let c_comps: Vec<[i32; 5]> = raw.comp.iter().map(c_fields).collect();
        assert_eq!(comps, c_comps, "{what}");
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn getters_read_what_c_stores_and_reports_for_every_entry() {
        let mut empty = 0;
        let mut aliases = 0;
        for fmt in 0..NB {
            let (p, d) = c_entry(fmt);
            // SAFETY: `p` points to a live descriptor in C's static table.
            let raw = unsafe { p.read() };
            assert_reads(d.as_ref(), &raw, &format!("format {fmt}"));
            assert_eq!(
                d.as_ref().name().map_or(core::ptr::null(), CStr::as_ptr),
                c_name(fmt)
            );
            let Some(name) = d.as_ref().name() else {
                // An entry C's table leaves out is all-zero.
                assert_eq!(d, PixelFormatDescriptor::new(), "format {fmt}");
                empty += 1;
                continue;
            };
            // C resolves the name, and every alias, to this entry.
            assert_eq!(c_lookup(name), fmt, "{name:?}");
            if let Some(alias) = d.as_ref().alias() {
                for token in alias.to_bytes().split(|&b| b == b',') {
                    let token = std::ffi::CString::new(token).unwrap();
                    assert_eq!(c_lookup(&token), fmt, "{name:?} alias {token:?}");
                    aliases += 1;
                }
            }
            // C's readers see the same descriptor through a Rust copy.
            let img = image();
            assert_eq!(
                c_observes(&d.as_ref().get().into_raw(), &img),
                c_observes(&raw, &img),
                "{name:?}"
            );
        }
        // C's iteration skips exactly the empty entries.
        let mut iterated = 0;
        let mut q = core::ptr::null();
        loop {
            // SAFETY: `q` is NULL or the previous descriptor
            // `av_pix_fmt_desc_next` returned.
            q = unsafe { ffi::av_pix_fmt_desc_next(q) };
            if q.is_null() {
                break;
            }
            iterated += 1;
        }
        assert_eq!(iterated + empty, NB);
        assert!(aliases > 0);
    }

    /// The C side of a rebuild step: a field assignment from C's entry.
    type CAssign = Box<dyn Fn(&mut ffi::AVPixFmtDescriptor, &ffi::AVPixFmtDescriptor)>;
    /// The Rust side of a rebuild step: a setter call from C's entry.
    type RustSet = Box<dyn Fn(&mut PixelFormatDescriptorMut<'_>, PixelFormatDescriptorRef<'_>)>;

    /// One step of a field-by-field rebuild: a label, the C-side assignment
    /// and the Rust-side setter call, both copying from C's entry.
    struct Step {
        what: String,
        c: CAssign,
        rust: RustSet,
    }

    /// The steps that rebuild a descriptor from all-zero, in the order that
    /// keeps every intermediate state readable by C: strings, flags and
    /// chroma shifts, then each component field by field with its depth
    /// last, then the component count.
    fn steps() -> Vec<Step> {
        fn step(
            what: impl Into<String>,
            c: impl Fn(&mut ffi::AVPixFmtDescriptor, &ffi::AVPixFmtDescriptor) + 'static,
            rust: impl Fn(&mut PixelFormatDescriptorMut<'_>, PixelFormatDescriptorRef<'_>) + 'static,
        ) -> Step {
            Step {
                what: what.into(),
                c: Box::new(c),
                rust: Box::new(rust),
            }
        }
        let mut steps = vec![
            step("name", |c, e| c.name = e.name, |m, e| m.set_name(e.name())),
            step(
                "alias",
                |c, e| c.alias = e.alias,
                |m, e| m.set_alias(e.alias()),
            ),
            step(
                "flags",
                |c, e| c.flags = e.flags,
                |m, e| m.set_flags(e.flags()),
            ),
            step(
                "log2_chroma_w",
                |c, e| c.log2_chroma_w = e.log2_chroma_w,
                |m, e| m.set_log2_chroma_w(e.log2_chroma_w()),
            ),
            step(
                "log2_chroma_h",
                |c, e| c.log2_chroma_h = e.log2_chroma_h,
                |m, e| m.set_log2_chroma_h(e.log2_chroma_h()),
            ),
        ];
        for i in 0..4 {
            let src = move |e: PixelFormatDescriptorRef<'_>| e.components().get(i).unwrap().get();
            steps.extend([
                step(
                    format!("comp[{i}].plane"),
                    move |c, e| c.comp[i].plane = e.comp[i].plane,
                    move |m, e| {
                        m.components_mut()
                            .get_mut(i)
                            .unwrap()
                            .set_plane(src(e).as_ref().plane())
                    },
                ),
                step(
                    format!("comp[{i}].step"),
                    move |c, e| c.comp[i].step = e.comp[i].step,
                    move |m, e| {
                        m.components_mut()
                            .get_mut(i)
                            .unwrap()
                            .set_step(src(e).as_ref().step())
                    },
                ),
                step(
                    format!("comp[{i}].offset"),
                    move |c, e| c.comp[i].offset = e.comp[i].offset,
                    move |m, e| {
                        m.components_mut()
                            .get_mut(i)
                            .unwrap()
                            .set_offset(src(e).as_ref().offset())
                    },
                ),
                step(
                    format!("comp[{i}].shift"),
                    move |c, e| c.comp[i].shift = e.comp[i].shift,
                    move |m, e| {
                        m.components_mut()
                            .get_mut(i)
                            .unwrap()
                            .set_shift(src(e).as_ref().shift())
                    },
                ),
                step(
                    format!("comp[{i}].depth"),
                    move |c, e| c.comp[i].depth = e.comp[i].depth,
                    move |m, e| {
                        m.components_mut()
                            .get_mut(i)
                            .unwrap()
                            .set_depth(src(e).as_ref().depth())
                    },
                ),
            ]);
        }
        steps.push(step(
            "nb_components",
            |c, e| c.nb_components = e.nb_components,
            |m, e| m.set_component_count(e.component_count()),
        ));
        steps
    }

    /// Rebuilds C's entry for `fmt` through `rust`, a handle to all-zero
    /// storage, comparing what C observes of it and of a C-side copy after
    /// every step, and of it and C's entry at the end, which it returns.
    fn rebuild(fmt: c_int, mut rust: PixelFormatDescriptorMut<'_>, img: &[Vec<u8>; 4]) -> Observed {
        let (p, entry) = c_entry(fmt);
        // SAFETY: `p` points to a live descriptor in C's static table.
        let raw = unsafe { p.read() };
        let mut c_side = ffi::AVPixFmtDescriptor::default();
        assert_eq!(
            c_observes(&rust.as_ref().get().into_raw(), img),
            c_observes(&c_side, img),
            "format {fmt} before any step"
        );
        for s in steps() {
            (s.c)(&mut c_side, &raw);
            (s.rust)(&mut rust, entry.as_ref());
            let what = format!("format {fmt} after {}", s.what);
            assert_reads(rust.as_ref(), &c_side, &what);
            assert_eq!(
                c_observes(&rust.as_ref().get().into_raw(), img),
                c_observes(&c_side, img),
                "{what}"
            );
        }
        // Rebuilt in full: equal to C's entry, and C's readers see the
        // entry itself the same way.
        assert_eq!(rust.as_ref().get(), entry, "format {fmt}");
        let observed = c_observes(&rust.as_ref().get().into_raw(), img);
        // SAFETY: `p` points to a live descriptor in C's static table.
        assert_eq!(observed, c_observes(unsafe { &*p }, img), "format {fmt}");
        observed
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn descriptors_rebuilt_through_setters_read_like_c_s_table() {
        let img = image();
        let (mut rebuilt, mut read, mut written) = (0, 0, 0);
        for fmt in 0..NB {
            if c_name(fmt).is_null() {
                continue;
            }
            // Alternate the storage: a local value and an `av_malloc` box.
            let observed = if fmt % 2 == 0 {
                let mut d = PixelFormatDescriptor::new();
                rebuild(fmt, d.as_mut(), &img)
            } else {
                let mut b = HeapBox::<PixelFormatDescriptor>::new_zeroed().unwrap();
                rebuild(fmt, b.as_mut(), &img)
            };
            rebuilt += 1;
            read += usize::from(observed.reads.iter().any(|r| *r != [0; W]));
            written += usize::from(observed.writes.iter().flatten().any(|p| p[..] != ZERO_PLANE[..]));
        }
        // Not vacuous: C reads and writes pixels for every format with
        // components in memory.
        assert!(rebuilt > 200, "{rebuilt}");
        assert!(read > 180 && written > 180, "{read} {written} of {rebuilt}");
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn a_heap_array_of_descriptors_set_whole_reads_like_c_s_table() {
        let img = image();
        let named: Vec<c_int> = (0..NB).filter(|&f| !c_name(f).is_null()).collect();
        let mut arr = alloc_cell_array_zeroed::<PixelFormatDescriptor>(named.len()).unwrap();
        // First pass: every element set whole from C's entry.
        {
            let mut handles = arr.as_handles_mut();
            for (i, &fmt) in named.iter().enumerate() {
                handles.get_mut(i).unwrap().set(c_entry(fmt).1);
            }
        }
        for (h, &fmt) in arr.as_handles().iter().zip(&named) {
            let (p, entry) = c_entry(fmt);
            assert_eq!(h.get(), entry);
            // SAFETY: `p` points to a live descriptor in C's static table.
            let c_side = unsafe { &*p };
            assert_eq!(
                c_observes(&h.get().into_raw(), &img),
                c_observes(c_side, &img),
                "format {fmt}"
            );
        }
        // Second pass: give each element its neighbour's strings and chroma
        // shifts and toggle its byte order, the C side doing the same to
        // plain copies, and compare after the change. These are the fields
        // C's readers accept from any table format: the shifts are at most
        // 2, and byte order only picks which byte of a pixel they read, while
        // another format's bitstream flag would give C negative shifts.
        let len = named.len();
        let mut c_copies: Vec<ffi::AVPixFmtDescriptor> =
            named.iter().map(|&f| c_entry(f).1.into_raw()).collect();
        {
            let mut handles = arr.as_handles_mut();
            for i in 0..len {
                let next = c_entry(named[(i + 1) % len]).1;
                let next = next.as_ref();
                let mut m = handles.get_mut(i).unwrap();
                let flags = m.as_ref().flags();
                let toggled = if flags.contains(PixelFormatFlags::BIG_ENDIAN) {
                    PixelFormatFlags::from_bits_retain(
                        flags.bits() & !PixelFormatFlags::BIG_ENDIAN.bits(),
                    )
                } else {
                    flags | PixelFormatFlags::BIG_ENDIAN
                };
                m.set_name(next.name());
                m.set_alias(next.alias());
                m.set_log2_chroma_w(next.log2_chroma_w());
                m.set_log2_chroma_h(next.log2_chroma_h());
                m.set_flags(toggled);
                let c = &mut c_copies[i];
                let n = next.get().into_raw();
                c.name = n.name;
                c.alias = n.alias;
                c.log2_chroma_w = n.log2_chroma_w;
                c.log2_chroma_h = n.log2_chroma_h;
                c.flags ^= ffi::AV_PIX_FMT_FLAG_BE as u64;
                assert!(n.log2_chroma_w <= 2 && n.log2_chroma_h <= 2);
            }
        }
        for (i, h) in arr.as_handles().iter().enumerate() {
            assert_reads(h, &c_copies[i], &format!("element {i}"));
            assert_eq!(
                c_observes(&h.get().into_raw(), &img),
                c_observes(&c_copies[i], &img),
                "element {i}"
            );
        }
    }
}

/// `PixelFormat`'s descriptor queries against `av_get_pix_fmt_string`,
/// `av_pix_fmt_count_planes` and `av_pix_fmt_get_chroma_sub_sample`, for
/// every pixel format C knows and values around it, with description lines
/// written into shrinking buffers and compared byte for byte after each
/// write.
mod pixel_format_queries {
    use core::ffi::{CStr, c_int};

    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::pixfmt::PixelFormat;

    const NB: c_int = ffi::AVPixelFormat::AV_PIX_FMT_NB.0;

    fn format(raw: c_int) -> PixelFormat {
        PixelFormat::from_raw(ffi::AVPixelFormat(raw))
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn describe_writes_what_c_writes() {
        for raw in (-3..NB).chain([c_int::MIN]) {
            let mut rust_buf = [b'#'; 40];
            let mut c_buf = [b'#'; 40];
            for size in [40, 33, 32, 31, 12, 11, 2, 1, 0] {
                let rust = format(raw)
                    .describe(&mut rust_buf[..size])
                    .map(CStr::to_owned);
                // SAFETY: `size` writable bytes, and a value C has a
                // descriptor for or a negative one (the header).
                let ret = unsafe {
                    ffi::av_get_pix_fmt_string(
                        c_buf.as_mut_ptr().cast(),
                        size as c_int,
                        ffi::AVPixelFormat(raw),
                    )
                };
                assert_eq!(ret.cast::<u8>(), c_buf.as_mut_ptr());
                assert_eq!(rust_buf, c_buf, "{raw} {size}");
                let c_text =
                    (size > 0).then(|| CStr::from_bytes_until_nul(&c_buf).unwrap().to_owned());
                assert_eq!(rust, c_text, "{raw} {size}");
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn plane_count_matches_c() {
        for raw in (-3..NB + 3).chain([c_int::MIN, c_int::MAX]) {
            // SAFETY: by value; C looks the value up through a range check.
            let c = unsafe { ffi::av_pix_fmt_count_planes(ffi::AVPixelFormat(raw)) };
            let c = usize::try_from(c).ok();
            assert_eq!(format(raw).plane_count(), c, "{raw}");
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn chroma_subsampling_matches_c() {
        for raw in (-3..NB + 3).chain([c_int::MIN, c_int::MAX]) {
            let (mut h, mut v) = (-7, -7);
            // SAFETY: two live `int` out-parameters; the value is
            // range-checked by C.
            let ret = unsafe {
                ffi::av_pix_fmt_get_chroma_sub_sample(ffi::AVPixelFormat(raw), &mut h, &mut v)
            };
            let c = (ret >= 0).then_some((h as u8, v as u8));
            if ret < 0 {
                assert_eq!((h, v), (-7, -7));
            }
            assert_eq!(format(raw).chroma_subsampling(), c, "{raw}");
        }
    }
}

mod descriptor_table {
    use core::ffi::{CStr, c_char, c_int};
    use core::ptr;

    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::pixdesc::{
        ComponentDescriptor, PixelFormatDescriptor, PixelFormatDescriptorRef, PixelFormatFlags,
        descriptors,
    };
    use ffmpeg_bindings::libavutil::libavutil::pixfmt::PixelFormat;

    const NB: c_int = ffi::AVPixelFormat::AV_PIX_FMT_NB.0;

    fn format(raw: c_int) -> PixelFormat {
        PixelFormat::from_raw(ffi::AVPixelFormat(raw))
    }

    /// C's descriptor for `raw`, as an address.
    fn c_get(raw: c_int) -> *const ffi::AVPixFmtDescriptor {
        // SAFETY: by value; C range-checks it.
        unsafe { ffi::av_pix_fmt_desc_get(ffi::AVPixelFormat(raw)) }
    }

    /// C's entry at `p`, or NULL, by its contents and its name's address:
    /// every named entry has a name literal of its own, so the address tells
    /// entries apart.
    fn c_ident(
        p: *const ffi::AVPixFmtDescriptor,
    ) -> Option<(PixelFormatDescriptor, *const c_char)> {
        // SAFETY: `p` is NULL or an entry of C's static table, whose names
        // and aliases are NULL or literals, as `from_raw` requires.
        (!p.is_null()).then(|| unsafe { (PixelFormatDescriptor::from_raw(p.read()), (*p).name) })
    }

    /// The same of a descriptor the wrappers returned.
    fn ident(
        d: Option<PixelFormatDescriptorRef<'_>>,
    ) -> Option<(PixelFormatDescriptor, *const c_char)> {
        d.map(|d| (d.get(), d.name().map_or(ptr::null(), CStr::as_ptr)))
    }

    /// The values to look up: every one C has an entry for, its neighbours,
    /// and the extremes.
    fn values() -> impl Iterator<Item = c_int> {
        (-3..NB + 3).chain([c_int::MIN, c_int::MAX])
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn descriptor_is_cs_table_entry() {
        for raw in values() {
            let d = format(raw).descriptor();
            assert_eq!(ident(d), c_ident(c_get(raw)), "{raw}");
            // The entry's own place in C's table, not a copy of it.
            if let Some(d) = d {
                assert_eq!(d.pixel_format(), Some(format(raw)), "{raw}");
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn pixel_format_matches_c_for_every_entry_and_for_copies() {
        for raw in 0..NB {
            let d = format(raw).descriptor().unwrap();
            // SAFETY: C compares the address against its table only.
            let c = unsafe { ffi::av_pix_fmt_desc_get_id(c_get(raw)) };
            assert_eq!(c.0, raw);
            assert_eq!(d.pixel_format(), Some(format(raw)));
            // A copy lies outside C's table, on either side.
            let c_copy = d.get().into_raw();
            // SAFETY: as above; C never reads through the address.
            let c = unsafe { ffi::av_pix_fmt_desc_get_id(&c_copy) };
            assert_eq!(c, ffi::AVPixelFormat::AV_PIX_FMT_NONE);
            assert_eq!(d.get().as_ref().pixel_format(), None);
        }
        let fresh = PixelFormatDescriptor::new();
        assert_eq!(fresh.as_ref().pixel_format(), None);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn iteration_follows_cs_walk_step_by_step() {
        let mut rust = descriptors();
        // SAFETY: NULL starts C's iteration.
        let mut c = unsafe { ffi::av_pix_fmt_desc_next(ptr::null()) };
        let mut steps = 0;
        loop {
            let r = rust.next();
            assert_eq!(ident(r), c_ident(c), "step {steps}");
            let Some(r) = r else { break };
            // From the entry, too.
            let next = r.next_in_table();
            // SAFETY: `c` is the entry C's iteration returned last.
            c = unsafe { ffi::av_pix_fmt_desc_next(c) };
            assert_eq!(ident(next), c_ident(c), "step {steps}");
            steps += 1;
        }
        assert!(rust.next().is_none());
        // Every entry with a name, the first one included.
        let named = (0..NB).filter(|&raw| format(raw).name().is_some()).count();
        assert_eq!(steps, named);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn next_in_table_from_every_entry_matches_c() {
        for raw in 0..NB {
            let p = c_get(raw);
            // SAFETY: `p` is an entry of C's table.
            let c = unsafe { ffi::av_pix_fmt_desc_next(p) };
            let rust = format(raw).descriptor().unwrap().next_in_table();
            assert_eq!(ident(rust), c_ident(c), "{raw}");
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn bit_counts_match_c_for_every_entry() {
        for raw in 0..NB {
            let p = c_get(raw);
            let d = format(raw).descriptor().unwrap();
            // SAFETY: `p` is an entry of C's table, which C's counters read
            // in bounds.
            let (bits, padded) = unsafe {
                (
                    ffi::av_get_bits_per_pixel(p),
                    ffi::av_get_padded_bits_per_pixel(p),
                )
            };
            assert_eq!(d.bits_per_pixel(), Ok(bits as u32), "{raw}");
            assert_eq!(d.padded_bits_per_pixel(), Ok(padded as u32), "{raw}");
            // The same of a Rust-held copy, which C reads as well.
            let copy = d.get();
            assert_eq!(copy.as_ref().bits_per_pixel(), Ok(bits as u32), "{raw}");
            assert_eq!(
                copy.as_ref().padded_bits_per_pixel(),
                Ok(padded as u32),
                "{raw}"
            );
        }
    }

    /// Edge values for a Rust-built descriptor's counted fields.
    const EDGES: [c_int; 9] = [-1, 0, 1, 7, 8, 1 << 20, (1 << 27) - 1, 1 << 28, c_int::MAX];

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn bit_counts_of_rust_built_descriptors_match_c_where_accepted() {
        let (mut accepted, mut refused) = (0, 0);
        for (count, log2) in [
            (1, (0, 0)),
            (3, (1, 1)),
            (4, (2, 2)),
            (4, (31, 0)),
            (4, (16, 16)),
        ] {
            for (i, &a) in EDGES.iter().enumerate() {
                for &b in &EDGES[i..] {
                    for flags in [PixelFormatFlags::empty(), PixelFormatFlags::BITSTREAM] {
                        let mut d = PixelFormatDescriptor::new();
                        let mut m = d.as_mut();
                        m.set_component_count(count);
                        m.set_log2_chroma_w(log2.0);
                        m.set_log2_chroma_h(log2.1);
                        m.set_flags(flags);
                        for (c, mut slot) in m.components_mut().iter_mut().enumerate() {
                            let (v, w) = if c % 2 == 0 { (a, b) } else { (b, a) };
                            slot.set(ComponentDescriptor::from_raw(ffi::AVComponentDescriptor {
                                plane: c as c_int,
                                step: v,
                                offset: 0,
                                shift: 0,
                                depth: w,
                            }));
                        }
                        let r = d.as_ref();
                        let raw = d.into_raw();
                        if let Ok(bits) = r.bits_per_pixel() {
                            // SAFETY: the wrapper found C's arithmetic
                            // defined for this descriptor.
                            let c = unsafe { ffi::av_get_bits_per_pixel(&raw) };
                            assert_eq!(bits, c as u32, "{a} {b} {count} {log2:?}");
                            accepted += 1;
                        } else {
                            refused += 1;
                        }
                        if let Ok(bits) = r.padded_bits_per_pixel() {
                            // SAFETY: as above.
                            let c = unsafe { ffi::av_get_padded_bits_per_pixel(&raw) };
                            assert_eq!(bits, c as u32, "{a} {b} {count} {log2:?}");
                            accepted += 1;
                        } else {
                            refused += 1;
                        }
                    }
                }
            }
        }
        assert!(accepted > 100 && refused > 100, "{accepted} {refused}");
    }
}

mod image_lines {
    use core::ffi::{c_int, c_void};

    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::imgutils::{Planes, PlanesMut};
    use ffmpeg_bindings::libavutil::libavutil::pixdesc::{
        LineElement, PixelFormatDescriptorRef, read_image_line, read_image_line_u16,
        write_image_line, write_image_line_u16,
    };
    use ffmpeg_bindings::libavutil::libavutil::pixfmt::PixelFormat;

    const NB: c_int = ffi::AVPixelFormat::AV_PIX_FMT_NB.0;
    /// Bytes per row: room for 11 pixels of the widest step and offset, and
    /// for a 1024-byte palette in plane 1's first rows.
    const LINESIZE: usize = 512;
    const ROWS: usize = 3;
    const PLANE: usize = LINESIZE * ROWS;
    /// The pixels of a line: `(x, y, w)`.
    const LINES: [(usize, usize, usize); 3] = [(0, 0, 8), (3, 1, 5), (1, 2, 1)];

    fn under_bsan() -> bool {
        std::env::var("CRUSTIFY_FFMPEG_VARIANT").as_deref() == Ok("bsan")
    }

    /// The formats to sweep: every named one, or under BSan, whose cost
    /// grows with every retag, one of each kind of layout.
    fn formats() -> Vec<(c_int, PixelFormatDescriptorRef<'static>)> {
        let all = (0..NB).filter_map(|raw| {
            let d = PixelFormat::from_raw(ffi::AVPixelFormat(raw)).descriptor()?;
            d.name().is_some().then_some((raw, d))
        });
        if under_bsan() {
            let kinds = [
                PixelFormat::YUV420P,
                PixelFormat::RGB565BE,
                PixelFormat::MONOWHITE,
                PixelFormat::PAL8,
                PixelFormat::GRAY16BE,
                PixelFormat::from_raw(ffi::AVPixelFormat::AV_PIX_FMT_X2RGB10LE),
                PixelFormat::from_raw(ffi::AVPixelFormat::AV_PIX_FMT_RGBAF32BE),
                PixelFormat::BGR4,
            ];
            all.filter(|(raw, _)| kinds.iter().any(|k| k.into_raw().0 == *raw))
                .collect()
        } else {
            all.collect()
        }
    }

    /// Four planes of pseudo-random bytes, as a compile-time table copied
    /// whole, so BSan sees four block copies rather than a retag per byte.
    fn image() -> [Vec<u8>; 4] {
        const PLANES: [[u8; PLANE]; 4] = {
            let mut planes = [[0u8; PLANE]; 4];
            let mut x = 0x2545_f491_u32;
            let mut p = 0;
            while p < 4 {
                let mut i = 0;
                while i < PLANE {
                    x ^= x << 13;
                    x ^= x >> 17;
                    x ^= x << 5;
                    planes[p][i] = x as u8;
                    i += 1;
                }
                p += 1;
            }
            planes
        };
        PLANES.map(|p| p.to_vec())
    }

    /// The C planes pointers and linesizes of `img`, top-down or bottom-up
    /// (row 0 last).
    fn c_layout(img: &mut [Vec<u8>; 4], flipped: bool) -> ([*mut u8; 4], [c_int; 4]) {
        let at = if flipped { LINESIZE * (ROWS - 1) } else { 0 };
        let ls = if flipped {
            -(LINESIZE as c_int)
        } else {
            LINESIZE as c_int
        };
        (img.each_mut().map(|p| p[at..].as_mut_ptr()), [ls; 4])
    }

    fn rust_linesizes(flipped: bool) -> [isize; 4] {
        [if flipped {
            -(LINESIZE as isize)
        } else {
            LINESIZE as isize
        }; 4]
    }

    /// C's own descriptor for `raw`.
    fn c_desc(raw: c_int) -> *const ffi::AVPixFmtDescriptor {
        // SAFETY: by value; C range-checks it.
        unsafe { ffi::av_pix_fmt_desc_get(ffi::AVPixelFormat(raw)) }
    }

    /// C's `av_read_image_line2` of the same line.
    fn c_read<T: LineElement + Default>(
        img: &[Vec<u8>; 4],
        flipped: bool,
        d: c_int,
        (x, y, c, w): (usize, usize, usize, usize),
        palette: bool,
    ) -> Vec<T> {
        let mut img = img.clone();
        let (data, linesize) = c_layout(&mut img, flipped);
        let mut data = data.map(|p| p.cast_const());
        let mut dst = vec![T::default(); w];
        // SAFETY: the wrapper accepted the same line of a same-sized image,
        // so every byte C reads lies inside `img`'s planes; `dst` holds `w`
        // elements of `T::SIZE` bytes.
        unsafe {
            ffi::av_read_image_line2(
                dst.as_mut_ptr().cast::<c_void>(),
                data.as_mut_ptr(),
                linesize.as_ptr(),
                c_desc(d),
                x as c_int,
                y as c_int,
                c as c_int,
                w as c_int,
                c_int::from(palette),
                T::SIZE,
            )
        };
        dst
    }

    /// C's `av_write_image_line2` of the same line into `img`.
    fn c_write<T: LineElement>(
        img: &mut [Vec<u8>; 4],
        flipped: bool,
        d: c_int,
        (x, y, c): (usize, usize, usize),
        src: &[T],
    ) {
        let (mut data, linesize) = c_layout(img, flipped);
        // SAFETY: as in `c_read`, for the bytes C reads and writes; `src`
        // holds `w` elements of `T::SIZE` bytes.
        unsafe {
            ffi::av_write_image_line2(
                src.as_ptr().cast::<c_void>(),
                data.as_mut_ptr(),
                linesize.as_ptr(),
                c_desc(d),
                x as c_int,
                y as c_int,
                c as c_int,
                src.len() as c_int,
                T::SIZE,
            )
        };
    }

    fn planes(img: &[Vec<u8>; 4], flipped: bool) -> Planes<'_> {
        Planes {
            data: img.each_ref().map(|p| &p[..]),
            linesizes: rust_linesizes(flipped),
        }
    }

    fn planes_mut(img: &mut [Vec<u8>; 4], flipped: bool) -> PlanesMut<'_> {
        PlanesMut {
            data: img.each_mut().map(|p| &mut p[..]),
            linesizes: rust_linesizes(flipped),
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn reads_match_c_for_every_format_component_and_line() {
        let img = image();
        for (raw, d) in formats() {
            let pal = d
                .flags()
                .contains(ffmpeg_bindings::libavutil::libavutil::pixdesc::PixelFormatFlags::PALETTE);
            for flipped in [false, true] {
                for c in 0..4 {
                    for &(x, y, w) in &LINES {
                        let what = format!("{raw} c{c} ({x}, {y}, {w}) flipped {flipped}");
                        let p = planes(&img, flipped);
                        let mut wide = vec![0u32; w];
                        read_image_line(&mut wide, p, d, x, y, c, false).expect(&what);
                        assert_eq!(
                            wide,
                            c_read::<u32>(&img, flipped, raw, (x, y, c, w), false),
                            "{what}"
                        );
                        let mut narrow = vec![0u16; w];
                        read_image_line(&mut narrow, p, d, x, y, c, false).expect(&what);
                        assert_eq!(
                            narrow,
                            c_read::<u16>(&img, flipped, raw, (x, y, c, w), false),
                            "{what}"
                        );
                        let mut original = vec![0u16; w];
                        read_image_line_u16(&mut original, p, d, x, y, c, false).expect(&what);
                        assert_eq!(original, narrow, "{what}");
                        // Palette lookups, where plane 1 holds every entry.
                        let mut looked_up = vec![0u32; w];
                        let ret = read_image_line(&mut looked_up, p, d, x, y, c, true);
                        if pal && c < 3 && !flipped {
                            ret.expect(&what);
                        }
                        if ret.is_ok() {
                            assert_eq!(
                                looked_up,
                                c_read::<u32>(&img, flipped, raw, (x, y, c, w), true),
                                "{what}"
                            );
                            let mut via_u16 = vec![0u16; w];
                            read_image_line_u16(&mut via_u16, p, d, x, y, c, true).unwrap();
                            assert_eq!(
                                via_u16,
                                c_read::<u16>(&img, flipped, raw, (x, y, c, w), true),
                                "{what}"
                            );
                        }
                    }
                }
            }
        }
    }

    /// The largest value a component of depth `depth` holds.
    fn mask(depth: i32) -> u32 {
        if depth >= 32 {
            u32::MAX
        } else {
            (1u32 << depth.max(0)) - 1
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn write_sequences_match_c_after_every_step() {
        for (raw, d) in formats() {
            for flipped in [false, true] {
                for &(x, y, w) in &LINES {
                    let mut rust = [(); 4].map(|_| vec![0u8; PLANE]);
                    let mut c_img = rust.clone();
                    // Every component in turn, as `u32`s fitting its depth,
                    // then as `u16`s with every bit set, spilling past it.
                    for step in 0..8 {
                        let c = step % 4;
                        let what =
                            format!("{raw} c{c} step {step} ({x}, {y}, {w}) flipped {flipped}");
                        let depth = d.components().get(c).unwrap().depth();
                        if step < 4 {
                            let src: Vec<u32> = (0..w as u32)
                                .map(|i| (0x9e37_79b9_u32 >> i) & mask(depth))
                                .collect();
                            write_image_line(&src, &mut planes_mut(&mut rust, flipped), d, x, y, c)
                                .expect(&what);
                            c_write(&mut c_img, flipped, raw, (x, y, c), &src);
                        } else {
                            let src = vec![u16::MAX; w];
                            if step % 2 == 0 {
                                write_image_line(
                                    &src,
                                    &mut planes_mut(&mut rust, flipped),
                                    d,
                                    x,
                                    y,
                                    c,
                                )
                            } else {
                                write_image_line_u16(
                                    &src,
                                    &mut planes_mut(&mut rust, flipped),
                                    d,
                                    x,
                                    y,
                                    c,
                                )
                            }
                            .expect(&what);
                            c_write(&mut c_img, flipped, raw, (x, y, c), &src);
                        }
                        assert!(rust == c_img, "{what}");
                    }
                    // Reading back what both wrote reads the same.
                    for c in 0..4 {
                        let mut back = vec![0u32; w];
                        read_image_line(&mut back, planes(&rust, flipped), d, x, y, c, false)
                            .unwrap();
                        assert_eq!(
                            back,
                            c_read::<u32>(&c_img, flipped, raw, (x, y, c, w), false),
                            "{raw} c{c}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn pal8_reads_its_palette_like_c() {
        let d = PixelFormat::PAL8.descriptor().unwrap();
        let mut img = image();
        // A 1024-byte palette in plane 1, exactly.
        img[1].truncate(1024);
        for c in 0..4 {
            for &(x, y, w) in &LINES {
                let mut rust = vec![0u16; w];
                read_image_line(&mut rust, planes(&img, false), d, x, y, c, true).unwrap();
                let raw = PixelFormat::PAL8.into_raw().0;
                assert_eq!(
                    rust,
                    c_read::<u16>(&img, false, raw, (x, y, c, w), true),
                    "c{c}"
                );
            }
        }
    }
}

/// The line wrappers' verdicts against an independent model of C: a literal
/// transliteration of `av_read_image_line2`'s and `av_write_image_line2`'s
/// loops, every `int` operation checked for the cases C leaves undefined and
/// every pointer offset tracked in `i64`, decides for a random descriptor,
/// planes and line whether C's accesses are defined and inside the planes.
/// The wrappers must accept exactly those lines (refusing one C would read
/// in bounds is an equivalence defect; accepting one it would not is UB),
/// and accepted lines run on exactly sized heap planes, where a byte C
/// touches past either end is a sanitizer report.
mod random_lines {
    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::imgutils::{Planes, PlanesMut};
    use ffmpeg_bindings::libavutil::libavutil::pixdesc::{
        ComponentDescriptor, ImageLineError, PixelFormatDescriptor, PixelFormatDescriptorRef,
        PixelFormatFlags, read_image_line, read_image_line_u16, write_image_line,
        write_image_line_u16,
    };
    use ffmpeg_bindings::libavutil::libavutil::pixfmt::PixelFormat;

    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x;
            x
        }

        fn below(&mut self, n: u64) -> u64 {
            self.next() % n
        }

        fn pick<T: Copy>(&mut self, xs: &[T]) -> T {
            xs[self.below(xs.len() as u64) as usize]
        }

        fn chance(&mut self, one_in: u64) -> bool {
            self.below(one_in) == 0
        }
    }

    /// The bytes C touches, as offsets from the row-0 pointer of one plane,
    /// and the bytes of plane 1 from its row 0 that palette lookups need
    /// (the largest index plus one, or a negative index).
    #[derive(Debug, Clone, Copy, PartialEq)]
    struct Footprint {
        plane: usize,
        lo: i64,
        hi: i64,
        palette: Option<i64>,
    }

    #[derive(Debug, Clone, Copy, PartialEq)]
    enum Model {
        /// C returns before touching the image.
        Nothing,
        /// C's arithmetic is undefined, or it indexes `data[4]` out of range.
        Undefined(&'static str),
        Touches(Footprint),
    }

    const BE: u32 = ffi::AV_PIX_FMT_FLAG_BE;
    const BITSTREAM: u32 = ffi::AV_PIX_FMT_FLAG_BITSTREAM;

    /// `av_read_image_line2` (or `av_write_image_line2` with `write`) on a
    /// descriptor whose slot `c` is `comp`, over an image of 0xFF bytes, so
    /// every value read, and so every palette index, is its largest.
    fn model(
        comp: [i32; 5],
        flags: u32,
        (x, y, c, w): (i32, i32, i32, i32),
        linesizes: [isize; 4],
        write: bool,
        palette: bool,
    ) -> Model {
        let [plane, step, offset, shift, depth] = comp;
        let palette = palette && !write;
        if w <= 0 {
            // The wrappers return before C runs.
            return Model::Nothing;
        }
        // The reader's `unsigned mask = (1ULL << depth) - 1`, before anything.
        let mut mask: u32 = 0;
        if !write {
            if !(0..64).contains(&depth) {
                return Model::Undefined("1ULL << depth");
            }
            mask = ((1u64 << depth) - 1) as u32;
        }
        if depth == 0 {
            return Model::Nothing;
        }
        let Some(p) = usize::try_from(plane).ok().filter(|&p| p < 4) else {
            return Model::Undefined("data[plane]");
        };
        let Ok(linesize) = i32::try_from(linesizes[p]) else {
            return Model::Undefined("linesize beyond int");
        };
        let Some(row) = y.checked_mul(linesize) else {
            return Model::Undefined("y * linesize");
        };
        let row = i64::from(row);
        let (mut lo, mut hi) = (i64::MAX, i64::MIN);
        let mut touch = |at: i64, n: i64| {
            lo = lo.min(at);
            hi = hi.max(at + n);
        };
        let mut need: Option<i64> = None;
        // `data[1][4 * val + c]`, with `val` an `int` (converted from the
        // unsigned value) or an `unsigned` (wrapping), as each reader has it.
        let mut look_up = |val: u32, int_arithmetic: bool| -> Result<(), &'static str> {
            let index: i64 = if int_arithmetic {
                let v = val as i32;
                let four = v.checked_mul(4).ok_or("4 * val")?;
                i64::from(four.checked_add(c).ok_or("4 * val + c")?)
            } else {
                i64::from(4u32.wrapping_mul(val).wrapping_add(c as u32))
            };
            need = Some(if index < 0 {
                index
            } else {
                need.map_or(index + 1, |n| n.max(index + 1))
            });
            Ok(())
        };
        if flags & BITSTREAM != 0 {
            if step > 8 {
                if write {
                    // `((1ULL << depth) - 1) << offset`, then `val << offset`
                    // on an `unsigned`.
                    if !(0..64).contains(&depth) {
                        return Model::Undefined("1ULL << depth (writer)");
                    }
                    if !(0..32).contains(&offset) {
                        return Model::Undefined("<< offset");
                    }
                } else if !(0..32).contains(&offset) {
                    // `val >> comp.offset` on an `int`.
                    return Model::Undefined("val >> offset");
                }
                for i in 0..i64::from(w) {
                    touch(row + 4 * i, 4);
                    // `int val = AV_RB32(p)` is -1, which `>> offset` keeps.
                    if palette && let Err(e) = look_up(mask, true) {
                        return Model::Undefined(e);
                    }
                }
            } else {
                let Some(skip) = step.checked_mul(x).and_then(|s| s.checked_add(offset)) else {
                    return Model::Undefined("skip");
                };
                let mut at = row + i64::from(skip >> 3);
                let Some(mut sh) = 8i32.checked_sub(depth).and_then(|s| s.checked_sub(skip & 7))
                else {
                    return Model::Undefined("8 - depth - (skip & 7)");
                };
                for _ in 0..w {
                    // `*p >> shift` on the promoted byte, or `val << shift`.
                    if !(0..32).contains(&sh) {
                        return Model::Undefined("shift by a byte");
                    }
                    touch(at, 1);
                    if palette && let Err(e) = look_up((255u32 >> sh) & mask, true) {
                        return Model::Undefined(e);
                    }
                    let Some(next) = sh.checked_sub(step) else {
                        return Model::Undefined("shift -= step");
                    };
                    sh = next;
                    at -= i64::from(sh >> 3);
                    sh &= 7;
                }
            }
        } else {
            let Some(xs) = x.checked_mul(step) else {
                return Model::Undefined("x * step");
            };
            let at = row + i64::from(xs) + i64::from(offset);
            let Some(width) = shift.checked_add(depth) else {
                return Model::Undefined("shift + depth");
            };
            let (size, at) = if width <= 8 {
                (1i64, at + i64::from(flags & BE != 0))
            } else if width <= 16 {
                (2, at)
            } else {
                (4, at)
            };
            // `(val >> shift) & mask` or `s << shift`, on an `unsigned`.
            if !(0..32).contains(&shift) {
                return Model::Undefined("shift");
            }
            for i in 0..i64::from(w) {
                touch(at + i * i64::from(step), size);
                if palette {
                    let largest = (((1u64 << (8 * size)) - 1) as u32) >> shift;
                    if let Err(e) = look_up(largest & mask, false) {
                        return Model::Undefined(e);
                    }
                }
            }
        }
        Model::Touches(Footprint {
            plane: p,
            lo,
            hi,
            palette: need,
        })
    }

    /// The wrappers' row-0 convention: the slice's start, or the last whole
    /// row of a bottom-up plane.
    fn row0(len: usize, linesize: isize) -> usize {
        if linesize >= 0 || len == 0 {
            return 0;
        }
        let l = linesize.unsigned_abs();
        (len - 1) / l * l
    }

    /// Whether C's accesses lie inside the planes.
    fn fits(f: &Footprint, lens: [usize; 4], linesizes: [isize; 4]) -> bool {
        let at = row0(lens[f.plane], linesizes[f.plane]) as i64;
        if at + f.lo < 0 || at + f.hi > lens[f.plane] as i64 {
            return false;
        }
        match f.palette {
            None => true,
            Some(need) if need < 0 => false,
            Some(need) => lens[1] as i64 - row0(lens[1], linesizes[1]) as i64 >= need,
        }
    }

    fn built(comp: [i32; 5], slot: usize, flags: PixelFormatFlags) -> PixelFormatDescriptor {
        let mut d = PixelFormatDescriptor::new();
        let mut m = d.as_mut();
        m.set_component_count(4);
        m.set_flags(flags);
        let [plane, step, offset, shift, depth] = comp;
        m.components_mut()
            .get_mut(slot)
            .unwrap()
            .set(ComponentDescriptor::from_raw(ffi::AVComponentDescriptor {
                plane,
                step,
                offset,
                shift,
                depth,
            }));
        d
    }

    const INTS: [i32; 24] = [
        i32::MIN,
        i32::MIN + 1,
        -(1 << 30),
        -(1 << 28),
        -300,
        -40,
        -9,
        -8,
        -7,
        -3,
        -2,
        -1,
        0,
        1,
        2,
        3,
        4,
        7,
        8,
        9,
        16,
        31,
        32,
        33,
    ];
    const BIG: [i32; 6] = [1 << 20, 1 << 27, (1 << 28) - 1, 1 << 28, 1 << 30, i32::MAX];

    fn small_or_edge(rng: &mut Rng) -> i32 {
        match rng.below(10) {
            0 => rng.pick(&INTS),
            1 => rng.pick(&BIG),
            _ => rng.below(80) as i32 - 40,
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn lines_are_accepted_exactly_where_c_is_defined_and_in_bounds() {
        let bsan = std::env::var("CRUSTIFY_FFMPEG_VARIANT").as_deref() == Ok("bsan");
        let rounds = if bsan { 500 } else { 40_000 };
        let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
        let table: Vec<PixelFormatDescriptorRef<'static>> = (0..ffi::AVPixelFormat::AV_PIX_FMT_NB.0)
            .filter_map(|raw| PixelFormat::from_raw(ffi::AVPixelFormat(raw)).descriptor())
            .filter(|d| d.name().is_some())
            .collect();
        let (mut accepted, mut refused) = (0, 0);
        for round in 0..rounds {
            // C's table, or one Rust-built component at the edges of C's
            // arithmetic.
            let c = rng.below(4) as usize;
            let own;
            let (desc, flags): (PixelFormatDescriptorRef<'_>, u32) = if rng.chance(3) {
                let d = rng.pick(&table);
                (d, d.flags().bits() as u32)
            } else {
                let plane = match rng.below(8) {
                    0 => rng.pick(&[-1, 4, 5, i32::MIN, i32::MAX]),
                    _ => rng.below(4) as i32,
                };
                let step = match rng.below(4) {
                    0 => rng.pick(&INTS),
                    1 => rng.pick(&BIG),
                    _ => rng.below(40) as i32 - 6,
                };
                let offset = small_or_edge(&mut rng);
                let shift = match rng.below(3) {
                    0 => small_or_edge(&mut rng),
                    _ => rng.below(34) as i32 - 1,
                };
                let depth = match rng.below(3) {
                    0 => rng.pick(&[-1, 0, 1, 31, 32, 33, 63, 64, 65, i32::MIN, i32::MAX]),
                    _ => rng.below(34) as i32 - 1,
                };
                let mut fl = PixelFormatFlags::empty();
                if rng.chance(2) {
                    fl |= PixelFormatFlags::BITSTREAM;
                }
                if rng.chance(2) {
                    fl |= PixelFormatFlags::BIG_ENDIAN;
                }
                if rng.chance(4) {
                    fl |= PixelFormatFlags::PALETTE;
                }
                own = built([plane, step, offset, shift, depth], c, fl);
                (own.as_ref(), fl.bits() as u32)
            };
            let comp = {
                let k = desc.components().get(c).unwrap();
                [k.plane(), k.step(), k.offset(), k.shift(), k.depth()]
            };
            let lens: [usize; 4] = core::array::from_fn(|_| match rng.below(6) {
                0 => 0,
                1 => rng.below(8) as usize,
                2 => 1024 + rng.below(64) as usize,
                _ => rng.below(700) as usize,
            });
            let linesizes: [isize; 4] = core::array::from_fn(|_| match rng.below(12) {
                0 => rng.pick(&[
                    isize::MIN,
                    isize::MAX,
                    i32::MIN as isize,
                    i32::MAX as isize,
                    i32::MAX as isize + 1,
                    i32::MIN as isize - 1,
                ]),
                1 => 0,
                2 => rng.below(80) as isize - 80,
                _ => rng.below(160) as isize - 80,
            });
            let position = |rng: &mut Rng| -> usize {
                match rng.below(16) {
                    0 => rng.pick(&[1usize << 20, 1 << 30, i32::MAX as usize, usize::MAX]),
                    _ => rng.below(14) as usize,
                }
            };
            let (x, y) = (position(&mut rng), position(&mut rng));
            let w = match rng.below(16) {
                0 => rng.pick(&[0usize, 1 << 12]),
                _ => rng.below(12) as usize,
            };
            let write = rng.chance(2);
            let palette = !write && rng.chance(2);
            let what = format!(
                "round {round}: comp {comp:?} flags {flags:#x} c {c} lens {lens:?} linesizes \
                 {linesizes:?} (x {x}, y {y}, w {w}) write {write} palette {palette}"
            );
            let modelled = match (i32::try_from(x), i32::try_from(y), i32::try_from(w)) {
                (Ok(x), Ok(y), Ok(w)) => {
                    model(comp, flags, (x, y, c as i32, w), linesizes, write, palette)
                }
                _ => Model::Undefined("position beyond int"),
            };
            let expected = match modelled {
                Model::Nothing => true,
                Model::Undefined(_) => false,
                Model::Touches(f) => fits(&f, lens, linesizes),
            };
            // The wrappers, on exactly sized planes of 0xFF bytes; every
            // entry point agrees with the others.
            let mut img: [Vec<u8>; 4] = lens.map(|n| vec![0xFF; n]);
            let verdict: Result<(), ImageLineError> = if write {
                let src16 = vec![u16::MAX; w];
                let src32 = vec![u32::MAX; w];
                let mut planes = PlanesMut {
                    data: img.each_mut().map(|p| &mut p[..]),
                    linesizes,
                };
                let a = write_image_line(&src32, &mut planes, desc, x, y, c);
                assert_eq!(a, write_image_line(&src16, &mut planes, desc, x, y, c), "{what}");
                assert_eq!(a, write_image_line_u16(&src16, &mut planes, desc, x, y, c), "{what}");
                a
            } else {
                let planes = Planes {
                    data: img.each_ref().map(|p| &p[..]),
                    linesizes,
                };
                let mut wide = vec![0u32; w];
                let mut narrow = vec![0u16; w];
                let mut original = vec![0u16; w];
                let a = read_image_line(&mut wide, planes, desc, x, y, c, palette);
                assert_eq!(a, read_image_line(&mut narrow, planes, desc, x, y, c, palette), "{what}");
                assert_eq!(a, read_image_line_u16(&mut original, planes, desc, x, y, c, palette), "{what}");
                if a.is_ok() {
                    assert_eq!(narrow, original, "{what}");
                    let truncated: Vec<u16> = wide.iter().map(|&v| v as u16).collect();
                    assert_eq!(truncated, narrow, "{what}");
                }
                a
            };
            assert_eq!(verdict.is_ok(), expected, "{what}\n  wrapper: {verdict:?}\n  model: {modelled:?}");
            if verdict.is_ok() && modelled != Model::Nothing {
                accepted += 1;
            } else if verdict.is_err() {
                refused += 1;
            }
        }
        assert!(accepted > rounds / 20 && refused > rounds / 4, "{accepted} {refused}");
    }
}
