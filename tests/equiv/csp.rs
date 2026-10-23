//! Equivalence of `libavutil/csp.h`'s safe API with the C library.
#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "test inputs are chosen within range, and the casts mirror the C conversions under comparison"
)]
#![expect(
    clippy::many_single_char_names,
    clippy::struct_field_names,
    reason = "names follow the C fields, formulas and fixtures they mirror"
)]

/// [`CieXy`] against C's `AVCIExy`: the safe getters against C's fields for
/// the white point and primaries of every gamut C describes
/// (`av_csp_primaries_desc_from_id`), gamuts rebuilt coordinate by coordinate
/// through the safe setters, in `av_malloc`-owned storage, against the same
/// writes to an independent C struct, as `av_csp_primaries_id_from_desc`
/// identifies them after each step, and equality against C's `av_cmp_q` on
/// both coordinates.
///
/// [`CieXy`]: ffmpeg_bindings::libavutil::libavutil::csp::CieXy
mod cie_xy {
    use core::mem::{align_of, offset_of, size_of};

    use ffmpeg_bindings::libavutil::libavutil::csp::{CieXy, CieXyMut, CieXyRef};
    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::mem::{HeapVec, alloc_cell_array_zeroed};
    use ffmpeg_bindings::libavutil::libavutil::rational::Rational;

    /// Every primaries value C may describe, past the end of both ranges
    /// included.
    fn primaries() -> impl Iterator<Item = ffi::AVColorPrimaries> {
        let nb = ffi::AVColorPrimaries::AVCOL_PRI_NB.0;
        let ext = ffi::AVColorPrimaries::AVCOL_PRI_EXT_BASE.0;
        let ext_nb = ffi::AVColorPrimaries::AVCOL_PRI_EXT_NB.0;
        (0..=nb).chain(ext..=ext_nb).map(ffi::AVColorPrimaries)
    }

    /// Copies of every gamut description C holds, with its primaries value.
    fn c_gamuts() -> Vec<(ffi::AVColorPrimaries, ffi::AVColorPrimariesDesc)> {
        primaries()
            .filter_map(|p| {
                // SAFETY: `av_csp_primaries_desc_from_id` takes any value and
                // returns NULL or a pointer into C's static, never-written
                // tables.
                let d = unsafe { ffi::av_csp_primaries_desc_from_id(p) };
                // SAFETY: a non-NULL result is a live, initialised
                // description; the copy is plain data.
                (!d.is_null()).then(|| (p, unsafe { d.read() }))
            })
            .collect()
    }

    /// The four chromaticities of a description, white point first.
    fn c_points(d: &ffi::AVColorPrimariesDesc) -> [ffi::AVCIExy; 4] {
        [d.wp, d.prim.r, d.prim.g, d.prim.b]
    }

    fn c_parts(c: &ffi::AVCIExy) -> [i32; 4] {
        [c.x.num, c.x.den, c.y.num, c.y.den]
    }

    fn rust_parts(xy: CieXyRef<'_>) -> [i32; 4] {
        let (x, y) = (xy.x(), xy.y());
        [x.num(), x.den(), y.num(), y.den()]
    }

    /// The description C sees for four Rust chromaticities, white point
    /// first, each copied out through the safe API.
    fn desc_of(points: &HeapVec<CieXy>) -> ffi::AVColorPrimariesDesc {
        let h = points.as_handles();
        let p = |i: usize| h.get(i).unwrap().get().into_raw();
        ffi::AVColorPrimariesDesc {
            wp: p(0),
            prim: ffi::AVPrimaryCoefficients {
                r: p(1),
                g: p(2),
                b: p(3),
            },
        }
    }

    /// C's identification of `d`.
    fn c_id(d: &ffi::AVColorPrimariesDesc) -> ffi::AVColorPrimaries {
        // SAFETY: `d` is a live, initialised description, which C only reads
        // during the call.
        unsafe { ffi::av_csp_primaries_id_from_desc(d) }
    }

    /// C's `av_cmp_q(a, b) == 0`.
    fn c_q_equal(a: ffi::AVRational, b: ffi::AVRational) -> bool {
        // SAFETY: `av_cmp_q` takes both by value and has no preconditions.
        unsafe { ffi::av_cmp_q(a, b) == 0 }
    }

    /// One coordinate part of C's struct, by index into `c_parts` order.
    fn c_part_mut(c: &mut ffi::AVCIExy, i: usize) -> &mut i32 {
        match i {
            0 => &mut c.x.num,
            1 => &mut c.x.den,
            2 => &mut c.y.num,
            _ => &mut c.y.den,
        }
    }

    /// The same part through the safe coordinate handles.
    fn rust_set_part(m: &mut CieXyMut<'_>, i: usize, v: i32) {
        match i {
            0 => m.x_mut().set_num(v),
            1 => m.x_mut().set_den(v),
            2 => m.y_mut().set_num(v),
            _ => m.y_mut().set_den(v),
        }
    }

