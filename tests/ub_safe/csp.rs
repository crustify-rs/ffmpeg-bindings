//! UB tests for `libavutil/csp.h`, through the safe API only.
#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    reason = "test inputs are chosen within range, and the casts mirror the C conversions under comparison"
)]
#![expect(
    clippy::many_single_char_names,
    reason = "names follow the C fields, formulas and fixtures they mirror"
)]
#![expect(
    clippy::assert_is_empty,
    let_underscore_drop,
    reason = "assertions keep the form of the C behaviour they check"
)]

/// `CieXy`: inline values, `av_malloc`-family owners and arrays of them (the
/// shape of a gamut's three primaries), every struct and coordinate handle
/// form, copies outliving their source, and the asserted `Send` / `Sync`.
mod cie_xy {
    use std::thread;

    use ffmpeg_bindings::libavutil::libavutil::csp::{
        CieXy, CieXyMut, CieXyRef, WhitepointCoefficients,
    };
    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::mem::{HeapBox, alloc_cell_array_zeroed};
    use ffmpeg_bindings::libavutil::libavutil::rational::Rational;

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
    #[cfg_attr(miri, ignore = "calls C")]
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
        #[expect(clippy::clone_on_copy, reason = "exercises the explicit `Clone` impl")]
        let c = Clone::clone(&b);
        assert_eq!(parts(copy.as_ref()), PARTS[1]);
        assert_eq!(parts(c.as_ref()), [3127, 10000, 3290, -1]);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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

    use ffmpeg_bindings::libavutil::libavutil::csp::{
        LumaCoefficients, LumaCoefficientsMut, LumaCoefficientsRef,
    };
    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::mem::{HeapBox, alloc_cell_array_zeroed};
    use ffmpeg_bindings::libavutil::libavutil::rational::Rational;

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
    #[cfg_attr(miri, ignore = "calls C")]
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
        #[expect(clippy::clone_on_copy, reason = "exercises the explicit `Clone` impl")]
        let cl = Clone::clone(&b);
        assert_eq!(fields(copy.as_ref()), FIELDS[4]);
        assert_eq!(fields(cl.as_ref())[0], 3);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
        assert_eq!(
            b.as_handles().iter().map(fields).collect::<Vec<_>>(),
            FIELDS
        );
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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

/// `PrimaryCoefficients` as a value, behind a `HeapBox`, as the element of
/// `av_malloc`-family arrays, and through its primary handles, on every
/// coordinate edge and across threads.
mod primary_coefficients {
    use std::thread;

    use ffmpeg_bindings::libavutil::libavutil::csp::{
        CieXy, PrimaryCoefficients, PrimaryCoefficientsMut, PrimaryCoefficientsRef,
    };
    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::mem::{HeapBox, alloc_cell_array_zeroed};
    use ffmpeg_bindings::libavutil::libavutil::rational::Rational;

    /// Coordinate tuples at every edge: all-zero, BT.709's primaries, and
    /// the `int` limits.
    const FIELDS: [[i32; 12]; 5] = [
        [0; 12],
        [64, 100, 33, 100, 30, 100, 60, 100, 15, 100, 6, 100],
        [i32::MIN; 12],
        [i32::MAX; 12],
        [
            -1,
            0,
            0,
            -1,
            i32::MIN,
            i32::MAX,
            1,
            1,
            -1,
            -1,
            i32::MAX,
            i32::MIN,
        ],
    ];

    fn fields(p: PrimaryCoefficientsRef<'_>) -> [i32; 12] {
        let mut out = [0; 12];
        for (i, xy) in [p.r(), p.g(), p.b()].into_iter().enumerate() {
            let (x, y) = (xy.x(), xy.y());
            out[4 * i..4 * i + 4].copy_from_slice(&[x.num(), x.den(), y.num(), y.den()]);
        }
        out
    }

