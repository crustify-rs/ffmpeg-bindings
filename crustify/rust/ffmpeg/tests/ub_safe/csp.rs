//! UB tests for `libavutil/csp.h`, through the safe API only.

/// `CieXy`: inline values, `av_malloc`-family owners and arrays of them (the
/// shape of a gamut's three primaries), every struct and coordinate handle
/// form, copies outliving their source, and the asserted `Send` / `Sync`.
mod cie_xy {
    use std::thread;

    use ffmpeg::libavutil::csp::{CieXy, CieXyMut, CieXyRef, WhitepointCoefficients};
    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::mem::{HeapBox, alloc_cell_array_zeroed};
    use ffmpeg::libavutil::rational::Rational;

    /// Coordinate parts at every edge: unspecified, C's own white point and
    /// primaries, the infinities and the `int` limits.
    const PARTS: [[i32; 4]; 6] = [
        [0, 0, 0, 0],
        [3127, 10000, 3290, 10000],
        [64, 100, 33, 100],
        [1, 0, -1, 0],
        [i32::MIN, i32::MIN, i32::MIN, i32::MIN],
        [i32::MAX, -1, i32::MIN, i32::MAX],
    ];

    fn parts(xy: CieXyRef<'_>) -> [i32; 4] {
        let (x, y) = (xy.x(), xy.y());
        [x.num(), x.den(), y.num(), y.den()]
    }

    /// Writes `p` one part at a time through the coordinate handles.
    fn fill(m: &mut CieXyMut<'_>, [xn, xd, yn, yd]: [i32; 4]) {
        let mut x = m.x_mut();
        x.set_num(xn);
        x.set_den(xd);
        let mut y = m.y_mut();
        y.set_num(yn);
        y.set_den(yd);
    }

    /// A rational built without a C call, so the tests using it also run
    /// under Miri.
    fn rat(num: i32, den: i32) -> Rational {
        Rational::from_raw(ffi::AVRational { num, den })
    }

    /// A chromaticity built without a C call, so the tests using it also
    /// run under Miri.
    fn xy([xn, xd, yn, yd]: [i32; 4]) -> CieXy {
        CieXy::from_raw(ffi::AVCIExy {
            x: ffi::AVRational { num: xn, den: xd },
            y: ffi::AVRational { num: yn, den: yd },
        })
    }

    #[test]
    fn every_part_tuple_builds_and_reads_back() {
        for p in PARTS {
            let mut v = CieXy::default();
            fill(&mut v.as_mut(), p);
            assert_eq!(parts(v.as_ref()), p);
            assert_eq!(parts(xy(p).as_ref()), p);
            let mut w = CieXy::default();
            w.as_mut().set_x(rat(p[0], p[1]));
            w.as_mut().set_y(rat(p[2], p[3]));
            assert_eq!(parts(w.as_ref()), p);
            let back: ffi::AVCIExy = v.into();
            assert_eq!([back.x.num, back.x.den, back.y.num, back.y.den], p);
            assert!(!format!("{v:?}").is_empty());
        }
        let wp: WhitepointCoefficients = xy(PARTS[1]);
        assert_eq!(parts(wp.as_ref()), PARTS[1]);
    }

    #[test]
    fn shared_and_coordinate_handles_copy_and_coexist() {
        let v = xy(PARTS[1]);
        let a = v.as_ref();
        let b = a;
        let (x, y) = (a.x(), b.y());
        let x2 = x;
        assert_eq!((x.num(), x2.den(), y.num()), (3127, 10000, 3290));
        assert_eq!(parts(v.as_ref()), PARTS[1]);
        assert_eq!(parts(b.get().as_ref()), PARTS[1]);
    }

    fn reset(mut m: CieXyMut<'_>) {
        m.set(CieXy::default());
    }

