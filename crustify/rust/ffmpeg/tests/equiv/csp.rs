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