    fn fill(m: &mut PrimaryCoefficientsMut<'_>, f: [i32; 12]) {
        for i in 0..3 {
            let mut xy = match i {
                0 => m.r_mut(),
                1 => m.g_mut(),
                _ => m.b_mut(),
            };
            xy.x_mut().set_num(f[4 * i]);
            xy.x_mut().set_den(f[4 * i + 1]);
            xy.y_mut().set_num(f[4 * i + 2]);
            xy.y_mut().set_den(f[4 * i + 3]);
        }
    }

    /// Primaries built without a C call, so the tests using them also run
    /// under Miri.
    fn p(f: [i32; 12]) -> PrimaryCoefficients {
        let q = |num, den| Rational::from_raw(ffi::AVRational { num, den });
        let xy = |i: usize| CieXy::new(q(f[i], f[i + 1]), q(f[i + 2], f[i + 3]));
        PrimaryCoefficients::new(xy(0), xy(4), xy(8))
    }

    #[test]
    fn every_field_tuple_builds_and_reads_back() {
        for f in FIELDS {
            let mut v = PrimaryCoefficients::default();
            fill(&mut v.as_mut(), f);
            assert_eq!(fields(v.as_ref()), f);
            assert_eq!(fields(p(f).as_ref()), f);
            let back: ffi::AVPrimaryCoefficients = v.into();
            assert_eq!(fields(PrimaryCoefficients::from(back).as_ref()), f);
            assert!(!format!("{v:?}").is_empty());
        }
    }

    #[test]
    fn primary_handles_copy_coexist_and_outlive_the_parent_handle() {
        let v = p(FIELDS[1]);
        // The primary handles borrow `v`, not the parent handle they came
        // from, and so do the coordinate handles they lend.
        let (r, g, bx) = {
            let h = v.as_ref();
            (h.r(), h.g(), h.b().x())
        };
        let r2 = r;
        assert_eq!(
            (r.x().num(), r2.y().num(), g.x().num(), bx.num()),
            (64, 33, 30, 15)
        );
        assert!(!format!("{g:?}").is_empty());
    }