    #[test]
    fn exclusive_handle_reborrows_without_aliasing() {
        let mut v = xy(PARTS[2]);
        let mut m = v.as_mut();
        reset(m.as_mut());
        assert_eq!(parts(m.as_ref()), [0; 4]);
        {
            let mut x = m.x_mut();
            x.set_num(7);
            let mut inner = x.as_mut();
            inner.set_den(8);
            assert_eq!(x.as_ref().get().into_raw().den, 8);
        }
        m.y_mut().set(rat(-1, 0));
        let shared = m.as_ref();
        assert_eq!(parts(shared), [7, 8, -1, 0]);
        m.set_x(rat(i32::MIN, 1));
        assert_eq!(parts(v.as_ref()), [i32::MIN, 1, -1, 0]);
    }

    #[test]
    fn copies_outlive_and_are_independent_of_their_source() {
        let copy = {
            let mut boxed = HeapBox::<CieXy>::new_zeroed().unwrap();
            boxed.as_mut().set(xy(PARTS[1]));
            let copy = boxed.as_ref().get();
            let x = boxed.as_ref().x().get();
            boxed.as_mut().x_mut().set_num(0);
            assert_eq!(x.as_ref().num(), 3127);
            copy
            // `boxed` is freed here; `copy` holds its own bytes.
        };
        assert_eq!(parts(copy.as_ref()), PARTS[1]);
        let mut b = copy;
        b.as_mut().y_mut().set_den(-1);
        #[allow(clippy::clone_on_copy)] // exercises the explicit `Clone` impl
        let c = Clone::clone(&b);
        assert_eq!(parts(copy.as_ref()), PARTS[1]);
        assert_eq!(parts(c.as_ref()), [3127, 10000, 3290, -1]);
    }

    #[test]
    fn heap_box_owner_lives_reads_writes_clones_and_frees() {
        let mut a = HeapBox::<CieXy>::new_zeroed().unwrap();
        assert_eq!(parts(a.as_ref()), [0; 4]);
        for p in PARTS {
            fill(&mut a.as_mut(), p);
            assert_eq!(parts(a.as_ref()), p);
        }
        let b = a.clone();
        let c = b.try_clone().unwrap();
        // The source may go first; the copies own their own blocks.
        drop(a);
        assert_eq!(parts(b.as_ref()), PARTS[5]);
        drop(b);
        assert_eq!(parts(c.as_ref()), PARTS[5]);
        drop(HeapBox::<CieXy>::new_uninit().unwrap());
    }

    #[test]
    fn heap_array_handles_stay_in_bounds() {
        let mut a = alloc_cell_array_zeroed::<CieXy>(PARTS.len()).unwrap();
        assert!(a.as_handles().iter().all(|h| parts(h) == [0; 4]));
        {
            let mut handles = a.as_handles_mut();
            for (i, p) in PARTS.into_iter().enumerate() {
                fill(&mut handles.get_mut(i).unwrap(), p);
            }
            assert!(handles.get_mut(PARTS.len()).is_none());
        }
        let read: Vec<[i32; 4]> = a.as_handles().iter().map(parts).collect();
        assert_eq!(read, PARTS);
        assert!(a.as_handles().get(PARTS.len()).is_none());
        // A coordinate handle of the last element reads its own place only.
        let last = a.as_handles().get(PARTS.len() - 1).unwrap().y();
        assert_eq!((last.num(), last.den()), (i32::MIN, i32::MAX));
        let b = a.clone();
        drop(a);
        assert_eq!(b.as_handles().iter().map(parts).collect::<Vec<_>>(), PARTS);
        drop(alloc_cell_array_zeroed::<CieXy>(0).unwrap());
    }

    #[test]
    fn values_and_owners_move_across_threads() {
        let v = xy(PARTS[2]);
        let b = HeapBox::<CieXy>::new_zeroed().unwrap();
        let (v, b) = thread::spawn(move || {
            let mut b = b;
            b.as_mut().set(v);
            (v, b)
        })
        .join()
        .unwrap();
        assert_eq!(parts(b.as_ref()), parts(v.as_ref()));
        // Freed on another thread than the one that allocated it.
        thread::spawn(move || drop(b)).join().unwrap();
    }

