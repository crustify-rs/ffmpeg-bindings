//! UB tests for `libavutil/pixdesc.h`, through the safe API only.

/// `ComponentDescriptor`: inline values, `av_malloc`-family owners and arrays
/// of them (the shape of a descriptor's `comp[4]`), every handle form, copies
/// outliving their source, and the asserted `Send` / `Sync`.
mod component_descriptor {
    use std::thread;

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::mem::{HeapBox, alloc_cell_array_zeroed};
    use ffmpeg::libavutil::pixdesc::{
        ComponentDescriptor, ComponentDescriptorMut, ComponentDescriptorRef,
    };

    /// Field tuples at every edge: all-zero, C's own extremes (a negative
    /// offset, 32-bit depth and step), and the `int` limits.
    const FIELDS: [[i32; 5]; 6] = [
        [0, 0, 0, 0, 0],
        [0, 2, -1, 3, 5],
        [3, 32, 30, 7, 32],
        [i32::MIN, i32::MIN, i32::MIN, i32::MIN, i32::MIN],
        [i32::MAX, i32::MAX, i32::MAX, i32::MAX, i32::MAX],
        [-1, 0, i32::MAX, -1, i32::MIN],
    ];

    fn fields(d: ComponentDescriptorRef<'_>) -> [i32; 5] {
        [d.plane(), d.step(), d.offset(), d.shift(), d.depth()]
    }

    fn fill(m: &mut ComponentDescriptorMut<'_>, [plane, step, offset, shift, depth]: [i32; 5]) {
        m.set_plane(plane);
        m.set_step(step);
        m.set_offset(offset);
        m.set_shift(shift);
        m.set_depth(depth);
    }

    /// A descriptor built without a C call, so the tests using it also run
    /// under Miri.
    fn d([plane, step, offset, shift, depth]: [i32; 5]) -> ComponentDescriptor {
        ComponentDescriptor::from_raw(ffi::AVComponentDescriptor {
            plane,
            step,
            offset,
            shift,
            depth,
        })
    }

    #[test]
    fn every_field_tuple_builds_and_reads_back() {
        for f in FIELDS {
            let mut v = ComponentDescriptor::new();
            fill(&mut v.as_mut(), f);
            assert_eq!(fields(v.as_ref()), f);
            assert_eq!(v, d(f));
            let back: ffi::AVComponentDescriptor = v.into();
            assert_eq!(
                [back.plane, back.step, back.offset, back.shift, back.depth],
                f
            );
            assert!(!format!("{v:?}").is_empty());
        }
        assert_eq!(fields(ComponentDescriptor::default().as_ref()), [0; 5]);
    }

    #[test]
    fn shared_handles_copy_and_coexist() {
        let v = d(FIELDS[1]);
        let a = v.as_ref();
        let b = a;
        let c = v.as_ref();
        assert_eq!((a.offset(), b.depth(), fields(c)), (-1, 5, FIELDS[1]));
    }

    fn reset(mut m: ComponentDescriptorMut<'_>) {
        m.set(ComponentDescriptor::new());
    }

    #[test]
    fn exclusive_handle_reborrows_without_aliasing() {
        let mut v = d(FIELDS[2]);
        let mut m = v.as_mut();
        reset(m.as_mut());
        assert_eq!(fields(m.as_ref()), [0; 5]);
        m.set_depth(i32::MIN);
        let shared = m.as_ref();
        assert_eq!(shared.depth(), i32::MIN);
        m.set_plane(3);
        assert_eq!(fields(v.as_ref()), [3, 0, 0, 0, i32::MIN]);
    }

    #[test]
    fn copies_outlive_and_are_independent_of_their_source() {
        let copy = {
            let mut boxed = HeapBox::<ComponentDescriptor>::new_zeroed().unwrap();
            boxed.as_mut().set(d(FIELDS[1]));
            let copy = boxed.as_ref().get();
            boxed.as_mut().set_offset(0);
            copy
            // `boxed` is freed here; `copy` holds its own bytes.
        };
        assert_eq!(fields(copy.as_ref()), FIELDS[1]);
        let mut b = copy;
        b.as_mut().set_shift(-1);
        #[allow(clippy::clone_on_copy)] // exercises the explicit `Clone` impl
        let c = Clone::clone(&b);
        assert_eq!(fields(copy.as_ref()), FIELDS[1]);
        assert_eq!(fields(c.as_ref()), [0, 2, -1, -1, 5]);
    }