    #[test]
    fn exclusive_primary_handles_reborrow_one_at_a_time() {
        let mut v = p(FIELDS[1]);
        let mut m = v.as_mut();
        {
            let mut r = m.r_mut();
            r.set_x(Rational::from_raw(ffi::AVRational { num: 1, den: 3 }));
            let mut again = r.as_mut();
            again.y_mut().set_den(4);
            assert_eq!(r.as_ref().y().den(), 4);
        }
        m.b_mut().x_mut().set_num(-5);
        m.set_g(CieXy::default());
        let shared = m.as_ref();
        assert_eq!((shared.r().x().num(), shared.g().x().num()), (1, 0));
        m.set(PrimaryCoefficients::default());
        assert_eq!(fields(v.as_ref()), [0; 12]);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn copies_outlive_and_are_independent_of_their_source() {
        let (copy, g) = {
            let mut boxed = HeapBox::<PrimaryCoefficients>::new_zeroed().unwrap();
            boxed.as_mut().set(p(FIELDS[4]));
            let copy = boxed.as_ref().get();
            let g = boxed.as_ref().g().get();
            boxed.as_mut().g_mut().x_mut().set_num(9);
            (copy, g)
            // `boxed` is freed here; the copies hold their own bytes.
        };
        assert_eq!(fields(copy.as_ref()), FIELDS[4]);
        assert_eq!((g.as_ref().x().num(), g.as_ref().y().den()), (i32::MIN, 1));
        let mut b = copy;
        b.as_mut().r_mut().x_mut().set_num(3);
        #[expect(clippy::clone_on_copy, reason = "exercises the explicit `Clone` impl")]
        let cl = Clone::clone(&b);
        assert_eq!(fields(copy.as_ref()), FIELDS[4]);
        assert_eq!(fields(cl.as_ref())[0], 3);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn heap_box_owner_lives_reads_writes_clones_and_frees() {
        let mut a = HeapBox::<PrimaryCoefficients>::new_zeroed().unwrap();
        assert_eq!(fields(a.as_ref()), [0; 12]);
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
        drop(HeapBox::<PrimaryCoefficients>::new_uninit().unwrap());
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn heap_array_handles_stay_in_bounds() {
        let mut a = alloc_cell_array_zeroed::<PrimaryCoefficients>(FIELDS.len()).unwrap();
        {
            let mut handles = a.as_handles_mut();
            for (i, f) in FIELDS.into_iter().enumerate() {
                fill(&mut handles.get_mut(i).unwrap(), f);
            }
            assert!(handles.get_mut(FIELDS.len()).is_none());
        }
        let read: Vec<[i32; 12]> = a.as_handles().iter().map(fields).collect();
        assert_eq!(read, FIELDS);
        assert!(a.as_handles().get(FIELDS.len()).is_none());
        // The last element's last coordinate reads the array's last bytes.
        let last = a.as_handles().get(FIELDS.len() - 1).unwrap().b().y();
        assert_eq!(last.den(), i32::MIN);
        let b = a.clone();
        drop(a);
        assert_eq!(
            b.as_handles().iter().map(fields).collect::<Vec<_>>(),
            FIELDS
        );
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn values_and_owners_move_across_threads() {
        let v = p(FIELDS[1]);
        let b = HeapBox::<PrimaryCoefficients>::new_zeroed().unwrap();
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
    #[cfg_attr(miri, ignore = "calls C")]
    fn shared_handles_and_primary_handles_read_concurrently() {
        let v = p(FIELDS[1]);
        let boxed = HeapBox::<PrimaryCoefficients>::new_zeroed().unwrap();
        let (vr, br) = (v.as_ref(), boxed.as_ref());
        let g = vr.g();
        thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(move || {
                    for _ in 0..1000 {
                        assert_eq!(fields(vr), FIELDS[1]);
                        assert_eq!(g.y().num(), 60);
                        assert_eq!(fields(br), [0; 12]);
                    }
                });
            }
        });
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn exclusive_handles_write_on_other_threads() {
        let mut values = [PrimaryCoefficients::default(); 4];
        let mut boxed = HeapBox::<PrimaryCoefficients>::new_zeroed().unwrap();
        let mut boxed_mut = boxed.as_mut();
        thread::scope(|s| {
            for (i, v) in values.iter_mut().enumerate() {
                let mut m = v.as_mut();
                s.spawn(move || m.g_mut().x_mut().set_num(i as i32));
            }
            // A primary handle sent on its own.
            let mut b = boxed_mut.b_mut();
            s.spawn(move || b.y_mut().set_den(i32::MIN));
        });
        let g: Vec<i32> = values.iter().map(|v| v.as_ref().g().x().num()).collect();
        assert_eq!(g, [0, 1, 2, 3]);
        let mut expect = [0; 12];
        expect[11] = i32::MIN;
        assert_eq!(fields(boxed.as_ref()), expect);
    }

    #[test]
    fn a_shared_exclusive_handle_only_reads() {
        let mut v = p(FIELDS[1]);
        let m = v.as_mut();
        let m = &m;
        thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(move || assert_eq!(fields(m.as_ref()), FIELDS[1]));
            }
        });
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn heap_array_elements_written_from_separate_threads() {
        let mut a = alloc_cell_array_zeroed::<PrimaryCoefficients>(4).unwrap();
        let mut handles = a.as_handles_mut();
        thread::scope(|s| {
            for (m, f) in handles.iter_mut().zip(FIELDS) {
                let mut m = m;
                s.spawn(move || fill(&mut m, f));
            }
        });
        let read: Vec<[i32; 12]> = a.as_handles().iter().map(fields).collect();
        assert_eq!(read, FIELDS[..4]);
    }
}

/// The `ColorTransfer` getters on every value, the `unsigned` extremes
/// included, and the handles they return called on every input class, from
/// other threads, and after the getters ran again: the EOTFs on exactly
/// sized heap arrays, so a write past the three channels is an `ASan` report.
mod color_transfer {
    use std::thread;