    #[test]
    fn shared_handles_read_concurrently() {
        let v = xy(PARTS[1]);
        let boxed = HeapBox::<CieXy>::new_zeroed().unwrap();
        let (vr, br) = (v.as_ref(), boxed.as_ref());
        let vx = vr.x();
        thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(move || {
                    for _ in 0..1000 {
                        assert_eq!(parts(vr), PARTS[1]);
                        assert_eq!(vx.num(), 3127);
                        assert_eq!(parts(br), [0; 4]);
                    }
                });
            }
        });
    }

    #[test]
    fn exclusive_handles_write_on_other_threads() {
        let mut values = [CieXy::default(); 4];
        let mut boxed = HeapBox::<CieXy>::new_zeroed().unwrap();
        thread::scope(|s| {
            for (i, v) in values.iter_mut().enumerate() {
                let mut m = v.as_mut();
                s.spawn(move || m.x_mut().set_num(i as i32));
            }
            let mut m = boxed.as_mut();
            s.spawn(move || m.set_y(rat(i32::MIN, 3)));
        });
        let xs: Vec<i32> = values.iter().map(|v| v.as_ref().x().num()).collect();
        assert_eq!(xs, [0, 1, 2, 3]);
        assert_eq!(parts(boxed.as_ref()), [0, 0, i32::MIN, 3]);
    }

    #[test]
    fn a_shared_exclusive_handle_only_reads() {
        let mut v = xy(PARTS[2]);
        let m = v.as_mut();
        let m = &m;
        thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(move || assert_eq!(parts(m.as_ref()), PARTS[2]));
            }
        });
    }

    #[test]
    fn heap_array_elements_written_from_separate_threads() {
        let mut a = alloc_cell_array_zeroed::<CieXy>(4).unwrap();
        let mut handles = a.as_handles_mut();
        thread::scope(|s| {
            // Each element is reached through its own exclusive handle.
            for (m, p) in handles.iter_mut().zip(PARTS) {
                let mut m = m;
                s.spawn(move || fill(&mut m, p));
            }
        });
        let read: Vec<[i32; 4]> = a.as_handles().iter().map(parts).collect();
        assert_eq!(read, PARTS[..4]);
    }

    #[test]
    fn equality_compares_without_touching_memory_out_of_place() {
        // `==` calls C's `av_cmp_q` on copies of each coordinate.
        let boxed = {
            let mut b = HeapBox::<CieXy>::new_zeroed().unwrap();
            b.as_mut().set(xy([1, 2, 3, 4]));
            b
        };
        let v = boxed.as_ref().get();
        drop(boxed);
        assert_eq!(v, xy([2, 4, 6, 8]));
        assert_ne!(v, xy(PARTS[0]));
        assert_ne!(xy(PARTS[4]), xy(PARTS[5]));
    }
}

/// `LumaCoefficients`: inline values, `av_malloc`-family owners and arrays
/// of them, the shared and exclusive field handles over each coefficient,
/// copies outliving their source, and the asserted `Send` / `Sync`.
mod luma_coefficients {
    use std::thread;

    use ffmpeg::libavutil::csp::{LumaCoefficients, LumaCoefficientsMut, LumaCoefficientsRef};
    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::mem::{HeapBox, alloc_cell_array_zeroed};
    use ffmpeg::libavutil::rational::Rational;

    /// Coefficient tuples at every edge: all-zero, BT.709's, and the `int`
    /// limits.
    const FIELDS: [[i32; 6]; 5] = [
        [0; 6],
        [2126, 10000, 7152, 10000, 722, 10000],
        [i32::MIN; 6],
        [i32::MAX; 6],
        [-1, 0, 0, -1, i32::MIN, i32::MAX],
    ];

    fn fields(c: LumaCoefficientsRef<'_>) -> [i32; 6] {
        [
            c.cr().num(),
            c.cr().den(),
            c.cg().num(),
            c.cg().den(),
            c.cb().num(),
            c.cb().den(),
        ]
    }

    fn fill(m: &mut LumaCoefficientsMut<'_>, f: [i32; 6]) {
        m.cr_mut().set_num(f[0]);
        m.cr_mut().set_den(f[1]);
        m.cg_mut().set_num(f[2]);
        m.cg_mut().set_den(f[3]);
        m.cb_mut().set_num(f[4]);
        m.cb_mut().set_den(f[5]);
    }

