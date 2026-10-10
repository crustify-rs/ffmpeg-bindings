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
