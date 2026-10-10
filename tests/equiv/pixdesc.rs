//! Equivalence of `libavutil/pixdesc.h`'s safe API with the C library.

/// [`ComponentDescriptor`] against C's `AVComponentDescriptor`: the safe
/// getters against C's fields for every component of every pixel format
/// descriptor, and descriptors rebuilt through the safe setters, in
/// `av_malloc`-owned storage, against C's own table, as C's readers
/// (`av_read_image_line2`, `av_get_padded_bits_per_pixel`) observe them
/// after each component is written.
///
/// [`ComponentDescriptor`]: ffmpeg::libavutil::pixdesc::ComponentDescriptor
mod component_descriptor {
    use core::ffi::{CStr, c_int, c_void};
    use core::ptr;

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::mem::alloc_cell_array_zeroed;
    use ffmpeg::libavutil::pixdesc::{
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
        // SAFETY: `desc` is a live descriptor whose components are each
        // either C's own table entry for its format, addressing planes 0..4
        // with C's offsets, steps and depths, or all-zero, whose `depth` of 0
        // makes C return before reading; every plane holds
        // `ROWS` rows of `LINESIZE` bytes, enough for `W` pixels of row 1 at
        // the table's widest step and offset. `dst` holds `W` 4-byte
        // elements and `read_pal_component` is 0, so `data[1]` is never
        // indexed as a palette. `av_get_padded_bits_per_pixel` reads
        // `desc`'s first `nb_components` components, whose planes are 0..4.
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

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::pixfmt::{
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
    fn lookups_match_c_for_every_candidate() {
        for name in candidates() {
            for (i, (rust, c)) in lookups(&name).into_iter().enumerate() {
                assert_eq!(rust, c, "property {i}, {name:?}");
            }
        }
    }

    #[test]
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
/// [`PixelFormat::swap_endianness`]: ffmpeg::libavutil::pixfmt::PixelFormat::swap_endianness
mod swap_endianness {
    use core::ffi::c_int;

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::pixfmt::PixelFormat;

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

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::pixfmt::{
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
    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::pixfmt::ChromaLocation;

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

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::pixfmt::PixelFormat;

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
    fn names_map_back_on_both_sides() {
        for raw in 0..nb() {
            let f = PixelFormat::from_raw(ffi::AVPixelFormat(raw));
            let name = f.name().unwrap();
            assert_eq!(lookups(name), (raw, raw), "{name:?}");
            assert_eq!(PixelFormat::from_name(name), Some(f));
        }
    }

    #[test]
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

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::pixdesc::Loss;
    use ffmpeg::libavutil::pixfmt::PixelFormat;

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
/// [`PixelFormatDescriptor`]: ffmpeg::libavutil::pixdesc::PixelFormatDescriptor
mod pixel_format_descriptor {
    use core::ffi::{CStr, c_int, c_void};

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::mem::{HeapBox, alloc_cell_array_zeroed};
    use ffmpeg::libavutil::pixdesc::{
        ComponentDescriptor, PixelFormatDescriptor, PixelFormatDescriptorMut,
        PixelFormatDescriptorRef, PixelFormatFlags,
    };

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

    /// C's table entry for `fmt`, and a Rust value adopted from a copy of it.
    ///
    /// The one unsafe fixture for this type: no safe constructor reaches C's
    /// table until `av_pix_fmt_desc_get` is wrapped.
    fn c_entry(fmt: c_int) -> (*const ffi::AVPixFmtDescriptor, PixelFormatDescriptor) {
        // SAFETY: `av_pix_fmt_desc_get` takes any value and returns NULL or
        // a pointer into C's static table.
        let p = unsafe { ffi::av_pix_fmt_desc_get(ffi::AVPixelFormat(fmt)) };
        assert!(!p.is_null(), "{fmt}");
        // SAFETY: `p` points to a live descriptor in C's static,
        // never-written table, whose `name` and `alias` are NULL or string
        // literals, as `from_raw` requires.
        (p, unsafe { PixelFormatDescriptor::from_raw(p.read()) })
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
        let mut x = 0x9e37_79b9_u32;
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
            // SAFETY: component `c` of `desc` is C's own table entry,
            // addressing planes 0..4 with C's offsets, steps and depths, or
            // has `depth == 0` (it is written depth last), on which C returns
            // before reading; every plane holds `ROWS` rows of `LINESIZE`
            // bytes, enough for `W` pixels of row 1 at the table's widest
            // step and offset. `dst` holds `W` 4-byte elements and
            // `read_pal_component` is 0, so `data[1]` is never indexed as a
            // palette.
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
            written += usize::from(observed.writes.iter().flatten().flatten().any(|&b| b != 0));
        }
        // Not vacuous: C reads and writes pixels for every format with
        // components in memory.
        assert!(rebuilt > 200, "{rebuilt}");
        assert!(read > 180 && written > 180, "{read} {written} of {rebuilt}");
    }

    #[test]
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

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::pixfmt::PixelFormat;

    const NB: c_int = ffi::AVPixelFormat::AV_PIX_FMT_NB.0;

    fn format(raw: c_int) -> PixelFormat {
        PixelFormat::from_raw(ffi::AVPixelFormat(raw))
    }

    #[test]
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
    fn plane_count_matches_c() {
        for raw in (-3..NB + 3).chain([c_int::MIN, c_int::MAX]) {
            // SAFETY: by value; C looks the value up through a range check.
            let c = unsafe { ffi::av_pix_fmt_count_planes(ffi::AVPixelFormat(raw)) };
            let c = usize::try_from(c).ok();
            assert_eq!(format(raw).plane_count(), c, "{raw}");
        }
    }

    #[test]
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