    /// Coefficients built without a C call, so the tests using them also
    /// run under Miri.
    fn c(f: [i32; 6]) -> LumaCoefficients {
        let q = |num, den| Rational::from_raw(ffi::AVRational { num, den });
        LumaCoefficients::new(q(f[0], f[1]), q(f[2], f[3]), q(f[4], f[5]))
    }

    #[test]
    fn every_field_tuple_builds_and_reads_back() {
        for f in FIELDS {
            let mut v = LumaCoefficients::default();
            fill(&mut v.as_mut(), f);
            assert_eq!(fields(v.as_ref()), f);
            assert_eq!(fields(c(f).as_ref()), f);
            let back: ffi::AVLumaCoefficients = v.into();
            assert_eq!(fields(LumaCoefficients::from(back).as_ref()), f);
            assert!(!format!("{v:?}").is_empty());
        }
    }

    #[test]
    fn field_handles_copy_coexist_and_outlive_the_parent_handle() {
        let v = c(FIELDS[1]);
        // The field handles borrow `v`, not the parent handle they came from.
        let (cr, cg, cb) = {
            let r = v.as_ref();
            (r.cr(), r.cg(), r.cb())
        };
        let cr2 = cr;
        assert_eq!(
            (cr.num(), cr2.den(), cg.num(), cb.den()),
            (2126, 10000, 7152, 10000)
        );
        assert!(!format!("{cg:?}").is_empty());
    }

    #[test]
    fn exclusive_field_handles_reborrow_one_at_a_time() {
        let mut v = c(FIELDS[1]);
        let mut m = v.as_mut();
        {
            let mut cr = m.cr_mut();
            cr.set(Rational::from_raw(ffi::AVRational { num: 1, den: 3 }));
            let mut again = cr.as_mut();
            again.set_den(4);
            assert_eq!(cr.as_ref().den(), 4);
        }
        m.cb_mut().set_num(-5);
        let shared = m.as_ref();
        assert_eq!(shared.cr().num(), 1);
        m.set(LumaCoefficients::default());
        assert_eq!(fields(v.as_ref()), [0; 6]);
    }

    #[test]
    fn copies_outlive_and_are_independent_of_their_source() {
        let (copy, cg) = {
            let mut boxed = HeapBox::<LumaCoefficients>::new_zeroed().unwrap();
            boxed.as_mut().set(c(FIELDS[4]));
            let copy = boxed.as_ref().get();
            let cg = boxed.as_ref().cg().get();
            boxed.as_mut().cg_mut().set_num(9);
            (copy, cg)
            // `boxed` is freed here; the copies hold their own bytes.
        };
        assert_eq!(fields(copy.as_ref()), FIELDS[4]);
        assert_eq!((cg.as_ref().num(), cg.as_ref().den()), (0, -1));
        let mut b = copy;
        b.as_mut().cr_mut().set_num(3);
        #[allow(clippy::clone_on_copy)] // exercises the explicit `Clone` impl
        let cl = Clone::clone(&b);
        assert_eq!(fields(copy.as_ref()), FIELDS[4]);
        assert_eq!(fields(cl.as_ref())[0], 3);
    }

    #[test]
    fn heap_box_owner_lives_reads_writes_clones_and_frees() {
        let mut a = HeapBox::<LumaCoefficients>::new_zeroed().unwrap();
        assert_eq!(fields(a.as_ref()), [0; 6]);
        for f in FIELDS {
            fill(&mut a.as_mut(), f);
            assert_eq!(fields(a.as_ref()), f);
        }
        let b = a.clone();
        let cl = b.try_clone().unwrap();
        drop(a);
        assert_eq!(fields(b.as_ref()), FIELDS[4]);
        drop(b);
        assert_eq!(fields(cl.as_ref()), FIELDS[4]);
        drop(HeapBox::<LumaCoefficients>::new_uninit().unwrap());
    }