    use ffmpeg_bindings::libavutil::libavutil::csp::{EotfFunction, TrcFunction};
    use ffmpeg_bindings::libavutil::libavutil::pixdesc::ColorTransfer;

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
            .map(ColorTransfer::from_raw)
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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

    use ffmpeg_bindings::libavutil::libavutil::csp::LumaCoefficientsRef;
    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::pixdesc::ColorSpace;

    fn values() -> impl Iterator<Item = u32> {
        (0..ffi::AVCOL_SPC_NB + 3).chain([u32::MAX, 1 << 31, (1 << 31) - 1])
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn every_value_looks_up_and_handles_cross_threads() {
        static KEPT: OnceLock<Vec<LumaCoefficientsRef<'static>>> = OnceLock::new();
        let kept = KEPT.get_or_init(|| {
            values()
                .filter_map(|raw| ColorSpace::from_raw(raw).luma_coefficients())
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
                        let _ = ColorSpace::from_raw(raw).luma_coefficients();
                    }
                });
            }
        });
    }
}

/// `ColorPrimariesDescriptor` as a value, behind a `HeapBox`, as the element
/// of `av_malloc`-family arrays, and through its white point and primaries
/// handles down to each coordinate, on every coordinate edge and across
/// threads.
mod color_primaries_descriptor {
    use std::thread;

    use ffmpeg_bindings::libavutil::libavutil::csp::{
        CieXy, CieXyMut, ColorPrimariesDescriptor, ColorPrimariesDescriptorMut,
        ColorPrimariesDescriptorRef, PrimaryCoefficients, WhitepointCoefficients,
    };
    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::mem::{HeapBox, alloc_cell_array_zeroed};
    use ffmpeg_bindings::libavutil::libavutil::rational::Rational;

    /// Coordinate tuples at every edge, white point first: all-zero, BT.709,
    /// the `int` limits and a mix of signs and zero denominators.
    const FIELDS: [[i32; 16]; 5] = [
        [0; 16],
        [
            3127, 10000, 3290, 10000, 64, 100, 33, 100, 30, 100, 60, 100, 15, 100, 6, 100,
        ],
        [i32::MIN; 16],
        [i32::MAX; 16],
        [
            1,
            0,
            -1,
            0,
            -1,
            0,
            0,
            -1,
            i32::MIN,
            i32::MAX,
            1,
            1,
            -1,
            -1,
            i32::MAX,
            i32::MIN,
        ],
    ];

