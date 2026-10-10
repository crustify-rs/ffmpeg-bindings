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