    #[test]
    fn layout_matches_c() {
        assert_eq!(size_of::<CieXy>(), size_of::<ffi::AVCIExy>());
        assert_eq!(align_of::<CieXy>(), align_of::<ffi::AVCIExy>());
        assert_eq!(offset_of!(ffi::AVCIExy, x), 0);
        assert_eq!(offset_of!(ffi::AVCIExy, y), size_of::<ffi::AVRational>());
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn getters_read_what_c_stores_for_every_gamut() {
        let gamuts = c_gamuts();
        // BT.709 .. EBU 3213 less the unspecified and reserved slots, and
        // V-Gamut.
        assert!(gamuts.len() >= 11, "C describes {} gamuts", gamuts.len());
        for (p, d) in &gamuts {
            for c in c_points(d) {
                let v = CieXy::from_raw(c);
                assert_eq!(rust_parts(v.as_ref()), c_parts(&c), "{p:?}");
                assert_eq!(rust_parts(v.as_ref().get().as_ref()), c_parts(&c));
                assert_eq!(c_parts(&v.into_raw()), c_parts(&c));
                let built = CieXy::new(Rational::from_raw(c.x), Rational::from_raw(c.y));
                assert_eq!(rust_parts(built.as_ref()), c_parts(&c));
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn gamuts_rebuilt_through_the_setters_identify_as_c_after_each_step() {
        for (p, reference) in c_gamuts() {
            // Rust's four points in an `av_malloc` block, C's in its own
            // struct: independently owned, written in the same order.
            let mut points = alloc_cell_array_zeroed::<CieXy>(4).unwrap();
            let mut c = ffi::AVColorPrimariesDesc::default();
            assert_eq!(c_id(&desc_of(&points)), c_id(&c));
            for (k, target) in c_points(&reference).iter().enumerate() {
                for i in 0..4 {
                    let v = c_parts(target)[i];
                    rust_set_part(&mut points.as_handles_mut().get_mut(k).unwrap(), i, v);
                    let mut cp = c_points(&c);
                    *c_part_mut(&mut cp[k], i) = v;
                    [c.wp, c.prim.r, c.prim.g, c.prim.b] = cp;
                    let rust = desc_of(&points);
                    assert_eq!(
                        c_points(&rust).map(|x| c_parts(&x)),
                        cp.map(|x| c_parts(&x))
                    );
                    assert_eq!(c_id(&rust), c_id(&c), "{p:?} point {k} part {i}");
                }
            }
            let id = c_id(&c);
            if p.0 < ffi::AVColorPrimaries::AVCOL_PRI_NB.0 {
                // C identifies a gamut by the first table entry within its
                // tolerance (SMPTE 240M is SMPTE 170M's twin).
                // SAFETY: `av_csp_primaries_desc_from_id` takes any value.
                assert!(!unsafe { ffi::av_csp_primaries_desc_from_id(id) }.is_null());
            }
            // Whole points through `set`, `set_x` and `set_y`, then a nudge
            // of the red x past C's 1/1000 tolerance and back.
            let mut handles = points.as_handles_mut();
            handles
                .get_mut(0)
                .unwrap()
                .set(CieXy::from_raw(reference.wp));
            handles
                .get_mut(1)
                .unwrap()
                .set_x(Rational::from_raw(reference.prim.r.x));
            handles
                .get_mut(1)
                .unwrap()
                .set_y(Rational::from_raw(reference.prim.r.y));
            assert_eq!(c_id(&desc_of(&points)), id, "{p:?}");
            let r = reference.prim.r.x;
            for (num, den) in [(r.num * 10 + r.den / 50, r.den * 10), (r.num, r.den)] {
                points
                    .as_handles_mut()
                    .get_mut(1)
                    .unwrap()
                    .set_x(Rational::new(num, den));
                c.prim.r.x = ffi::AVRational { num, den };
                assert_eq!(
                    c_id(&desc_of(&points)),
                    c_id(&c),
                    "{p:?} nudged {num}/{den}"
                );
                if num != r.num && p.0 < ffi::AVColorPrimaries::AVCOL_PRI_NB.0 {
                    assert_ne!(c_id(&c), id, "{p:?}: the nudge leaves C's tolerance");
                }
            }
            assert_eq!(c_id(&c), id);
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn equality_is_c_comparison_on_both_coordinates() {
        let mut points: Vec<ffi::AVCIExy> =
            c_gamuts().iter().flat_map(|(_, d)| c_points(d)).collect();
        let q = |num, den| ffi::AVRational { num, den };
        points.extend([
            ffi::AVCIExy {
                x: q(0, 0),
                y: q(0, 0),
            },
            ffi::AVCIExy {
                x: q(0, 0),
                y: q(1, 3),
            },
            ffi::AVCIExy {
                x: q(1, 0),
                y: q(-1, 0),
            },
            ffi::AVCIExy {
                x: q(2, 0),
                y: q(-5, 0),
            },
            ffi::AVCIExy {
                x: q(3127, 10000),
                y: q(6580, 20000),
            },
            ffi::AVCIExy {
                x: q(i32::MAX, 1),
                y: q(i32::MIN, 1),
            },
        ]);
        let mut equal = 0;
        for a in &points {
            for b in &points {
                let c = c_q_equal(a.x, b.x) && c_q_equal(a.y, b.y);
                assert_eq!(CieXy::from_raw(*a) == CieXy::from_raw(*b), c, "{a:?} {b:?}");
                equal += usize::from(c);
            }
        }
        // Shared white points across gamuts make some distinct pairs equal.
        assert!(equal > points.len(), "{equal}");
    }
}

/// [`LumaCoefficients`] against C's `AVLumaCoefficients`: the safe getters,
/// over C's own static table (`av_csp_luma_coeffs_from_avcsp`) and over
/// copies of it, against C's fields for every colour space, and twin values
/// built field by field, by C-layout writes and through the safe field
/// handles in `av_malloc`-owned storage, compared after every write.
///
/// [`LumaCoefficients`]: ffmpeg_bindings::libavutil::libavutil::csp::LumaCoefficients
mod luma_coefficients {
    use core::ptr::NonNull;

    use ffibox::CSlice;
    use ffmpeg_bindings::libavutil::libavutil::csp::{LumaCoefficients, LumaCoefficientsRef};
    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::mem::HeapBox;
    use ffmpeg_bindings::libavutil::libavutil::rational::Rational;

    /// Every colour space id, out-of-range ones on both sides included.
    fn spaces() -> impl Iterator<Item = ffi::AVColorSpace> {
        let nb = ffi::AVColorSpace::AVCOL_SPC_NB.0 as i32;
        (-2..nb + 3).map(|v| ffi::AVColorSpace(v as _))
    }

    /// C's table entry for `csp`, if it has one, as a safe handle over C's
    /// static storage: the test's one adoption of C-owned
    /// `AVLumaCoefficients`.
    fn c_entry(csp: ffi::AVColorSpace) -> Option<LumaCoefficientsRef<'static>> {
        // SAFETY: takes any value and returns NULL or a pointer into C's
        // static, never-written table.
        let p = unsafe { ffi::av_csp_luma_coeffs_from_avcsp(csp) };
        let p = NonNull::new(p.cast_mut())?;
        // SAFETY: `p` addresses one live, initialised, never-written entry of
        // a static table; `LumaCoefficients` is transparent over
        // `AVLumaCoefficients`.
        let one: CSlice<'static, LumaCoefficients> = unsafe { CSlice::from_raw_parts(p.cast(), 1) };
        one.get(0)
    }

    /// One write, to the C side and to the Rust side.
    type Step<'a> = dyn Fn(&mut ffi::AVLumaCoefficients, &mut HeapBox<LumaCoefficients>) + 'a;

    fn c_fields(c: &ffi::AVLumaCoefficients) -> [i32; 6] {
        [c.cr.num, c.cr.den, c.cg.num, c.cg.den, c.cb.num, c.cb.den]
    }

    fn rust_fields(c: LumaCoefficientsRef<'_>) -> [i32; 6] {
        [
            c.cr().num(),
            c.cr().den(),
            c.cg().num(),
            c.cg().den(),
            c.cb().num(),
            c.cb().den(),
        ]
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn getters_read_c_s_table_for_every_colour_space() {
        let mut found = 0;
        for csp in spaces() {
            // SAFETY: as in `c_entry`.
            let p = unsafe { ffi::av_csp_luma_coeffs_from_avcsp(csp) };
            let entry = c_entry(csp);
            assert_eq!(entry.is_some(), !p.is_null(), "{csp:?}");
            let Some(entry) = entry else { continue };
            // SAFETY: `p` is a live entry of C's static table.
            let raw = unsafe { p.read() };
            assert_eq!(rust_fields(entry), c_fields(&raw), "{csp:?}");
            assert_eq!(rust_fields(entry.get().as_ref()), c_fields(&raw));
            let copy = LumaCoefficients::from_raw(raw);
            assert_eq!(rust_fields(copy.as_ref()), c_fields(&raw));
            assert_eq!(c_fields(&copy.into_raw()), c_fields(&raw));
            // C hands out only entries with a numerator for red.
            assert_ne!(entry.cr().num(), 0);
            found += 1;
        }
        // FCC, BT470BG, SMPTE170M, BT709, SMPTE240M, YCOCG, RGB, BT2020 x2.
        assert_eq!(found, 9);
        let bt709 = c_entry(ffi::AVColorSpace::AVCOL_SPC_BT709).unwrap();
        assert!((bt709.cr().get().to_f64() - 0.2126).abs() < 1e-9);
        assert!((bt709.cg().get().to_f64() - 0.7152).abs() < 1e-9);
        assert!((bt709.cb().get().to_f64() - 0.0722).abs() < 1e-9);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn equality_matches_c_s_comparison_of_every_coefficient() {
        let mut entries: Vec<ffi::AVLumaCoefficients> = spaces()
            .filter_map(c_entry)
            .map(|e| e.get().into_raw())
            .collect();
        // Unreduced twins and a `0/0` entry, beside C's own.
        let mut twin = entries[0];
        twin.cg.num *= 3;
        twin.cg.den *= 3;
        entries.push(twin);
        entries.push(LumaCoefficients::default().into_raw());
        let mut equal = 0;
        for a in &entries {
            for b in &entries {
                // SAFETY: `av_cmp_q` takes any two rationals by value.
                let c_eq = unsafe { ffi::av_cmp_q(a.cr, b.cr) } == 0
                    && unsafe { ffi::av_cmp_q(a.cg, b.cg) } == 0
                    && unsafe { ffi::av_cmp_q(a.cb, b.cb) } == 0;
                let rust_eq = LumaCoefficients::from_raw(*a) == LumaCoefficients::from_raw(*b);
                assert_eq!(rust_eq, c_eq, "{a:?} {b:?}");
                equal += usize::from(rust_eq);
            }
        }
        // Each entry equals itself, but the `0/0` one; BT.470BG, SMPTE170M
        // and the two BT.2020s pair up; the twin equals its source.
        assert_eq!(equal, entries.len() - 1 + 2 + 2 + 2);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn values_written_through_field_handles_match_c_writes() {
        for csp in spaces() {
            let Some(entry) = c_entry(csp) else { continue };
            let target = entry.get().into_raw();
            // C side: a zeroed struct, then C-layout writes, part by part.
            let mut c_side = ffi::AVLumaCoefficients {
                cr: ffi::AVRational { num: 0, den: 0 },
                cg: ffi::AVRational { num: 0, den: 0 },
                cb: ffi::AVRational { num: 0, den: 0 },
            };
            // Rust side: an `av_malloc`-owned value written through the
            // field handles.
            let mut rust_side = HeapBox::<LumaCoefficients>::new_zeroed().unwrap();
            assert_eq!(rust_fields(rust_side.as_ref()), c_fields(&c_side));
            let steps: [&Step<'_>; 8] = [
                &|c, r| {
                    c.cr.num = target.cr.num;
                    r.as_mut().cr_mut().set_num(target.cr.num);
                },
                &|c, r| {
                    c.cr.den = target.cr.den;
                    r.as_mut().cr_mut().set_den(target.cr.den);
                },
                &|c, r| {
                    c.cg = target.cg;
                    r.as_mut().cg_mut().set(Rational::from_raw(target.cg));
                },
                &|c, r| {
                    c.cb.den = target.cb.den;
                    r.as_mut().cb_mut().set_den(target.cb.den);
                },
                &|c, r| {
                    c.cr = target.cg;
                    c.cg = target.cb;
                    let mut m = r.as_mut();
                    m.set_cr(Rational::from_raw(target.cg));
                    m.set_cg(Rational::from_raw(target.cb));
                },
                &|c, r| {
                    c.cr = target.cr;
                    c.cg = target.cg;
                    c.cb = target.cb;
                    let mut m = r.as_mut();
                    m.set_cr(Rational::from_raw(target.cr));
                    m.set_cb(Rational::from_raw(target.cb));
                    m.set_cg(Rational::from_raw(target.cg));
                },
                &|c, r| {
                    c.cb.num = target.cb.num;
                    r.as_mut().cb_mut().set_num(target.cb.num);
                },
                &|c, r| {
                    *c = target;
                    r.as_mut().set(LumaCoefficients::from_raw(target));
                },
            ];
            for (n, step) in steps.iter().enumerate() {
                step(&mut c_side, &mut rust_side);
                assert_eq!(
                    rust_fields(rust_side.as_ref()),
                    c_fields(&c_side),
                    "{csp:?} step {n}"
                );
            }
            assert_eq!(rust_fields(rust_side.as_ref()), rust_fields(entry));
            // A clone (`av_memdup`) and a value built by `new` read alike.
            let cloned = rust_side.clone();
            drop(rust_side);
            assert_eq!(rust_fields(cloned.as_ref()), c_fields(&target));
            let built = LumaCoefficients::new(entry.cr().get(), entry.cg().get(), entry.cb().get());
            assert_eq!(c_fields(&built.into_raw()), c_fields(&target));
        }
    }
}

/// [`PrimaryCoefficients`] against C's `AVPrimaryCoefficients`: the safe
/// getters against the primaries of every gamut C describes
/// (`av_csp_primaries_desc_from_id`), read in place in C's static table;
/// primaries rebuilt coordinate by coordinate and primary by primary through
/// the safe setters, in `av_malloc`-owned storage, against the same writes to
/// an independent C struct, as `av_csp_primaries_id_from_desc` identifies the
/// gamut after each step; and equality against C's `av_cmp_q` on all six
/// coordinates.
///
/// [`PrimaryCoefficients`]: ffmpeg_bindings::libavutil::libavutil::csp::PrimaryCoefficients
mod primary_coefficients {
    use core::mem::{align_of, offset_of, size_of};

    use ffmpeg_bindings::libavutil::libavutil::csp::{
        CieXy, PrimaryCoefficients, PrimaryCoefficientsMut, PrimaryCoefficientsRef,
    };
    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::mem::HeapBox;
    use ffmpeg_bindings::libavutil::libavutil::pixfmt::ColorPrimaries;
    use ffmpeg_bindings::libavutil::libavutil::rational::Rational;

    /// Every primaries value C may describe, past the end of both ranges
    /// included.
    fn primaries() -> impl Iterator<Item = ffi::AVColorPrimaries> {
        let nb = ffi::AVColorPrimaries::AVCOL_PRI_NB.0;
        let ext = ffi::AVColorPrimaries::AVCOL_PRI_EXT_BASE.0;
        let ext_nb = ffi::AVColorPrimaries::AVCOL_PRI_EXT_NB.0;
        (0..=nb).chain(ext..=ext_nb).map(ffi::AVColorPrimaries)
    }

    /// The primaries of C's description for `p`, if it has one, read in
    /// place in C's static storage through the safe lookup.
    fn c_entry(p: ffi::AVColorPrimaries) -> Option<PrimaryCoefficientsRef<'static>> {
        ColorPrimaries::from_raw(p)
            .descriptor()
            .map(|d| d.primaries())
    }

    /// Copies of every gamut description C holds, with its primaries value.
    fn c_gamuts() -> Vec<(ffi::AVColorPrimaries, ffi::AVColorPrimariesDesc)> {
        primaries()
            .filter_map(|p| {
                // SAFETY: as in `c_entry`.
                let d = unsafe { ffi::av_csp_primaries_desc_from_id(p) };
                // SAFETY: a non-NULL result is a live, initialised
                // description; the copy is plain data.
                (!d.is_null()).then(|| (p, unsafe { d.read() }))
            })
            .collect()
    }

    fn c_fields(c: &ffi::AVPrimaryCoefficients) -> [i32; 12] {
        let mut out = [0; 12];
        for (i, xy) in [c.r, c.g, c.b].iter().enumerate() {
            out[4 * i..4 * i + 4].copy_from_slice(&[xy.x.num, xy.x.den, xy.y.num, xy.y.den]);
        }
        out
    }

    fn rust_fields(p: PrimaryCoefficientsRef<'_>) -> [i32; 12] {
        let mut out = [0; 12];
        for (i, xy) in [p.r(), p.g(), p.b()].into_iter().enumerate() {
            let (x, y) = (xy.x(), xy.y());
            out[4 * i..4 * i + 4].copy_from_slice(&[x.num(), x.den(), y.num(), y.den()]);
        }
        out
    }

    /// Part `i` (in `c_fields` order) of C's struct.
    fn c_part_mut(c: &mut ffi::AVPrimaryCoefficients, i: usize) -> &mut i32 {
        let xy = match i / 4 {
            0 => &mut c.r,
            1 => &mut c.g,
            _ => &mut c.b,
        };
        match i % 4 {
            0 => &mut xy.x.num,
            1 => &mut xy.x.den,
            2 => &mut xy.y.num,
            _ => &mut xy.y.den,
        }
    }

    /// The same part through the safe primary and coordinate handles.
    fn rust_set_part(m: &mut PrimaryCoefficientsMut<'_>, i: usize, v: i32) {
        let mut xy = match i / 4 {
            0 => m.r_mut(),
            1 => m.g_mut(),
            _ => m.b_mut(),
        };
        match i % 4 {
            0 => xy.x_mut().set_num(v),
            1 => xy.x_mut().set_den(v),
            2 => xy.y_mut().set_num(v),
            _ => xy.y_mut().set_den(v),
        }
    }

    /// C's identification of the gamut of white point `wp` and primaries
    /// `prim`.
    fn c_id(wp: ffi::AVCIExy, prim: ffi::AVPrimaryCoefficients) -> ffi::AVColorPrimaries {
        let d = ffi::AVColorPrimariesDesc { wp, prim };
        // SAFETY: `d` is a live, initialised description, which C only reads
        // during the call.
        unsafe { ffi::av_csp_primaries_id_from_desc(&raw const d) }
    }

    #[test]
    fn layout_matches_c() {
        assert_eq!(
            size_of::<PrimaryCoefficients>(),
            size_of::<ffi::AVPrimaryCoefficients>()
        );
        assert_eq!(
            align_of::<PrimaryCoefficients>(),
            align_of::<ffi::AVPrimaryCoefficients>()
        );
        assert_eq!(
            offset_of!(ffi::AVPrimaryCoefficients, g),
            size_of::<ffi::AVCIExy>()
        );
        assert_eq!(
            offset_of!(ffi::AVColorPrimariesDesc, prim),
            size_of::<ffi::AVCIExy>()
        );
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn getters_read_c_s_table_for_every_gamut() {
        let gamuts = c_gamuts();
        assert!(gamuts.len() >= 11, "C describes {} gamuts", gamuts.len());
        for (p, d) in &gamuts {
            let entry = c_entry(*p).unwrap();
            assert_eq!(rust_fields(entry), c_fields(&d.prim), "{p:?}");
            assert_eq!(rust_fields(entry.get().as_ref()), c_fields(&d.prim));
            let copy = PrimaryCoefficients::from_raw(d.prim);
            assert_eq!(rust_fields(copy.as_ref()), c_fields(&d.prim));
            assert_eq!(c_fields(&copy.into_raw()), c_fields(&d.prim));
            let built = PrimaryCoefficients::new(
                CieXy::from_raw(d.prim.r),
                CieXy::from_raw(d.prim.g),
                CieXy::from_raw(d.prim.b),
            );
            assert_eq!(rust_fields(built.as_ref()), c_fields(&d.prim));
            // C hands out only described gamuts, red x numerator set.
            assert_ne!(entry.r().x().num(), 0);
        }
        let bt709 = c_entry(ffi::AVColorPrimaries::AVCOL_PRI_BT709).unwrap();
        assert!((bt709.g().y().get().to_f64() - 0.60).abs() < 1e-9);
        assert!(c_entry(ffi::AVColorPrimaries::AVCOL_PRI_UNSPECIFIED).is_none());
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn primaries_rebuilt_through_the_setters_identify_as_c_after_each_step() {
        for (p, reference) in c_gamuts() {
            // Rust's primaries in an `av_malloc` block, C's in its own
            // struct: independently owned, written in the same order.
            let mut rust = HeapBox::<PrimaryCoefficients>::new_zeroed().unwrap();
            let mut c = ffi::AVPrimaryCoefficients::default();
            let wp = reference.wp;
            assert_eq!(c_id(wp, rust.as_ref().get().into_raw()), c_id(wp, c));
            let target = c_fields(&reference.prim);
            for (i, &v) in target.iter().enumerate() {
                rust_set_part(&mut rust.as_mut(), i, v);
                *c_part_mut(&mut c, i) = v;
                let now = rust.as_ref().get().into_raw();
                assert_eq!(c_fields(&now), c_fields(&c), "{p:?} part {i}");
                assert_eq!(c_id(wp, now), c_id(wp, c), "{p:?} part {i}");
            }
            let id = c_id(wp, c);
            // Whole primaries through `set_r`, `set_g`, `set_b` and `set`,
            // with a red primary moved past C's 1/1000 tolerance and back.
            let r = reference.prim.r;
            let far = ffi::AVCIExy {
                x: ffi::AVRational {
                    num: r.x.num * 10 + r.x.den / 50,
                    den: r.x.den * 10,
                },
                y: r.y,
            };
            for (step, red) in [("far", far), ("back", r)] {
                rust.as_mut().set_r(CieXy::from_raw(red));
                c.r = red;
                let now = rust.as_ref().get().into_raw();
                assert_eq!(c_id(wp, now), c_id(wp, c), "{p:?} {step}");
            }
            rust.as_mut().set_g(CieXy::default());
            c.g = ffi::AVCIExy::default();
            assert_eq!(
                c_id(wp, rust.as_ref().get().into_raw()),
                c_id(wp, c),
                "{p:?} no green"
            );
            rust.as_mut().set_g(CieXy::from_raw(reference.prim.g));
            rust.as_mut().set_b(CieXy::from_raw(reference.prim.b));
            c.g = reference.prim.g;
            assert_eq!(c_id(wp, rust.as_ref().get().into_raw()), id, "{p:?}");
            rust.as_mut().set(PrimaryCoefficients::default());
            assert_eq!(
                c_id(wp, rust.as_ref().get().into_raw()),
                c_id(wp, ffi::AVPrimaryCoefficients::default())
            );
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn equality_is_c_comparison_on_every_coordinate() {
        let q = |num, den| ffi::AVRational { num, den };
        let mut entries: Vec<ffi::AVPrimaryCoefficients> =
            c_gamuts().into_iter().map(|(_, d)| d.prim).collect();
        // An unreduced twin of BT.709's and an all-`0/0` entry.
        let mut twin = entries[0];
        twin.r.x = q(twin.r.x.num * 3, twin.r.x.den * 3);
        twin.b.y = q(twin.b.y.num * -2, twin.b.y.den * -2);
        entries.extend([twin, ffi::AVPrimaryCoefficients::default()]);
        for a in &entries {
            for b in &entries {
                let pairs = [
                    (a.r.x, b.r.x),
                    (a.r.y, b.r.y),
                    (a.g.x, b.g.x),
                    (a.g.y, b.g.y),
                    (a.b.x, b.b.x),
                    (a.b.y, b.b.y),
                ];
                let c_equal = pairs.iter().all(|&(x, y)| {
                    // SAFETY: `av_cmp_q` takes both by value.
                    unsafe { ffi::av_cmp_q(x, y) == 0 }
                });
                assert_eq!(
                    PrimaryCoefficients::from_raw(*a) == PrimaryCoefficients::from_raw(*b),
                    c_equal,
                    "{:?} {:?}",
                    c_fields(a),
                    c_fields(b)
                );
            }
        }
        let rust_twin = PrimaryCoefficients::from_raw(twin);
        assert_eq!(rust_twin, PrimaryCoefficients::from_raw(entries[0]));
        assert_eq!(
            Rational::from_raw(twin.r.x),
            Rational::from_raw(entries[0].r.x)
        );
    }
}

/// The `ColorTransfer` getters against `av_csp_approximate_trc_gamma`,
/// `av_csp_approximate_eotf_gamma`, `av_csp_trc_func_from_id`,
/// `av_csp_trc_func_inv_from_id`, `av_csp_itu_eotf` and
/// `av_csp_itu_eotf_inv`: for every transfer value past both ranges and the
/// `unsigned` extremes, the gammas bit for bit, and every handle against the
/// function C returns, called on the same inputs in sequence.
mod color_transfer {
    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::pixfmt::ColorTransfer;

    type Trc = unsafe extern "C" fn(f64) -> f64;
    type Eotf = unsafe extern "C" fn(f64, f64, *mut f64);

    fn raws() -> impl Iterator<Item = u32> {
        (0..300).chain([
            511,
            512,
            1 << 16,
            i32::MAX as u32,
            1 << 31,
            u32::MAX - 1,
            u32::MAX,
        ])
    }

    fn c(raw: u32) -> ffi::AVColorTransferCharacteristic {
        ffi::AVColorTransferCharacteristic(raw)
    }

    fn t(raw: u32) -> ColorTransfer {
        ColorTransfer::from_raw(c(raw))
    }

    const INPUTS: [f64; 10] = [
        0.0,
        -0.0,
        1.0,
        0.5,
        0.018,
        -0.25,
        1.5,
        1e300,
        f64::NEG_INFINITY,
        f64::NAN,
    ];

    /// C's gamma result as the Rust one reports it.
    fn gamma(g: f64) -> Option<u64> {
        (g != 0.0).then_some(g.to_bits())
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn gammas_match_c_for_every_value() {
        let (mut trc, mut eotf) = (0, 0);
        for raw in raws() {
            // SAFETY: the gamma getters take the enum by value and read
            // their tables only for an index inside them.
            let (ct, ce) = (
                unsafe { ffi::av_csp_approximate_trc_gamma(c(raw)) },
                unsafe { ffi::av_csp_approximate_eotf_gamma(c(raw)) },
            );
            assert_eq!(
                t(raw).approximate_trc_gamma().map(f64::to_bits),
                gamma(ct),
                "{raw}"
            );
            assert_eq!(
                t(raw).approximate_eotf_gamma().map(f64::to_bits),
                gamma(ce),
                "{raw}"
            );
            trc += usize::from(ct != 0.0);
            eotf += usize::from(ce != 0.0);
        }
        // 11 plain characteristics and V-Log.
        assert_eq!((trc, eotf), (12, 12));
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn trc_handles_match_c_for_every_value() {
        let mut found = [0; 2];
        for raw in raws() {
            // SAFETY: the getters take the enum by value and return NULL or
            // a `csp.c` transfer function.
            let cs: [Option<Trc>; 2] = [unsafe { ffi::av_csp_trc_func_from_id(c(raw)) }, unsafe {
                ffi::av_csp_trc_func_inv_from_id(c(raw))
            }];
            let rs = [t(raw).trc_function(), t(raw).inverse_trc_function()];
            for (i, (r, cf)) in rs.into_iter().zip(cs).enumerate() {
                assert_eq!(r.is_some(), cf.is_some(), "{raw} #{i}");
                let (Some(r), Some(cf)) = (r, cf) else {
                    continue;
                };
                found[i] += 1;
                for x in INPUTS {
                    // SAFETY: a `csp.c` transfer function takes any `f64`.
                    let expect = unsafe { cf(x) };
                    assert_eq!(r.call(x).to_bits(), expect.to_bits(), "{raw} #{i} at {x}");
                }
            }
        }
        // 16 plain characteristics and V-Log, each with an inverse.
        assert_eq!(found, [17, 17]);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn eotf_handles_match_c_for_every_value() {
        let mut found = [0; 2];
        for raw in raws() {
            // SAFETY: the getters take the enum by value and return NULL or
            // a `csp.c` EOTF.
            let cs: [Option<Eotf>; 2] = [unsafe { ffi::av_csp_itu_eotf(c(raw)) }, unsafe {
                ffi::av_csp_itu_eotf_inv(c(raw))
            }];
            let rs = [t(raw).eotf(), t(raw).inverse_eotf()];
            for (i, (r, cf)) in rs.into_iter().zip(cs).enumerate() {
                assert_eq!(r.is_some(), cf.is_some(), "{raw} #{i}");
                let (Some(r), Some(cf)) = (r, cf) else {
                    continue;
                };
                found[i] += 1;
                // Each step feeds both sides' outputs back as inputs.
                let mut rust = [0.25, 0.5, 0.75];
                let mut cv = rust;
                for (k, (lw, lb)) in [(1000.0, 0.005), (100.0, 0.0), (48.0, 0.02), (0.0, 0.0)]
                    .into_iter()
                    .cycle()
                    .take(8)
                    .enumerate()
                {
                    r.call(lw, lb, &mut rust);
                    // SAFETY: a `csp.c` EOTF reads and writes `cv[0..3]`.
                    unsafe { cf(lw, lb, cv.as_mut_ptr()) };
                    assert_eq!(
                        rust.map(f64::to_bits),
                        cv.map(f64::to_bits),
                        "{raw} #{i} step {k}"
                    );
                }
            }
        }
        // 14 plain characteristics, no logarithmic or extended one.
        assert_eq!(found, [14, 14]);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn forward_and_inverse_compose_as_in_c() {
        for raw in raws() {
            let (Some(f), Some(g)) = (t(raw).trc_function(), t(raw).inverse_trc_function()) else {
                continue;
            };
            // SAFETY: as above.
            let (cf, cg) = (
                unsafe { ffi::av_csp_trc_func_from_id(c(raw)) }.unwrap(),
                unsafe { ffi::av_csp_trc_func_inv_from_id(c(raw)) }.unwrap(),
            );
            for x in INPUTS {
                // SAFETY: as the comment below states, for this part of the operation.
                let cf_2 = unsafe { cf(x) };
                // SAFETY: `csp.c` transfer functions take any `f64`.
                let expect = unsafe { cg(cf_2) };
                assert_eq!(
                    g.call(f.call(x)).to_bits(),
                    expect.to_bits(),
                    "{raw} at {x}"
                );
            }
        }
    }
}

mod luma_lookup {
    use ffmpeg_bindings::libavutil::libavutil::csp::LumaCoefficients;
    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::pixfmt::ColorSpace;

    /// Every value C has a table slot for, its neighbours, and the extremes,
    /// as C's `unsigned` enum takes them.
    fn values() -> impl Iterator<Item = u32> {
        (0..ffi::AVColorSpace::AVCOL_SPC_NB.0 + 3).chain([u32::MAX, u32::MAX - 1, 1 << 31])
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn luma_coefficients_are_cs_table_entries() {
        let mut found = 0;
        for raw in values() {
            // SAFETY: by value; C checks the value against its table.
            let c = unsafe { ffi::av_csp_luma_coeffs_from_avcsp(ffi::AVColorSpace(raw)) };
            let rust = ColorSpace::from_raw(ffi::AVColorSpace(raw)).luma_coefficients();
            assert_eq!(rust.is_some(), !c.is_null(), "{raw}");
            if let Some(r) = rust {
                // SAFETY: a non-NULL return is an entry of C's static table.
                let c = unsafe { c.read() };
                let [cr, cg, cb] = [c.cr, c.cg, c.cb].map(|q| (q.num, q.den));
                assert_eq!((r.cr().num(), r.cr().den()), cr, "{raw}");
                assert_eq!((r.cg().num(), r.cg().den()), cg, "{raw}");
                assert_eq!((r.cb().num(), r.cb().den()), cb, "{raw}");
                assert_eq!(r.get(), LumaCoefficients::from_raw(c), "{raw}");
                found += 1;
            }
        }
        assert!(found >= 9, "{found}");
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn repeated_lookups_return_the_same_entry() {
        for raw in values() {
            let cs = ColorSpace::from_raw(ffi::AVColorSpace(raw));
            let (a, b) = (cs.luma_coefficients(), cs.luma_coefficients());
            assert_eq!(a.map(|r| r.get()), b.map(|r| r.get()), "{raw}");
        }
    }
}

/// [`ColorPrimariesDescriptor`] against C's `AVColorPrimariesDesc`: the safe
/// getters against every gamut C describes (`av_csp_primaries_desc_from_id`),
/// read in place in C's static tables; descriptions rebuilt coordinate by
/// coordinate and field by field through the safe setters, in
/// `av_malloc`-owned storage that C reads in place, against the same writes
/// to an independent C struct, as `av_csp_primaries_id_from_desc` identifies
/// both after each step; and equality against C's `av_cmp_q` on all eight
/// coordinates.
///
/// [`ColorPrimariesDescriptor`]: ffmpeg_bindings::libavutil::libavutil::csp::ColorPrimariesDescriptor
mod color_primaries_descriptor {
    use core::mem::{align_of, offset_of, size_of};
    use core::ptr::{self};

    use ffmpeg_bindings::libavutil::libavutil::csp::{
        CieXy, CieXyMut, ColorPrimariesDescriptor, ColorPrimariesDescriptorMut,
        ColorPrimariesDescriptorRef, PrimaryCoefficients,
    };
    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::mem::HeapBox;
    use ffmpeg_bindings::libavutil::libavutil::pixfmt::ColorPrimaries;
    use ffmpeg_bindings::libavutil::libavutil::rational::Rational;

    /// Every primaries value C may describe, past the end of both ranges
    /// included.
    fn primaries() -> impl Iterator<Item = ffi::AVColorPrimaries> {
        let nb = ffi::AVColorPrimaries::AVCOL_PRI_NB.0;
        let ext = ffi::AVColorPrimaries::AVCOL_PRI_EXT_BASE.0;
        let ext_nb = ffi::AVColorPrimaries::AVCOL_PRI_EXT_NB.0;
        (0..=nb).chain(ext..=ext_nb).map(ffi::AVColorPrimaries)
    }

    /// C's description for `p`, if it has one, read in place in C's static
    /// storage through the safe lookup.
    fn c_entry(p: ffi::AVColorPrimaries) -> Option<ColorPrimariesDescriptorRef<'static>> {
        ColorPrimaries::from_raw(p).descriptor()
    }

    /// Copies of every gamut description C holds, with its primaries value.
    fn c_gamuts() -> Vec<(ffi::AVColorPrimaries, ffi::AVColorPrimariesDesc)> {
        primaries()
            .filter_map(|p| {
                // SAFETY: as in `c_entry`.
                let d = unsafe { ffi::av_csp_primaries_desc_from_id(p) };
                // SAFETY: a non-NULL result is a live, initialised
                // description; the copy is plain data.
                (!d.is_null()).then(|| (p, unsafe { d.read() }))
            })
            .collect()
    }

    /// The four chromaticities of a C description, white point first.
    fn c_points(d: &ffi::AVColorPrimariesDesc) -> [ffi::AVCIExy; 4] {
        [d.wp, d.prim.r, d.prim.g, d.prim.b]
    }

    fn c_fields(d: &ffi::AVColorPrimariesDesc) -> [i32; 16] {
        let mut out = [0; 16];
        for (i, xy) in c_points(d).iter().enumerate() {
            out[4 * i..4 * i + 4].copy_from_slice(&[xy.x.num, xy.x.den, xy.y.num, xy.y.den]);
        }
        out
    }

    fn rust_fields(d: ColorPrimariesDescriptorRef<'_>) -> [i32; 16] {
        let p = d.primaries();
        let mut out = [0; 16];
        for (i, xy) in [d.white_point(), p.r(), p.g(), p.b()]
            .into_iter()
            .enumerate()
        {
            let (x, y) = (xy.x(), xy.y());
            out[4 * i..4 * i + 4].copy_from_slice(&[x.num(), x.den(), y.num(), y.den()]);
        }
        out
    }

    /// Part `i` (in `c_fields` order) of C's struct.
    fn c_part_mut(d: &mut ffi::AVColorPrimariesDesc, i: usize) -> &mut i32 {
        let xy = match i / 4 {
            0 => &mut d.wp,
            1 => &mut d.prim.r,
            2 => &mut d.prim.g,
            _ => &mut d.prim.b,
        };
        match i % 4 {
            0 => &mut xy.x.num,
            1 => &mut xy.x.den,
            2 => &mut xy.y.num,
            _ => &mut xy.y.den,
        }
    }

    fn rust_set_xy_part(xy: &mut CieXyMut<'_>, i: usize, v: i32) {
        match i % 4 {
            0 => xy.x_mut().set_num(v),
            1 => xy.x_mut().set_den(v),
            2 => xy.y_mut().set_num(v),
            _ => xy.y_mut().set_den(v),
        }
    }

    /// The same part through the safe field, primary and coordinate handles.
    fn rust_set_part(m: &mut ColorPrimariesDescriptorMut<'_>, i: usize, v: i32) {
        if i < 4 {
            return rust_set_xy_part(&mut m.white_point_mut(), i, v);
        }
        let mut p = m.primaries_mut();
        let mut xy = match i / 4 {
            1 => p.r_mut(),
            2 => p.g_mut(),
            _ => p.b_mut(),
        };
        rust_set_xy_part(&mut xy, i, v);
    }

    /// C's identification of the description `d`.
    fn c_id_at(d: &ffi::AVColorPrimariesDesc) -> ffi::AVColorPrimaries {
        // SAFETY: `d` is a live, initialised description, which C only reads
        // during the call.
        unsafe { ffi::av_csp_primaries_id_from_desc(d) }
    }

    /// C's identification of the Rust-owned description, read by C in place.
    fn rust_id(d: &HeapBox<ColorPrimariesDescriptor>) -> ffi::AVColorPrimaries {
        // SAFETY: `as_c_ptr` addresses the live, initialised box, which the
        // shared borrow of `d` keeps unwritten while C reads it.
        unsafe { ffi::av_csp_primaries_id_from_desc(d.as_c_ptr()) }
    }

    #[test]
    fn layout_matches_c() {
        assert_eq!(
            size_of::<ColorPrimariesDescriptor>(),
            size_of::<ffi::AVColorPrimariesDesc>()
        );
        assert_eq!(
            align_of::<ColorPrimariesDescriptor>(),
            align_of::<ffi::AVColorPrimariesDesc>()
        );
        assert_eq!(offset_of!(ffi::AVColorPrimariesDesc, wp), 0);
        assert_eq!(
            offset_of!(ffi::AVColorPrimariesDesc, prim),
            size_of::<ffi::AVCIExy>()
        );
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn getters_read_c_s_tables_for_every_gamut() {
        let gamuts = c_gamuts();
        assert!(gamuts.len() >= 11, "C describes {} gamuts", gamuts.len());
        for (p, d) in &gamuts {
            let entry = c_entry(*p).unwrap();
            assert_eq!(rust_fields(entry), c_fields(d), "{p:?}");
            assert_eq!(rust_fields(entry.get().as_ref()), c_fields(d));
            let copy = ColorPrimariesDescriptor::from_raw(*d);
            assert_eq!(rust_fields(copy.as_ref()), c_fields(d));
            assert_eq!(c_fields(&copy.into_raw()), c_fields(d));
            let built = ColorPrimariesDescriptor::new(
                CieXy::from_raw(d.wp),
                PrimaryCoefficients::from_raw(d.prim),
            );
            assert_eq!(rust_fields(built.as_ref()), c_fields(d));
            let rebuilt =
                ColorPrimariesDescriptor::new(entry.white_point().get(), entry.primaries().get());
            assert_eq!(c_fields(&rebuilt.into_raw()), c_fields(d));
            // C hands out only described gamuts, red x numerator set.
            assert_ne!(entry.primaries().r().x().num(), 0);
            assert_eq!(c_id_at(&rebuilt.into_raw()), c_id_at(d));
        }
        let bt709 = c_entry(ffi::AVColorPrimaries::AVCOL_PRI_BT709).unwrap();
        assert!((bt709.white_point().x().get().to_f64() - 0.3127).abs() < 1e-9);
        let v_gamut = c_entry(ffi::AVColorPrimaries::AVCOL_PRI_V_GAMUT).unwrap();
        assert!(v_gamut.primaries().b().y().num() < 0);
        assert!(c_entry(ffi::AVColorPrimaries::AVCOL_PRI_UNSPECIFIED).is_none());
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn descriptions_rebuilt_through_the_setters_identify_as_c_after_each_step() {
        for (p, reference) in c_gamuts() {
            // Rust's description in an `av_malloc` block, C's in its own
            // struct: independently owned, written in the same order.
            let mut rust = HeapBox::<ColorPrimariesDescriptor>::new_zeroed().unwrap();
            let mut c = ffi::AVColorPrimariesDesc::default();
            assert_eq!(rust_id(&rust), c_id_at(&c));
            for (i, &v) in c_fields(&reference).iter().enumerate() {
                rust_set_part(&mut rust.as_mut(), i, v);
                *c_part_mut(&mut c, i) = v;
                assert_eq!(rust_fields(rust.as_ref()), c_fields(&c), "{p:?} part {i}");
                assert_eq!(rust_id(&rust), c_id_at(&c), "{p:?} part {i}");
            }
            let id = c_id_at(&c);
            // Whole fields through `set_white_point`, `set_primaries` and
            // `set`, with the white point moved past C's 1/1000 tolerance
            // and back.
            let wp = reference.wp;
            let far = ffi::AVCIExy {
                x: ffi::AVRational {
                    // x + 2/1000, exact for every denominator (D65's
                    // 10000, equal energy's 3).
                    num: wp.x.num * 1000 + wp.x.den * 2,
                    den: wp.x.den * 1000,
                },
                y: wp.y,
            };
            for (step, white) in [("far", far), ("back", wp)] {
                rust.as_mut().set_white_point(CieXy::from_raw(white));
                c.wp = white;
                assert_eq!(rust_fields(rust.as_ref()), c_fields(&c), "{p:?} {step}");
                assert_eq!(rust_id(&rust), c_id_at(&c), "{p:?} {step}");
                if step == "far" && p.0 < ffi::AVColorPrimaries::AVCOL_PRI_NB.0 {
                    assert_ne!(c_id_at(&c), id, "{p:?}: the nudge leaves C's tolerance");
                }
            }
            rust.as_mut().set_primaries(PrimaryCoefficients::default());
            c.prim = ffi::AVPrimaryCoefficients::default();
            assert_eq!(rust_id(&rust), c_id_at(&c), "{p:?} no primaries");
            rust.as_mut()
                .set_primaries(PrimaryCoefficients::from_raw(reference.prim));
            c.prim = reference.prim;
            assert_eq!(rust_id(&rust), id, "{p:?}");
            assert_eq!(c_id_at(&c), id, "{p:?}");
            // A copy written into another owner through `set`.
            let mut other = HeapBox::<ColorPrimariesDescriptor>::new_zeroed().unwrap();
            other.as_mut().set(rust.as_ref().get());
            assert_eq!(rust_id(&other), id, "{p:?} copy");
            rust.as_mut().set(ColorPrimariesDescriptor::default());
            assert_eq!(
                rust_id(&rust),
                c_id_at(&ffi::AVColorPrimariesDesc::default())
            );
            assert_eq!(rust_id(&other), id, "{p:?} copy after source reset");
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn equality_is_c_comparison_on_every_coordinate() {
        let q = |num, den| ffi::AVRational { num, den };
        let mut entries: Vec<ffi::AVColorPrimariesDesc> =
            c_gamuts().into_iter().map(|(_, d)| d).collect();
        // An unreduced twin of BT.709's, a twin differing in the white point
        // only, and an all-`0/0` entry.
        let mut twin = entries[0];
        twin.wp.x = q(twin.wp.x.num * 3, twin.wp.x.den * 3);
        twin.prim.b.y = q(twin.prim.b.y.num * -2, twin.prim.b.y.den * -2);
        let mut other_wp = entries[0];
        other_wp.wp.y = q(other_wp.wp.y.num + 1, other_wp.wp.y.den);
        entries.extend([twin, other_wp, ffi::AVColorPrimariesDesc::default()]);
        for a in &entries {
            for b in &entries {
                let c_equal = c_points(a).iter().zip(c_points(b)).all(|(x, y)| {
                    // SAFETY: `av_cmp_q` takes both by value.
                    (unsafe { ffi::av_cmp_q(x.x, y.x) } == 0)
                        && unsafe { ffi::av_cmp_q(x.y, y.y) } == 0
                });
                assert_eq!(
                    ColorPrimariesDescriptor::from_raw(*a)
                        == ColorPrimariesDescriptor::from_raw(*b),
                    c_equal,
                    "{:?} {:?}",
                    c_fields(a),
                    c_fields(b)
                );
            }
        }
        let rust_twin = ColorPrimariesDescriptor::from_raw(twin);
        assert_eq!(rust_twin, ColorPrimariesDescriptor::from_raw(entries[0]));
        assert_ne!(
            ColorPrimariesDescriptor::from_raw(other_wp),
            ColorPrimariesDescriptor::from_raw(entries[0])
        );
        assert_eq!(
            Rational::from_raw(twin.wp.x),
            Rational::from_raw(entries[0].wp.x)
        );
    }

    /// `AVDOVIDmLevel10`, the C struct that embeds a description by value,
    /// mirrored with the wrapper in the field's place: Rust-owned storage
    /// C reads through a pointer to the parent.
    #[repr(C)]
    struct DmLevel10 {
        target_display_index: u8,
        target_max_pq: u16,
        target_min_pq: u16,
        target_primary_index: u8,
        target_display_primaries: ColorPrimariesDescriptor,
    }

    /// C's identification of the description embedded in the Rust-owned
    /// parent, read in place through the parent's C layout.
    fn embedded_id(parent: &DmLevel10) -> ffi::AVColorPrimaries {
        let c = ptr::from_ref(parent).cast::<ffi::AVDOVIDmLevel10>();
        // SAFETY: `c` is to a live object, as the block below relies on, so the
        // place of its `target_display_primaries` field is in bounds; nothing is accessed here.
        let target_display_primaries_ptr = unsafe { &raw const (*c).target_display_primaries };
        // SAFETY: `DmLevel10` mirrors `AVDOVIDmLevel10` field for field
        // (sizes and the field offset asserted by the test), and the shared
        // borrow keeps the parent live and unwritten while C reads the
        // projected field.
        unsafe { ffi::av_csp_primaries_id_from_desc(target_display_primaries_ptr) }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn embedded_in_a_c_parent_is_read_by_c_in_place_after_each_step() {
        assert_eq!(size_of::<DmLevel10>(), size_of::<ffi::AVDOVIDmLevel10>());
        assert_eq!(align_of::<DmLevel10>(), align_of::<ffi::AVDOVIDmLevel10>());
        assert_eq!(
            offset_of!(DmLevel10, target_display_primaries),
            offset_of!(ffi::AVDOVIDmLevel10, target_display_primaries)
        );
        for (p, reference) in c_gamuts() {
            let mut rust = DmLevel10 {
                target_display_index: 1,
                target_max_pq: 2,
                target_min_pq: 3,
                target_primary_index: 4,
                target_display_primaries: ColorPrimariesDescriptor::default(),
            };
            let mut c = ffi::AVDOVIDmLevel10 {
                target_display_index: 1,
                target_max_pq: 2,
                target_min_pq: 3,
                target_primary_index: 4,
                ..Default::default()
            };
            assert_eq!(embedded_id(&rust), c_id_at(&c.target_display_primaries));
            for (i, &v) in c_fields(&reference).iter().enumerate() {
                rust_set_part(&mut rust.target_display_primaries.as_mut(), i, v);
                *c_part_mut(&mut c.target_display_primaries, i) = v;
                assert_eq!(
                    rust_fields(rust.target_display_primaries.as_ref()),
                    c_fields(&c.target_display_primaries),
                    "{p:?} part {i}"
                );
                assert_eq!(
                    embedded_id(&rust),
                    c_id_at(&c.target_display_primaries),
                    "{p:?} part {i}"
                );
            }
            // C's own identification of the reference: the first table entry
            // within tolerance, which for SMPTE 240M is SMPTE 170M's twin.
            let id = c_id_at(&reference);
            assert_eq!(embedded_id(&rust), id, "{p:?}");
            // The neighbouring scalars are untouched by the field's handles.
            assert_eq!(
                (
                    rust.target_display_index,
                    rust.target_max_pq,
                    rust.target_min_pq,
                    rust.target_primary_index
                ),
                (1, 2, 3, 4)
            );
            // A whole-value swap through `set`, then C's own copy of the
            // parent carries the same description.
            rust.target_display_primaries
                .as_mut()
                .set(ColorPrimariesDescriptor::default());
            c.target_display_primaries = ffi::AVColorPrimariesDesc::default();
            assert_eq!(embedded_id(&rust), c_id_at(&c.target_display_primaries));
            rust.target_display_primaries
                .as_mut()
                .set(ColorPrimariesDescriptor::from_raw(reference));
            c.target_display_primaries = reference;
            let copy = c;
            assert_eq!(embedded_id(&rust), c_id_at(&copy.target_display_primaries));
            assert_eq!(c_id_at(&copy.target_display_primaries), id, "{p:?}");
        }
    }
}

/// `ColorPrimaries::descriptor` and `ColorPrimariesDescriptorRef::identify`
/// against `av_csp_primaries_desc_from_id` and
/// `av_csp_primaries_id_from_desc`: the lookup for every value past both
/// ranges and the `unsigned` extremes, the identification of every
/// description C holds, read in place and from copies, around C's 1/1000
/// tolerance, and along random sequences of single-coordinate edits to
/// independently owned descriptions, compared after every edit. C's
/// `AVCOL_PRI_UNSPECIFIED` is the wrapper's `None`.
///
/// Descriptions with a `0/0` coordinate or two zero denominators are where
/// the wrapper corrects C (which reports BT.709 for them, see
/// `csp::unit_tests`): the sequences check the wrapper's `None` there
/// instead.
mod primaries_lookup {
    use ffmpeg_bindings::libavutil::libavutil::csp::{
        CieXy, ColorPrimariesDescriptor, ColorPrimariesDescriptorRef,
    };
    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::mem::HeapBox;
    use ffmpeg_bindings::libavutil::libavutil::pixfmt::ColorPrimaries;

    /// Every value C has a table slot for, its neighbours, and the extremes,
    /// as C's `unsigned` enum takes them.
    fn values() -> impl Iterator<Item = u32> {
        let nb = ffi::AVColorPrimaries::AVCOL_PRI_NB.0;
        let ext = ffi::AVColorPrimaries::AVCOL_PRI_EXT_BASE.0;
        let ext_nb = ffi::AVColorPrimaries::AVCOL_PRI_EXT_NB.0;
        (0..nb + 3).chain(ext - 2..ext_nb + 3).chain([
            u32::MAX,
            u32::MAX - 1,
            1 << 31,
            (1 << 31) - 1,
        ])
    }

    /// C's description for `raw`, copied out of its table.
    fn c_desc(raw: u32) -> Option<ffi::AVColorPrimariesDesc> {
        // SAFETY: by value; C checks the value against both tables and
        // returns NULL or a pointer into one of them.
        let d = unsafe { ffi::av_csp_primaries_desc_from_id(ffi::AVColorPrimaries(raw)) };
        // SAFETY: a non-NULL result is a live, initialised table entry; the
        // copy is plain data.
        (!d.is_null()).then(|| unsafe { d.read() })
    }

    /// C's identification of `d`, as the wrapper reports it.
    fn c_identify(d: &ffi::AVColorPrimariesDesc) -> Option<ColorPrimaries> {
        // SAFETY: `d` is a live, initialised description C only reads.
        let id = unsafe { ffi::av_csp_primaries_id_from_desc(d) };
        (id != ffi::AVColorPrimaries::AVCOL_PRI_UNSPECIFIED).then(|| ColorPrimaries::from_raw(id))
    }

    fn fields(d: &ffi::AVColorPrimariesDesc) -> [i32; 16] {
        let mut out = [0; 16];
        for (i, xy) in [d.wp, d.prim.r, d.prim.g, d.prim.b].iter().enumerate() {
            out[4 * i..4 * i + 4].copy_from_slice(&[xy.x.num, xy.x.den, xy.y.num, xy.y.den]);
        }
        out
    }

    fn rust_fields(d: ColorPrimariesDescriptorRef<'_>) -> [i32; 16] {
        fields(&d.get().into_raw())
    }

    fn desc(f: [i32; 16]) -> ffi::AVColorPrimariesDesc {
        let q = |i: usize| ffi::AVRational {
            num: f[i],
            den: f[i + 1],
        };
        let xy = |i: usize| ffi::AVCIExy {
            x: q(i),
            y: q(i + 2),
        };
        ffi::AVColorPrimariesDesc {
            wp: xy(0),
            prim: ffi::AVPrimaryCoefficients {
                r: xy(4),
                g: xy(8),
                b: xy(12),
            },
        }
    }

    /// Where the wrapper deliberately differs from C.
    fn corrected(f: &[i32; 16]) -> bool {
        let dens = (0..8).filter(|i| f[2 * i + 1] == 0);
        dens.clone().count() >= 2 || dens.into_iter().any(|i| f[2 * i] == 0)
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn descriptors_are_cs_table_entries() {
        let mut found = 0;
        for raw in values() {
            let rust = ColorPrimaries::from_raw(ffi::AVColorPrimaries(raw)).descriptor();
            let c = c_desc(raw);
            assert_eq!(rust.is_some(), c.is_some(), "{raw}");
            if let (Some(r), Some(c)) = (rust, c) {
                assert_eq!(rust_fields(r), fields(&c), "{raw}");
                assert_eq!(r.get(), ColorPrimariesDescriptor::from_raw(c), "{raw}");
                found += 1;
            }
        }
        assert!(found >= 12, "{found}");
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn repeated_lookups_return_the_same_entry() {
        for raw in values() {
            let p = ColorPrimaries::from_raw(ffi::AVColorPrimaries(raw));
            let (a, b) = (p.descriptor(), p.descriptor());
            assert_eq!(a.map(rust_fields), b.map(rust_fields), "{raw}");
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn every_description_identifies_as_in_c() {
        let mut ids = Vec::new();
        for raw in values() {
            let Some(c) = c_desc(raw) else { continue };
            let rust = ColorPrimaries::from_raw(ffi::AVColorPrimaries(raw))
                .descriptor()
                .unwrap();
            let expect = c_identify(&c);
            // In place in C's table, from a copy, and from `av_malloc`
            // storage.
            assert_eq!(rust.identify(), expect, "{raw}");
            let copy = rust.get();
            assert_eq!(copy.as_ref().identify(), expect, "{raw}");
            let mut boxed = HeapBox::<ColorPrimariesDescriptor>::new_zeroed().unwrap();
            boxed.as_mut().set(copy);
            assert_eq!(boxed.as_ref().identify(), expect, "{raw}");
            ids.push((raw, expect));
        }
        // Each H.273 description names itself, but SMPTE 240M, whose
        // identical entry SMPTE 170M comes first, and V-Gamut, which C does
        // not search.
        for (raw, id) in ids {
            let p = ColorPrimaries::from_raw(ffi::AVColorPrimaries(raw));
            match p {
                ColorPrimaries::SMPTE240M => assert_eq!(id, Some(ColorPrimaries::SMPTE170M)),
                ColorPrimaries::V_GAMUT => assert_eq!(id, None),
                _ => assert_eq!(id, Some(p)),
            }
        }
    }

    /// `x` moved by `k` millionths, in lowest terms.
    fn nudge(num: i32, den: i32, k: i64) -> (i32, i32) {
        let (n, d) = (
            i64::from(num) * 1_000_000 + i64::from(den) * k,
            i64::from(den) * 1_000_000,
        );
        let (mut a, mut b) = (n.abs(), d.abs());
        while b != 0 {
            (a, b) = (b, a % b);
        }
        ((n / a).try_into().unwrap(), (d / a).try_into().unwrap())
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn tolerance_boundary_matches_c() {
        // [inside, at or past 1/1000][elsewhere, the unmoved entry's id]
        let mut outcomes = [[0; 2]; 2];
        for raw in values() {
            let Some(c) = c_desc(raw) else { continue };
            let base = fields(&c);
            // One coordinate, or two sharing the distance; C's tolerance is
            // a strict 1/1000 on the sum.
            for (i, j) in [
                (0, None),
                (4, None),
                (14, None),
                (2, Some(8)),
                (6, Some(12)),
            ] {
                for total in [0, 1, 999, 1000, 1001, 2000] {
                    for sign in [1, -1] {
                        let mut f = base;
                        let (ki, kj) = match j {
                            None => (total, 0),
                            Some(_) => (total / 2, total - total / 2),
                        };
                        (f[i], f[i + 1]) = nudge(f[i], f[i + 1], sign * ki);
                        if let Some(j) = j {
                            (f[j], f[j + 1]) = nudge(f[j], f[j + 1], -sign * kj);
                        }
                        let d = desc(f);
                        let rust = ColorPrimariesDescriptor::from_raw(d).as_ref().identify();
                        assert_eq!(rust, c_identify(&d), "{raw} {i} {j:?} {total} {sign}");
                        let own = rust == c_identify(&c);
                        outcomes[usize::from(total >= 1000)][usize::from(own)] += 1;
                    }
                }
            }
        }
        // Inside the tolerance every description keeps its entry's
        // identification; at and past it, the H.273 ones lose it.
        assert_eq!(outcomes[0][0], 0, "{outcomes:?}");
        assert!(outcomes[1][0] > 0 && outcomes[1][1] > 0, "{outcomes:?}");
    }

    struct XorShift(u64);

    impl XorShift {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
    }

    /// Whether `num/den` (`den > 0`), standing in for the entry's
    /// `ref_num/100000` coordinate, keeps the description's exact distance
    /// to the entry below 1/1000: the verdict of exact rational arithmetic,
    /// which C's `int` reduction (`av_reduce`, `INT_MAX`) only approximates
    /// once `den * 100000` exceeds `INT_MAX`.
    fn exactly_inside(num: i32, den: i32, ref_num: i32) -> bool {
        let diff = (i128::from(num) * 100_000 - i128::from(ref_num) * i128::from(den)).abs();
        diff * 1000 < i128::from(den) * 100_000
    }

    /// BT.709's white point `x` moved to within `1e-13` of 1/1000 away, with
    /// denominators too large for C's reduction to keep the difference
    /// exact: exact arithmetic says inside, C's `av_reduce` says 1/1000.
    const REDUCED_TO_THE_TOLERANCE: [(i32, i32); 3] = [
        (311_418_082, 999_095_547),
        (322_460_225, 1_034_521_094),
        (528_928_917, 1_696_916_641),
    ];

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn reduced_sums_follow_c_not_exact_arithmetic() {
        let bt709 = c_desc(ffi::AVColorPrimaries::AVCOL_PRI_BT709.0).unwrap();
        let base = fields(&bt709);
        // Each part of the description, with the one coordinate at a random
        // large denominator placed one part short of, at, or past 1/1000
        // from the entry, plus the known reduced-to-the-tolerance ones.
        let mut rng = XorShift(0x2545_F491_4F6C_DD1D);
        let mut cases: Vec<(usize, i32, i32)> = REDUCED_TO_THE_TOLERANCE
            .iter()
            .map(|&(num, den)| (0, num, den))
            .collect();
        for round in 0..3000 {
            let i = 2 * (round % 8);
            let den = (rng.next() % (i64::from(i32::MAX) - 100_001) as u64) as i64 + 100_001;
            let sign = if rng.next() & 1 == 0 { 1 } else { -1 };
            // (ref + sign/1000) * den, rounded down, then one part around.
            let target = i64::from(base[i]) * den * 10 + sign * den * 1000;
            let num = target.div_euclid(1_000_000) + (rng.next() % 3) as i64 - 1;
            if let Ok(num) = i32::try_from(num) {
                cases.push((i, num, den as i32));
            }
        }
        let (mut compared, mut reduced_away) = (0, 0);
        for (i, num, den) in cases {
            let mut f = base;
            (f[i], f[i + 1]) = (num, den);
            let c = desc(f);
            let rust = ColorPrimariesDescriptor::from_raw(c).as_ref().identify();
            let expect = c_identify(&c);
            assert_eq!(rust, expect, "{i} {num}/{den}");
            compared += 1;
            if exactly_inside(num, den, base[i]) && expect.is_none() {
                reduced_away += 1;
            }
        }
        assert!(compared > 2000, "{compared}");
        // The wrapper follows C's reduction, not the exact verdict.
        assert!(reduced_away >= 3, "{reduced_away}");
    }

    const EDGES: [i32; 16] = [
        0,
        1,
        -1,
        2,
        3,
        i32::MAX,
        i32::MIN,
        i32::MAX - 1,
        i32::MIN + 1,
        100_000,
        -100_000,
        31_270,
        1 << 30,
        -(1 << 30),
        46_341,
        99_999,
    ];

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn random_edits_identify_as_in_c_after_each_step() {
        let entries: Vec<[i32; 16]> = values().filter_map(c_desc).map(|d| fields(&d)).collect();
        let mut rng = XorShift(0x9E37_79B9_7F4A_7C15);
        let (mut compared, mut corrected_steps) = (0, 0);
        for round in 0..400 {
            let start = entries[round % entries.len()];
            // The Rust description in an `av_malloc` block, C's in its own
            // struct, edited in the same order.
            let mut rust = HeapBox::<ColorPrimariesDescriptor>::new_zeroed().unwrap();
            rust.as_mut()
                .set(ColorPrimariesDescriptor::from_raw(desc(start)));
            let mut c = desc(start);
            for step in 0..12 {
                let i = (rng.next() % 16) as usize;
                let r = rng.next();
                let mut f = fields(&c);
                f[i] = match r % 4 {
                    0 => EDGES[(r >> 8) as usize % EDGES.len()],
                    1 => (r >> 16) as i32,
                    2 => f[i].wrapping_add((r >> 16) as i32 % 7 - 3),
                    _ => f[i].checked_neg().unwrap_or(f[i]),
                };
                c = desc(f);
                let xy = CieXy::from_raw(match i / 4 {
                    0 => c.wp,
                    1 => c.prim.r,
                    2 => c.prim.g,
                    _ => c.prim.b,
                });
                let mut m = rust.as_mut();
                match i / 4 {
                    0 => m.set_white_point(xy),
                    1 => m.primaries_mut().set_r(xy),
                    2 => m.primaries_mut().set_g(xy),
                    _ => m.primaries_mut().set_b(xy),
                }
                assert_eq!(rust_fields(rust.as_ref()), f, "{round} {step}");
                if corrected(&f) {
                    assert_eq!(rust.as_ref().identify(), None, "{round} {step} {f:?}");
                    corrected_steps += 1;
                } else {
                    assert_eq!(
                        rust.as_ref().identify(),
                        c_identify(&c),
                        "{round} {step} {f:?}"
                    );
                    compared += 1;
                }
            }
        }
        assert!(
            compared > 4000 && corrected_steps > 0,
            "{compared} {corrected_steps}"
        );
    }
}