    fn fields(d: ColorPrimariesDescriptorRef<'_>) -> [i32; 16] {
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

    fn set_parts(xy: &mut CieXyMut<'_>, p: &[i32]) {
        xy.x_mut().set_num(p[0]);
        xy.x_mut().set_den(p[1]);
        xy.y_mut().set_num(p[2]);
        xy.y_mut().set_den(p[3]);
    }

    fn fill(m: &mut ColorPrimariesDescriptorMut<'_>, f: [i32; 16]) {
        set_parts(&mut m.white_point_mut(), &f[..4]);
        let mut p = m.primaries_mut();
        set_parts(&mut p.r_mut(), &f[4..8]);
        set_parts(&mut p.g_mut(), &f[8..12]);
        set_parts(&mut p.b_mut(), &f[12..]);
    }

    /// A description built without a C call, so the tests using it also run
    /// under Miri.
    fn d(f: [i32; 16]) -> ColorPrimariesDescriptor {
        let q = |num, den| Rational::from_raw(ffi::AVRational { num, den });
        let xy = |i: usize| CieXy::new(q(f[i], f[i + 1]), q(f[i + 2], f[i + 3]));
        ColorPrimariesDescriptor::new(xy(0), PrimaryCoefficients::new(xy(4), xy(8), xy(12)))
    }

    #[test]
    fn every_field_tuple_builds_and_reads_back() {
        for f in FIELDS {
            let mut v = ColorPrimariesDescriptor::default();
            fill(&mut v.as_mut(), f);
            assert_eq!(fields(v.as_ref()), f);
            assert_eq!(fields(d(f).as_ref()), f);
            let back: ffi::AVColorPrimariesDesc = v.into();
            assert_eq!(fields(ColorPrimariesDescriptor::from(back).as_ref()), f);
            assert!(!format!("{v:?}").is_empty());
        }
    }

    #[test]
    fn field_handles_copy_coexist_and_outlive_the_parent_handle() {
        let v = d(FIELDS[1]);
        // The field handles borrow `v`, not the parent handle they came
        // from, and so do the primary and coordinate handles they lend.
        let (wp, prim, r, by) = {
            let h = v.as_ref();
            (
                h.white_point(),
                h.primaries(),
                h.primaries().r(),
                h.primaries().b().y(),
            )
        };
        let (wp2, prim2) = (wp, prim);
        assert_eq!(
            (
                wp.x().num(),
                wp2.y().num(),
                prim2.g().x().num(),
                r.y().num(),
                by.num()
            ),
            (3127, 3290, 30, 33, 6)
        );
        assert!(!format!("{prim:?} {wp:?}").is_empty());
    }

    #[test]
    fn exclusive_field_handles_reborrow_one_at_a_time() {
        let mut v = d(FIELDS[1]);
        let mut m = v.as_mut();
        {
            let mut p = m.primaries_mut();
            let mut r = p.r_mut();
            r.set_x(Rational::from_raw(ffi::AVRational { num: 1, den: 3 }));
            r.as_mut().y_mut().set_den(4);
            assert_eq!(r.as_ref().y().den(), 4);
            let mut again = p.as_mut();
            again.set_g(CieXy::default());
            assert_eq!(p.as_ref().r().x().den(), 3);
        }
        {
            let mut wp = m.white_point_mut();
            wp.x_mut().set_num(-5);
            let mut again = wp.as_mut();
            again.set_y(Rational::from_raw(ffi::AVRational { num: 1, den: 1 }));
        }
        let shared = m.as_ref();
        assert_eq!(
            (
                shared.white_point().x().num(),
                shared.white_point().y().den(),
                shared.primaries().r().x().num(),
                shared.primaries().g().x().num()
            ),
            (-5, 1, 1, 0)
        );
        let mut again = m.as_mut();
        again.set_primaries(PrimaryCoefficients::default());
        again.set_white_point(WhitepointCoefficients::default());
        assert_eq!(fields(m.as_ref()), [0; 16]);
        m.set(d(FIELDS[4]));
        assert_eq!(fields(v.as_ref()), FIELDS[4]);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn copies_outlive_and_are_independent_of_their_source() {
        let (copy, wp, prim) = {
            let mut boxed = HeapBox::<ColorPrimariesDescriptor>::new_zeroed().unwrap();
            boxed.as_mut().set(d(FIELDS[4]));
            let copy = boxed.as_ref().get();
            let wp = boxed.as_ref().white_point().get();
            let prim = boxed.as_ref().primaries().get();
            boxed.as_mut().white_point_mut().x_mut().set_num(9);
            boxed.as_mut().primaries_mut().r_mut().x_mut().set_num(9);
            (copy, wp, prim)
            // `boxed` is freed here; the copies hold their own bytes.
        };
        assert_eq!(fields(copy.as_ref()), FIELDS[4]);
        assert_eq!((wp.as_ref().x().num(), wp.as_ref().y().den()), (1, 0));
        assert_eq!(prim.as_ref().r().x().num(), -1);
        let mut b = copy;
        b.as_mut().white_point_mut().x_mut().set_num(3);
        #[expect(clippy::clone_on_copy, reason = "exercises the explicit `Clone` impl")]
        let cl = Clone::clone(&b);
        assert_eq!(fields(copy.as_ref()), FIELDS[4]);
        assert_eq!(fields(cl.as_ref())[0], 3);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn heap_box_owner_lives_reads_writes_clones_and_frees() {
        let mut a = HeapBox::<ColorPrimariesDescriptor>::new_zeroed().unwrap();
        assert_eq!(fields(a.as_ref()), [0; 16]);
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
        drop(HeapBox::<ColorPrimariesDescriptor>::new_uninit().unwrap());
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn heap_array_handles_stay_in_bounds() {
        let mut a = alloc_cell_array_zeroed::<ColorPrimariesDescriptor>(FIELDS.len()).unwrap();
        {
            let mut handles = a.as_handles_mut();
            for (i, f) in FIELDS.into_iter().enumerate() {
                fill(&mut handles.get_mut(i).unwrap(), f);
            }
            assert!(handles.get_mut(FIELDS.len()).is_none());
        }
        let read: Vec<[i32; 16]> = a.as_handles().iter().map(fields).collect();
        assert_eq!(read, FIELDS);
        assert!(a.as_handles().get(FIELDS.len()).is_none());
        // The last element's last coordinate reads the array's last bytes.
        let last = a.as_handles().get(FIELDS.len() - 1).unwrap();
        assert_eq!(last.primaries().b().y().den(), i32::MIN);
        let b = a.clone();
        drop(a);
        assert_eq!(
            b.as_handles().iter().map(fields).collect::<Vec<_>>(),
            FIELDS
        );
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn values_and_owners_move_across_threads() {
        let v = d(FIELDS[1]);
        let b = HeapBox::<ColorPrimariesDescriptor>::new_zeroed().unwrap();
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
    #[cfg_attr(miri, ignore = "calls C")]
    fn shared_handles_and_field_handles_read_concurrently() {
        let v = d(FIELDS[1]);
        let boxed = HeapBox::<ColorPrimariesDescriptor>::new_zeroed().unwrap();
        let (vr, br) = (v.as_ref(), boxed.as_ref());
        let (wp, prim) = (vr.white_point(), vr.primaries());
        thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(move || {
                    for _ in 0..1000 {
                        assert_eq!(fields(vr), FIELDS[1]);
                        assert_eq!((wp.y().num(), prim.g().y().num()), (3290, 60));
                        assert_eq!(fields(br), [0; 16]);
                    }
                });
            }
        });
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn exclusive_handles_write_on_other_threads() {
        let mut values = [ColorPrimariesDescriptor::default(); 4];
        let mut boxed = HeapBox::<ColorPrimariesDescriptor>::new_zeroed().unwrap();
        let mut boxed_mut = boxed.as_mut();
        thread::scope(|s| {
            for (i, v) in values.iter_mut().enumerate() {
                let mut m = v.as_mut();
                s.spawn(move || m.white_point_mut().x_mut().set_num(i as i32));
            }
            // A primaries handle sent on its own.
            let mut p = boxed_mut.primaries_mut();
            s.spawn(move || p.b_mut().y_mut().set_den(i32::MIN));
        });
        // Then a white point handle, once the primaries handle is gone.
        thread::scope(|s| {
            let mut wp = boxed_mut.white_point_mut();
            s.spawn(move || wp.y_mut().set_num(7));
        });
        let wp: Vec<i32> = values
            .iter()
            .map(|v| v.as_ref().white_point().x().num())
            .collect();
        assert_eq!(wp, [0, 1, 2, 3]);
        let mut expect = [0; 16];
        expect[2] = 7;
        expect[15] = i32::MIN;
        assert_eq!(fields(boxed.as_ref()), expect);
    }

    #[test]
    fn a_shared_exclusive_handle_only_reads() {
        let mut v = d(FIELDS[1]);
        let m = v.as_mut();
        let m = &m;
        thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(move || assert_eq!(fields(m.as_ref()), FIELDS[1]));
            }
        });
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn heap_array_elements_written_from_separate_threads() {
        let mut a = alloc_cell_array_zeroed::<ColorPrimariesDescriptor>(4).unwrap();
        let mut handles = a.as_handles_mut();
        thread::scope(|s| {
            for (m, f) in handles.iter_mut().zip(FIELDS) {
                let mut m = m;
                s.spawn(move || fill(&mut m, f));
            }
        });
        let read: Vec<[i32; 16]> = a.as_handles().iter().map(fields).collect();
        assert_eq!(read, FIELDS[..4]);
    }
}

/// `ColorPrimaries::descriptor` and `ColorPrimariesDescriptorRef::identify`:
/// every lookup value, the `'static` handles kept past the lookup and
/// identified from many threads, and identification of every owner form
/// (value, `HeapBox`, the last element of an `av_malloc` array, C's static
/// table) at every coordinate edge, while exclusive handles reborrow and
/// after the owner is written.
mod primaries_lookup {
    use std::sync::OnceLock;
    use std::thread;

    use ffmpeg_bindings::libavutil::libavutil::csp::{
        CieXy, ColorPrimariesDescriptor, ColorPrimariesDescriptorMut, ColorPrimariesDescriptorRef,
        PrimaryCoefficients,
    };
    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::mem::{HeapBox, alloc_cell_array_zeroed};
    use ffmpeg_bindings::libavutil::libavutil::pixdesc::ColorPrimaries;
    use ffmpeg_bindings::libavutil::libavutil::rational::Rational;

    fn values() -> impl Iterator<Item = u32> {
        let ext = ffi::AVCOL_PRI_EXT_BASE;
        (0..ffi::AVCOL_PRI_NB + 3)
            .chain(ext - 1..ffi::AVCOL_PRI_EXT_NB + 3)
            .chain([u32::MAX, 1 << 31, (1 << 31) - 1])
    }

    fn kept() -> &'static [ColorPrimariesDescriptorRef<'static>] {
        static KEPT: OnceLock<Vec<ColorPrimariesDescriptorRef<'static>>> = OnceLock::new();
        KEPT.get_or_init(|| {
            values()
                .filter_map(|raw| ColorPrimaries::from_raw(raw).descriptor())
                .collect()
        })
    }

    /// Coordinate parts at every edge for C's rational arithmetic: the
    /// `int` limits (whose cross products are C's widest), zero and
    /// negative denominators, and values one part off a table entry.
    const EDGES: [i32; 12] = [
        0,
        1,
        -1,
        i32::MAX,
        i32::MIN,
        i32::MAX - 1,
        i32::MIN + 1,
        100_000,
        -100_000,
        63_999,
        1 << 30,
        46_341,
    ];

    fn desc(f: [i32; 16]) -> ColorPrimariesDescriptor {
        let q = |num, den| Rational::from_raw(ffi::AVRational { num, den });
        let xy = |i: usize| CieXy::new(q(f[i], f[i + 1]), q(f[i + 2], f[i + 3]));
        ColorPrimariesDescriptor::new(xy(0), PrimaryCoefficients::new(xy(4), xy(8), xy(12)))
    }

    fn fields(d: ColorPrimariesDescriptorRef<'_>) -> [i32; 16] {
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

    /// BT.709's parts with every part, in turn, replaced by every edge, and
    /// every part set to each edge at once.
    fn edge_descriptions() -> Vec<ColorPrimariesDescriptor> {
        let bt709 = ColorPrimaries::BT709.descriptor().unwrap();
        let base = fields(bt709);
        let bt709 = bt709.get();
        let mut out = vec![bt709];
        for i in 0..16 {
            for e in EDGES {
                let mut f = base;
                f[i] = e;
                out.push(desc(f));
            }
        }
        out.extend(EDGES.map(|e| desc([e; 16])));
        out
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn every_value_looks_up_and_identifies_across_threads() {
        let kept = kept();
        assert!(kept.len() >= 12, "{}", kept.len());
        thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(|| {
                    for d in kept {
                        let _ = d.identify();
                        assert_ne!(d.primaries().r().x().num(), 0);
                    }
                    for raw in values() {
                        let p = ColorPrimaries::from_raw(raw);
                        let _ = p.descriptor().map(|d| d.identify());
                    }
                });
            }
        });
        // A `'static` handle moves into a detached thread on its own.
        let d = ColorPrimaries::BT2020.descriptor().unwrap();
        let id = thread::spawn(move || d.identify()).join().unwrap();
        assert_eq!(id, Some(ColorPrimaries::BT2020));
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn edge_descriptions_identify_in_every_owner_form() {
        let all = edge_descriptions();
        let mut boxed = HeapBox::<ColorPrimariesDescriptor>::new_zeroed().unwrap();
        // The description C reads is the array's last element, ending at the
        // block's end.
        let mut array = alloc_cell_array_zeroed::<ColorPrimariesDescriptor>(3).unwrap();
        for d in &all {
            let id = d.as_ref().identify();
            boxed.as_mut().set(*d);
            assert_eq!(boxed.as_ref().identify(), id);
            let mut handles = array.as_handles_mut();
            handles.get_mut(2).unwrap().set(*d);
            let last = array.as_handles().get(2).unwrap();
            assert_eq!(last.identify(), id);
        }
        // The all-zero and every zero-denominator description match nothing.
        assert_eq!(
            HeapBox::<ColorPrimariesDescriptor>::new_zeroed()
                .unwrap()
                .as_ref()
                .identify(),
            None
        );
        for d in &all {
            let f = fields(d.as_ref());
            if (0..8).any(|i| f[2 * i + 1] == 0) {
                assert_eq!(d.as_ref().identify(), None, "{f:?}");
            }
        }
    }

    fn reborrow_identify(mut m: ColorPrimariesDescriptorMut<'_>) -> Option<ColorPrimaries> {
        // A shared reborrow of the exclusive handle, read by C, then a
        // write through the exclusive handle once it is gone.
        let id = m.as_ref().identify();
        m.white_point_mut().x_mut().set_num(0);
        assert_eq!(m.as_ref().identify(), None);
        id
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn identify_through_reborrows_and_after_writes() {
        let reference = ColorPrimaries::SMPTE431.descriptor().unwrap();
        let mut value = reference.get();
        assert_eq!(
            reborrow_identify(value.as_mut()),
            Some(ColorPrimaries::SMPTE431)
        );
        let mut boxed = HeapBox::<ColorPrimariesDescriptor>::new_zeroed().unwrap();
        boxed.as_mut().set(reference.get());
        assert_eq!(
            reborrow_identify(boxed.as_mut()),
            Some(ColorPrimaries::SMPTE431)
        );
        // A copy outlives its owner with the write, and C's table entry is
        // unchanged.
        let copy = boxed.as_ref().get();
        drop(boxed);
        assert_eq!(copy.as_ref().identify(), None);
        assert_eq!(reference.identify(), Some(ColorPrimaries::SMPTE431));
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn shared_owners_identify_concurrently() {
        let all = edge_descriptions();
        let mut boxed = HeapBox::<ColorPrimariesDescriptor>::new_zeroed().unwrap();
        boxed
            .as_mut()
            .set(ColorPrimaries::FILM.descriptor().unwrap().get());
        let (boxed, all) = (&boxed, &all);
        thread::scope(|s| {
            for t in 0..4 {
                s.spawn(move || {
                    assert_eq!(boxed.as_ref().identify(), Some(ColorPrimaries::FILM));
                    for d in all.iter().skip(t).step_by(4) {
                        let _ = d.as_ref().identify();
                    }
                });
            }
        });
    }
}