    #[test]
    fn heap_box_owner_lives_reads_writes_clones_and_frees() {
        let mut a = HeapBox::<ComponentDescriptor>::new_zeroed().unwrap();
        assert_eq!(fields(a.as_ref()), [0; 5]);
        for f in FIELDS {
            fill(&mut a.as_mut(), f);
            assert_eq!(fields(a.as_ref()), f);
        }
        let b = a.clone();
        let c = b.try_clone().unwrap();
        // The source may go first; the copies own their own blocks.
        drop(a);
        assert_eq!(fields(b.as_ref()), FIELDS[5]);
        drop(b);
        assert_eq!(fields(c.as_ref()), FIELDS[5]);
        drop(HeapBox::<ComponentDescriptor>::new_uninit().unwrap());
    }

    #[test]
    fn heap_array_handles_stay_in_bounds() {
        let mut a = alloc_cell_array_zeroed::<ComponentDescriptor>(FIELDS.len()).unwrap();
        assert!(a.as_handles().iter().all(|h| fields(h) == [0; 5]));
        {
            let mut handles = a.as_handles_mut();
            for (i, f) in FIELDS.into_iter().enumerate() {
                fill(&mut handles.get_mut(i).unwrap(), f);
            }
            assert!(handles.get_mut(FIELDS.len()).is_none());
        }
        let read: Vec<[i32; 5]> = a.as_handles().iter().map(fields).collect();
        assert_eq!(read, FIELDS);
        assert!(a.as_handles().get(FIELDS.len()).is_none());
        let b = a.clone();
        drop(a);
        assert_eq!(
            b.as_handles().iter().map(fields).collect::<Vec<_>>(),
            FIELDS
        );
        drop(alloc_cell_array_zeroed::<ComponentDescriptor>(0).unwrap());
    }

    #[test]
    fn values_and_owners_move_across_threads() {
        let v = d(FIELDS[2]);
        let b = HeapBox::<ComponentDescriptor>::new_zeroed().unwrap();
        let (v, b) = thread::spawn(move || {
            let mut b = b;
            b.as_mut().set(v);
            (v, b)
        })
        .join()
        .unwrap();
        assert_eq!(fields(b.as_ref()), fields(v.as_ref()));
        // Freed on another thread than the one that allocated it.
        thread::spawn(move || drop(b)).join().unwrap();
    }

    #[test]
    fn shared_handles_read_concurrently() {
        let v = d(FIELDS[1]);
        let boxed = HeapBox::<ComponentDescriptor>::new_zeroed().unwrap();
        let (vr, br) = (v.as_ref(), boxed.as_ref());
        thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(move || {
                    for _ in 0..1000 {
                        assert_eq!(fields(vr), FIELDS[1]);
                        assert_eq!(br.get(), ComponentDescriptor::new());
                    }
                });
            }
        });
    }

    #[test]
    fn exclusive_handles_write_on_other_threads() {
        let mut values = [ComponentDescriptor::new(); 4];
        let mut boxed = HeapBox::<ComponentDescriptor>::new_zeroed().unwrap();
        thread::scope(|s| {
            for (i, v) in values.iter_mut().enumerate() {
                let mut m = v.as_mut();
                s.spawn(move || m.set_plane(i as i32));
            }
            let mut m = boxed.as_mut();
            s.spawn(move || m.set_depth(i32::MIN));
        });
        let planes: Vec<i32> = values.iter().map(|v| v.as_ref().plane()).collect();
        assert_eq!(planes, [0, 1, 2, 3]);
        assert_eq!(fields(boxed.as_ref()), [0, 0, 0, 0, i32::MIN]);
    }

    #[test]
    fn a_shared_exclusive_handle_only_reads() {
        let mut v = d(FIELDS[2]);
        let m = v.as_mut();
        let m = &m;
        thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(move || assert_eq!(fields(m.as_ref()), FIELDS[2]));
            }
        });
    }

    #[test]
    fn heap_array_elements_written_from_separate_threads() {
        let mut a = alloc_cell_array_zeroed::<ComponentDescriptor>(4).unwrap();
        let mut handles = a.as_handles_mut();
        thread::scope(|s| {
            // Each element is reached through its own exclusive handle.
            for (m, f) in handles.iter_mut().zip(FIELDS) {
                let mut m = m;
                s.spawn(move || fill(&mut m, f));
            }
        });
        let read: Vec<[i32; 5]> = a.as_handles().iter().map(fields).collect();
        assert_eq!(read, FIELDS[..4]);
    }
}