    #[test]
    fn heap_array_handles_stay_in_bounds() {
        let mut a = alloc_cell_array_zeroed::<LumaCoefficients>(FIELDS.len()).unwrap();
        {
            let mut handles = a.as_handles_mut();
            for (i, f) in FIELDS.into_iter().enumerate() {
                fill(&mut handles.get_mut(i).unwrap(), f);
            }
            assert!(handles.get_mut(FIELDS.len()).is_none());
        }
        let read: Vec<[i32; 6]> = a.as_handles().iter().map(fields).collect();
        assert_eq!(read, FIELDS);
        assert!(a.as_handles().get(FIELDS.len()).is_none());
        // The last element's last field handle reads the array's last bytes.
        let last = a.as_handles().get(FIELDS.len() - 1).unwrap().cb();
        assert_eq!(last.den(), i32::MAX);
        let b = a.clone();
        drop(a);
        assert_eq!(b.as_handles().iter().map(fields).collect::<Vec<_>>(), FIELDS);
    }

    #[test]
    fn values_and_owners_move_across_threads() {
        let v = c(FIELDS[1]);
        let b = HeapBox::<LumaCoefficients>::new_zeroed().unwrap();
        let (v, b) = thread::spawn(move || {
            let mut b = b;
            b.as_mut().set(v);
            (v, b)
        })
        .join()
        .unwrap();
        assert_eq!(fields(b.as_ref()), fields(v.as_ref()));
        thread::spawn(move || drop(b)).join().unwrap();
    }

    #[test]
    fn shared_handles_and_field_handles_read_concurrently() {
        let v = c(FIELDS[1]);
        let boxed = HeapBox::<LumaCoefficients>::new_zeroed().unwrap();
        let (vr, br) = (v.as_ref(), boxed.as_ref());
        let cg = vr.cg();
        thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(move || {
                    for _ in 0..1000 {
                        assert_eq!(fields(vr), FIELDS[1]);
                        assert_eq!(cg.num(), 7152);
                        assert_eq!(fields(br), [0; 6]);
                    }
                });
            }
        });
    }

    #[test]
    fn exclusive_handles_write_on_other_threads() {
        let mut values = [LumaCoefficients::default(); 4];
        let mut boxed = HeapBox::<LumaCoefficients>::new_zeroed().unwrap();
        let mut boxed_mut = boxed.as_mut();
        thread::scope(|s| {
            for (i, v) in values.iter_mut().enumerate() {
                let mut m = v.as_mut();
                s.spawn(move || m.cg_mut().set_num(i as i32));
            }
            // A field handle sent on its own.
            let mut cb = boxed_mut.cb_mut();
            s.spawn(move || cb.set_den(i32::MIN));
        });
        let g: Vec<i32> = values.iter().map(|v| v.as_ref().cg().num()).collect();
        assert_eq!(g, [0, 1, 2, 3]);
        assert_eq!(fields(boxed.as_ref()), [0, 0, 0, 0, 0, i32::MIN]);
    }

    #[test]
    fn a_shared_exclusive_handle_only_reads() {
        let mut v = c(FIELDS[1]);
        let m = v.as_mut();
        let m = &m;
        thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(move || assert_eq!(fields(m.as_ref()), FIELDS[1]));
            }
        });
    }

    #[test]
    fn heap_array_elements_written_from_separate_threads() {
        let mut a = alloc_cell_array_zeroed::<LumaCoefficients>(4).unwrap();
        let mut handles = a.as_handles_mut();
        thread::scope(|s| {
            for (m, f) in handles.iter_mut().zip(FIELDS) {
                let mut m = m;
                s.spawn(move || fill(&mut m, f));
            }
        });
        let read: Vec<[i32; 6]> = a.as_handles().iter().map(fields).collect();
        assert_eq!(read, FIELDS[..4]);
    }
}

/// The `ColorTransfer` getters on every value, the `unsigned` extremes
/// included, and the handles they return called on every input class, from
/// other threads, and after the getters ran again: the EOTFs on exactly
/// sized heap arrays, so a write past the three channels is an ASan report.
mod color_transfer {
    use std::thread;

    use ffmpeg::libavutil::csp::{EotfFunction, TrcFunction};
    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::pixfmt::ColorTransfer;

