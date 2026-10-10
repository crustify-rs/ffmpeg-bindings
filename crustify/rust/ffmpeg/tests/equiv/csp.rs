//! Equivalence of `libavutil/csp.h`'s safe API with the C library.

/// [`CieXy`] against C's `AVCIExy`: the safe getters against C's fields for
/// the white point and primaries of every gamut C describes
/// (`av_csp_primaries_desc_from_id`), gamuts rebuilt coordinate by coordinate
/// through the safe setters, in `av_malloc`-owned storage, against the same
/// writes to an independent C struct, as `av_csp_primaries_id_from_desc`
/// identifies them after each step, and equality against C's `av_cmp_q` on
/// both coordinates.
///
/// [`CieXy`]: ffmpeg::libavutil::csp::CieXy
mod cie_xy {
    use core::mem::{align_of, offset_of, size_of};

    use ffmpeg::libavutil::csp::{CieXy, CieXyMut, CieXyRef};
    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::mem::{HeapVec, alloc_cell_array_zeroed};
    use ffmpeg::libavutil::rational::Rational;

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
/// [`LumaCoefficients`]: ffmpeg::libavutil::csp::LumaCoefficients
mod luma_coefficients {
    use core::ptr::NonNull;

    use ffibox::CSlice;
    use ffmpeg::libavutil::csp::{LumaCoefficients, LumaCoefficientsRef};
    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::mem::HeapBox;
    use ffmpeg::libavutil::rational::Rational;

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
    fn equality_matches_c_s_comparison_of_every_coefficient() {
        let mut entries: Vec<ffi::AVLumaCoefficients> =
            spaces().filter_map(c_entry).map(|e| e.get().into_raw()).collect();
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
                let c_eq = unsafe {
                    ffi::av_cmp_q(a.cr, b.cr) == 0
                        && ffi::av_cmp_q(a.cg, b.cg) == 0
                        && ffi::av_cmp_q(a.cb, b.cb) == 0
                };
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
                assert_eq!(rust_fields(rust_side.as_ref()), c_fields(&c_side), "{csp:?} step {n}");
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
/// [`PrimaryCoefficients`]: ffmpeg::libavutil::csp::PrimaryCoefficients
mod primary_coefficients {
    use core::mem::{align_of, offset_of, size_of};
    use core::ptr::{NonNull, addr_of};

    use ffibox::CSlice;
    use ffmpeg::libavutil::csp::{
        CieXy, PrimaryCoefficients, PrimaryCoefficientsMut, PrimaryCoefficientsRef,
    };
    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::mem::HeapBox;
    use ffmpeg::libavutil::rational::Rational;

    /// Every primaries value C may describe, past the end of both ranges
    /// included.
    fn primaries() -> impl Iterator<Item = ffi::AVColorPrimaries> {
        let nb = ffi::AVColorPrimaries::AVCOL_PRI_NB.0;
        let ext = ffi::AVColorPrimaries::AVCOL_PRI_EXT_BASE.0;
        let ext_nb = ffi::AVColorPrimaries::AVCOL_PRI_EXT_NB.0;
        (0..=nb).chain(ext..=ext_nb).map(ffi::AVColorPrimaries)
    }

    /// The primaries of C's description for `p`, if it has one, as a safe
    /// handle over C's static storage: the test's one adoption of C-owned
    /// `AVPrimaryCoefficients`.
    fn c_entry(p: ffi::AVColorPrimaries) -> Option<PrimaryCoefficientsRef<'static>> {
        // SAFETY: takes any value and returns NULL or a pointer into C's
        // static, never-written table.
        let d = NonNull::new(unsafe { ffi::av_csp_primaries_desc_from_id(p) }.cast_mut())?;
        // SAFETY: `d` is a live description; the projection forms no
        // reference.
        let prim = unsafe { addr_of!((*d.as_ptr()).prim) }.cast_mut();
        let prim = NonNull::new(prim).unwrap().cast::<PrimaryCoefficients>();
        // SAFETY: `prim` addresses one live, initialised, never-written
        // `AVPrimaryCoefficients` of a static table;
        // `PrimaryCoefficients` is transparent over it.
        let one: CSlice<'static, _> = unsafe { CSlice::from_raw_parts(prim, 1) };
        one.get(0)
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
        unsafe { ffi::av_csp_primaries_id_from_desc(&d) }
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
    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::pixfmt::ColorTransfer;

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
    fn gammas_match_c_for_every_value() {
        let (mut trc, mut eotf) = (0, 0);
        for raw in raws() {
            // SAFETY: the gamma getters take the enum by value and read
            // their tables only for an index inside them.
            let (ct, ce) = unsafe {
                (
                    ffi::av_csp_approximate_trc_gamma(c(raw)),
                    ffi::av_csp_approximate_eotf_gamma(c(raw)),
                )
            };
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
    fn trc_handles_match_c_for_every_value() {
        let mut found = [0; 2];
        for raw in raws() {
            // SAFETY: the getters take the enum by value and return NULL or
            // a `csp.c` transfer function.
            let cs: [Option<Trc>; 2] = unsafe {
                [
                    ffi::av_csp_trc_func_from_id(c(raw)),
                    ffi::av_csp_trc_func_inv_from_id(c(raw)),
                ]
            };
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
    fn eotf_handles_match_c_for_every_value() {
        let mut found = [0; 2];
        for raw in raws() {
            // SAFETY: the getters take the enum by value and return NULL or
            // a `csp.c` EOTF.
            let cs: [Option<Eotf>; 2] = unsafe {
                [
                    ffi::av_csp_itu_eotf(c(raw)),
                    ffi::av_csp_itu_eotf_inv(c(raw)),
                ]
            };
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
    fn forward_and_inverse_compose_as_in_c() {
        for raw in raws() {
            let (Some(f), Some(g)) = (t(raw).trc_function(), t(raw).inverse_trc_function()) else {
                continue;
            };
            // SAFETY: as above.
            let (cf, cg) = unsafe {
                (
                    ffi::av_csp_trc_func_from_id(c(raw)).unwrap(),
                    ffi::av_csp_trc_func_inv_from_id(c(raw)).unwrap(),
                )
            };
            for x in INPUTS {
                // SAFETY: `csp.c` transfer functions take any `f64`.
                let expect = unsafe { cg(cf(x)) };
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
    use ffmpeg::libavutil::csp::LumaCoefficients;
    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::pixfmt::ColorSpace;

    /// Every value C has a table slot for, its neighbours, and the extremes,
    /// as C's `unsigned` enum takes them.
    fn values() -> impl Iterator<Item = u32> {
        (0..ffi::AVColorSpace::AVCOL_SPC_NB.0 + 3).chain([u32::MAX, u32::MAX - 1, 1 << 31])
    }

    #[test]
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
/// [`ColorPrimariesDescriptor`]: ffmpeg::libavutil::csp::ColorPrimariesDescriptor
mod color_primaries_descriptor {
    use core::mem::{align_of, offset_of, size_of};
    use core::ptr::{self, NonNull, addr_of};

    use ffibox::CSlice;
    use ffmpeg::libavutil::csp::{
        CieXy, CieXyMut, ColorPrimariesDescriptor, ColorPrimariesDescriptorMut,
        ColorPrimariesDescriptorRef, PrimaryCoefficients,
    };
    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::mem::HeapBox;
    use ffmpeg::libavutil::rational::Rational;

    /// Every primaries value C may describe, past the end of both ranges
    /// included.
    fn primaries() -> impl Iterator<Item = ffi::AVColorPrimaries> {
        let nb = ffi::AVColorPrimaries::AVCOL_PRI_NB.0;
        let ext = ffi::AVColorPrimaries::AVCOL_PRI_EXT_BASE.0;
        let ext_nb = ffi::AVColorPrimaries::AVCOL_PRI_EXT_NB.0;
        (0..=nb).chain(ext..=ext_nb).map(ffi::AVColorPrimaries)
    }

    /// C's description for `p`, if it has one, as a safe handle over C's
    /// static storage: the test's one adoption of a C-owned
    /// `AVColorPrimariesDesc`.
    fn c_entry(p: ffi::AVColorPrimaries) -> Option<ColorPrimariesDescriptorRef<'static>> {
        // SAFETY: takes any value and returns NULL or a pointer into C's
        // static, never-written tables.
        let d = NonNull::new(unsafe { ffi::av_csp_primaries_desc_from_id(p) }.cast_mut())?;
        let d = d.cast::<ColorPrimariesDescriptor>();
        // SAFETY: `d` addresses one live, initialised, never-written
        // `AVColorPrimariesDesc` of a static table;
        // `ColorPrimariesDescriptor` is transparent over it.
        let one: CSlice<'static, _> = unsafe { CSlice::from_raw_parts(d, 1) };
        one.get(0)
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
                    unsafe { ffi::av_cmp_q(x.x, y.x) == 0 && ffi::av_cmp_q(x.y, y.y) == 0 }
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
        // SAFETY: `DmLevel10` mirrors `AVDOVIDmLevel10` field for field
        // (sizes and the field offset asserted by the test), and the shared
        // borrow keeps the parent live and unwritten while C reads the
        // projected field.
        unsafe { ffi::av_csp_primaries_id_from_desc(addr_of!((*c).target_display_primaries)) }
    }

    #[test]
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