    fn transfers() -> Vec<ColorTransfer> {
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
            .map(|r| ColorTransfer::from_raw(ffi::AVColorTransferCharacteristic(r)))
            .collect()
    }

    const INPUTS: [f64; 9] = [
        0.0,
        -0.0,
        1.0,
        0.5,
        -1.0,
        2.0,
        1e308,
        f64::INFINITY,
        f64::NAN,
    ];

    fn handles() -> (Vec<TrcFunction>, Vec<EotfFunction>) {
        let mut trc = Vec::new();
        let mut eotf = Vec::new();
        for t in transfers() {
            trc.extend(t.trc_function());
            trc.extend(t.inverse_trc_function());
            eotf.extend(t.eotf());
            eotf.extend(t.inverse_eotf());
        }
        (trc, eotf)
    }

    #[test]
    fn getters_accept_every_value() {
        for t in transfers() {
            for g in [t.approximate_trc_gamma(), t.approximate_eotf_gamma()]
                .into_iter()
                .flatten()
            {
                assert!(g > 0.0 && g.is_finite());
            }
        }
        let (trc, eotf) = handles();
        assert_eq!((trc.len(), eotf.len()), (34, 28));
    }

    #[test]
    fn handles_run_on_every_input_in_bounds() {
        let (trc, eotf) = handles();
        for f in &trc {
            for x in INPUTS {
                let _ = f.call(x);
            }
        }
        for f in &eotf {
            for (lw, lb) in [
                (1000.0, 0.005),
                (0.0, 0.0),
                (-1.0, f64::NAN),
                (f64::MAX, f64::MIN),
            ] {
                for x in INPUTS {
                    let mut c = Box::new([x, -x, x * 0.5]);
                    f.call(lw, lb, &mut c);
                }
            }
        }
    }

    #[test]
    fn handles_outlive_the_getters_and_cross_threads() {
        let (trc, eotf) = handles();
        let expect: Vec<u64> = trc.iter().map(|f| f.call(0.5).to_bits()).collect();
        // The getters run again, elsewhere, while the handles are held.
        thread::scope(|s| {
            for t in 0..8 {
                let (trc, eotf, expect) = (&trc, &eotf, &expect);
                s.spawn(move || {
                    let _ = handles();
                    for (f, e) in trc.iter().zip(expect).skip(t) {
                        assert_eq!(f.call(0.5).to_bits(), *e);
                    }
                    for f in eotf.iter().skip(t) {
                        let mut c = [0.25, 0.5, 0.75];
                        f.call(100.0, 0.1, &mut c);
                    }
                });
            }
        });
        let moved =
            thread::spawn(move || trc.into_iter().filter(|f| !f.call(1.0).is_nan()).count())
                .join()
                .unwrap();
        assert_eq!(moved, 34);
    }
}

/// `ColorSpace::luma_coefficients`: every value, the `'static` handles kept
/// past the lookup and read from many threads.
mod luma_lookup {
    use std::sync::OnceLock;
    use std::thread;

    use ffmpeg::libavutil::csp::LumaCoefficientsRef;
    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::pixfmt::ColorSpace;

    fn values() -> impl Iterator<Item = u32> {
        (0..ffi::AVColorSpace::AVCOL_SPC_NB.0 + 3).chain([u32::MAX, 1 << 31, (1 << 31) - 1])
    }

    #[test]
    fn every_value_looks_up_and_handles_cross_threads() {
        static KEPT: OnceLock<Vec<LumaCoefficientsRef<'static>>> = OnceLock::new();
        let kept = KEPT.get_or_init(|| {
            values()
                .filter_map(|raw| ColorSpace::from_raw(ffi::AVColorSpace(raw)).luma_coefficients())
                .collect()
        });
        assert!(!kept.is_empty());
        thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(|| {
                    for c in kept {
                        let sum = [c.cr(), c.cg(), c.cb()]
                            .map(|q| q.get().to_f64())
                            .iter()
                            .sum::<f64>();
                        assert!(sum > 0.9, "{sum}");
                    }
                    for raw in values() {
                        let _ = ColorSpace::from_raw(ffi::AVColorSpace(raw)).luma_coefficients();
                    }
                });
            }
        });
    }
}
