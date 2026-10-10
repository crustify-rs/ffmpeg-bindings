//! UB tests for `libavutil/channel_layout.h`'s [`Channel`], through the safe
//! API only: as a value, its Ambisonic conversions at every boundary, and as
//! the plain element of `av_malloc`-family arrays.

use std::thread;

use ffmpeg::libavutil::channel_layout::Channel;
use ffmpeg::libavutil::ffi;
use ffmpeg::libavutil::mem::{HeapVec, alloc_array, alloc_array_from_fn, alloc_array_zeroed};

/// Named ids, the Ambisonic bounds and their neighbours, user ids and the
/// `int` extremes.
fn channel_sample() -> Vec<Channel> {
    [
        i32::MIN,
        i32::MIN + 1,
        -2,
        -1,
        0,
        17,
        18,
        62,
        63,
        64,
        0x200,
        0x300,
        0x3ff,
        0x400,
        0x7ff,
        0x800,
        i32::MAX - 1,
        i32::MAX,
    ]
    .map(|raw| Channel::from_raw(ffi::AVChannel(raw)))
    .into()
}

#[test]
fn channel_debug_order_and_ambisonic_index_accept_every_value() {
    let values = channel_sample();
    for &ch in &values {
        assert!(!format!("{ch:?}").is_empty());
        let raw: ffi::AVChannel = ch.into();
        assert_eq!(Channel::from(raw), ch);
        if let Some(acn) = ch.ambisonic_index() {
            assert_eq!(Channel::ambisonic(acn), Some(ch));
        }
        assert!(values.iter().all(|&o| o.cmp(&ch).is_eq() == (o == ch)));
    }
    let mut sorted = values.clone();
    sorted.sort();
    assert_eq!(sorted, values);
}

#[test]
fn ambisonic_accepts_every_acn() {
    let mut some = 0;
    for acn in 0..=u16::MAX {
        if let Some(ch) = Channel::ambisonic(acn) {
            assert_eq!(ch.ambisonic_index(), Some(acn));
            some += 1;
        }
    }
    assert_eq!(some, 1024);
}

#[test]
fn channel_arrays_fill_clone_and_free() {
    let values = channel_sample();
    let mut a = alloc_array_zeroed::<Channel>(values.len()).unwrap();
    assert!(a.as_slice().iter().all(|&ch| ch == Channel::FRONT_LEFT));
    a.as_mut_slice().copy_from_slice(&values);
    assert!(a.as_slice().get(values.len()).is_none());
    let mut u = alloc_array::<Channel>(values.len()).unwrap();
    for (slot, &ch) in u.as_mut_slice().iter_mut().zip(&values) {
        slot.write(ch);
    }
    drop(u);
    let f = alloc_array_from_fn(values.len(), |i| values[i]).unwrap();
    let b = a.clone();
    a.as_mut_slice().fill(Channel::NONE);
    a.as_mut_slice().sort_unstable_by(|x, y| y.cmp(x));
    drop(a);
    assert_eq!(b.as_slice(), &values[..]);
    assert_eq!(f.as_slice(), &values[..]);
    drop(alloc_array_zeroed::<Channel>(0).unwrap());
    drop(HeapVec::<Channel>::from_slice(&[]).unwrap());
}

#[test]
fn channel_arrays_cross_and_are_shared_between_threads() {
    let values = channel_sample();
    let a = HeapVec::from_slice(&values).unwrap();
    // Shared reads from several threads at once.
    thread::scope(|s| {
        for _ in 0..4 {
            s.spawn(|| {
                assert_eq!(a.as_slice(), &values[..]);
                assert!(a.as_slice().is_sorted());
            });
        }
    });
    // Moved to another thread, written and freed there.
    let back = thread::spawn(move || {
        let mut a = a;
        a.as_mut_slice().reverse();
        a.as_slice().to_vec()
    })
    .join()
    .unwrap();
    assert_eq!(back.iter().rev().copied().collect::<Vec<_>>(), values);
}

/// `MatrixEncoding`: every named and unnamed value through each conversion,
/// `av_malloc`-family arrays of it, and moving it across threads.
mod matrix_encoding {
    use std::thread;

    use ffmpeg::libavutil::channel_layout::MatrixEncoding;
    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::mem::{alloc_array_from_fn, alloc_array_zeroed};

    /// Raw values at every edge: each named one, the `NB` sentinel, and the
    /// `unsigned int` limits a C `int` field's negative values map onto.
    fn raws() -> impl Iterator<Item = u32> {
        (0..=8).chain([i32::MAX as u32, i32::MIN as u32, u32::MAX - 1, u32::MAX])
    }

    #[test]
    fn every_raw_value_converts_indexes_and_formats() {
        for raw in raws() {
            let e = MatrixEncoding::from_raw(ffi::AVMatrixEncoding(raw));
            assert_eq!(e.into_raw().0, raw);
            let back: ffi::AVMatrixEncoding = e.into();
            assert_eq!(MatrixEncoding::from(back), e);
            match e.index() {
                Some(i) => assert!(i < MatrixEncoding::COUNT && i as u32 == raw),
                None => assert!(raw as usize >= MatrixEncoding::COUNT),
            }
            assert!(!format!("{e:?}").is_empty());
        }
    }

    #[test]
    fn heap_arrays_read_any_stored_value() {
        let named = alloc_array_from_fn(MatrixEncoding::COUNT, |i| {
            MatrixEncoding::from_raw(ffi::AVMatrixEncoding(i as u32))
        })
        .unwrap();
        let idx: Vec<Option<usize>> = named.as_slice().iter().map(|e| e.index()).collect();
        assert_eq!(
            idx,
            (0..MatrixEncoding::COUNT).map(Some).collect::<Vec<_>>()
        );
        let mut zeroed = alloc_array_zeroed::<MatrixEncoding>(3).unwrap();
        assert!(zeroed.as_slice().iter().all(|&e| e == MatrixEncoding::NONE));
        zeroed.as_mut_slice()[2] = MatrixEncoding::from_raw(ffi::AVMatrixEncoding(u32::MAX));
        let copy = zeroed.clone();
        drop(zeroed);
        assert_eq!(copy.as_slice()[2].index(), None);
    }

    #[test]
    fn values_cross_threads() {
        let all: Vec<MatrixEncoding> = raws()
            .map(|r| MatrixEncoding::from_raw(ffi::AVMatrixEncoding(r)))
            .collect();
        let shared = &all;
        thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(move || {
                    for e in shared {
                        let _ = (e.index(), format!("{e:?}"));
                    }
                });
            }
        });
        let e = thread::spawn(|| MatrixEncoding::DPLIIZ).join().unwrap();
        assert_eq!(e, MatrixEncoding::DPLIIZ);
    }
}

/// [`ChannelOrder`] through the safe API only: as a value, and as the plain element
/// of `av_malloc`-family arrays, the storage a C array of `enum AVChannelOrder`
/// lives in (formats wave 0, batch 1).
mod channel_order {
    use std::thread;

    use ffmpeg::libavutil::channel_layout::ChannelOrder;
    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::mem::{HeapVec, alloc_array, alloc_array_from_fn, alloc_array_zeroed};

    /// Every value C names, the values just past them and the `unsigned`
    /// extremes, which C may store in `AVChannelLayout.order`.
    fn sample() -> Vec<ChannelOrder> {
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 1 << 31, u32::MAX - 1, u32::MAX]
            .map(|raw| ChannelOrder::from_raw(ffi::AVChannelOrder(raw)))
            .into()
    }

    #[test]
    fn debug_default_and_conversions_accept_every_value() {
        for t in sample() {
            assert!(!format!("{t:?}").is_empty());
            let raw: ffi::AVChannelOrder = t.into();
            assert_eq!(ChannelOrder::from(raw), t);
            assert_eq!(ChannelOrder::from_raw(t.into_raw()), t);
        }
        assert_eq!(ChannelOrder::default(), ChannelOrder::UNSPEC);
    }

    #[test]
    fn zeroed_array_reads_default_and_takes_every_value() {
        let values = sample();
        let mut a = alloc_array_zeroed::<ChannelOrder>(values.len()).unwrap();
        assert!(a.as_slice().iter().all(|&t| t == ChannelOrder::default()));
        a.as_mut_slice().copy_from_slice(&values);
        assert_eq!(a.as_slice(), &values[..]);
        assert!(a.as_slice().get(values.len()).is_none());
    }

    #[test]
    fn uninit_and_from_fn_arrays_fill_end_to_end() {
        let values = sample();
        let mut u = alloc_array::<ChannelOrder>(values.len()).unwrap();
        for (slot, &t) in u.as_mut_slice().iter_mut().zip(&values) {
            slot.write(t);
        }
        drop(u);
        let f = alloc_array_from_fn(values.len(), |i| values[i]).unwrap();
        assert_eq!(f.as_slice(), &values[..]);
    }

    #[test]
    fn clone_is_independent_of_its_source() {
        let values = sample();
        let mut a = HeapVec::from_slice(&values).unwrap();
        let b = a.clone();
        a.as_mut_slice().fill(ChannelOrder::AMBISONIC);
        drop(a);
        assert_eq!(b.as_slice(), &values[..]);
    }

    #[test]
    fn empty_arrays_allocate_and_free() {
        drop(alloc_array_zeroed::<ChannelOrder>(0).unwrap());
        drop(HeapVec::<ChannelOrder>::from_slice(&[]).unwrap());
    }

    #[test]
    fn arrays_cross_and_are_shared_between_threads() {
        let values = sample();
        let a = HeapVec::from_slice(&values).unwrap();
        // Shared reads from several threads at once.
        thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(|| assert_eq!(a.as_slice(), &values[..]));
            }
        });
        // Moved to another thread, written and freed there.
        let back = thread::spawn(move || {
            let mut a = a;
            a.as_mut_slice().reverse();
            a.as_slice().to_vec()
        })
        .join()
        .unwrap();
        assert_eq!(back.iter().rev().copied().collect::<Vec<_>>(), values);
    }
}

/// `ChannelCustom`: inline values, `av_malloc`-family owners and arrays of
/// them (the shape of a custom layout's map), names of every length from
/// exactly sized heap strings, unterminated name fields, user pointers that
/// dangle, every handle form, copies outliving their source, and the
/// asserted `Send` / `Sync`.
mod channel_custom {
    use std::ffi::{CStr, CString};
    use std::thread;

    use ffmpeg::libavutil::channel_layout::{
        Channel, ChannelCustom, ChannelCustomMut, ChannelCustomRef, ChannelName, NameTooLong,
    };
    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::mem::{HeapBox, alloc_cell_array_zeroed};

    /// A name of `len` bytes in an exactly sized heap allocation, so a read
    /// past its NUL is an ASan report.
    fn heap_name(len: usize) -> CString {
        CString::new((0..len).map(|i| b'a' + (i % 26) as u8).collect::<Vec<_>>()).unwrap()
    }

    /// A user pointer that addresses nothing: storing and reading it back
    /// must never dereference it.
    fn dangling(addr: usize) -> *mut core::ffi::c_void {
        core::ptr::without_provenance_mut(addr)
    }

    fn fields(c: ChannelCustomRef<'_>) -> (Channel, ChannelName, usize) {
        (c.id(), c.name(), c.opaque().addr())
    }

    fn fill(m: &mut ChannelCustomMut<'_>, id: i32, name: &CStr, opaque: usize) {
        m.set_id(Channel::from_raw(ffi::AVChannel(id)));
        m.set_name_truncated(name);
        m.set_opaque(dangling(opaque));
    }

    #[test]
    fn names_of_every_length_stay_in_the_field() {
        let mut boxed = HeapBox::<ChannelCustom>::new_zeroed().unwrap();
        let mut value = ChannelCustom::default();
        for len in 0..=40 {
            let name = heap_name(len);
            for m in [&mut boxed.as_mut(), &mut value.as_mut()] {
                m.set_opaque(dangling(0xdead));
                m.set_id(Channel::FRONT_LEFT);
                let strict = m.set_name(&name);
                assert_eq!(strict.is_ok(), len <= ChannelName::MAX_LEN);
                if let Err(e) = strict {
                    assert_eq!(e, NameTooLong { len });
                    assert!(!e.to_string().is_empty());
                }
                assert_eq!(m.set_name_truncated(&name), len <= ChannelName::MAX_LEN);
                let read = m.as_ref().name();
                assert_eq!(read.as_bytes(), &name.as_bytes()[..len.min(ChannelName::MAX_LEN)]);
                // The neighbouring fields are untouched.
                assert_eq!(m.as_ref().opaque().addr(), 0xdead);
                assert_eq!(m.as_ref().id(), Channel::FRONT_LEFT);
            }
        }
    }

    #[test]
    fn unterminated_name_fields_read_within_the_field() {
        let raw = ffi::AVChannelCustom {
            id: ffi::AVChannel(i32::MIN),
            name: [b'z' as core::ffi::c_char; 16],
            opaque: dangling(usize::MAX),
        };
        let mut boxed = HeapBox::<ChannelCustom>::new_zeroed().unwrap();
        boxed.as_mut().set(ChannelCustom::from_raw(raw));
        let name = boxed.as_ref().name();
        assert_eq!(name.as_bytes(), [b'z'; 16]);
        assert_eq!(name.as_c_str().to_bytes().len(), 16);
        assert_eq!(name.to_str(), Ok("zzzzzzzzzzzzzzzz"));
        assert!(!format!("{:?}", boxed.as_ref()).is_empty());
        // A name written over it is terminated again.
        assert!(boxed.as_mut().set_name_truncated(c"ok"));
        assert_eq!(boxed.as_ref().name(), c"ok");
    }

    #[test]
    fn user_pointers_are_stored_never_followed() {
        let mut v = ChannelCustom::new(Channel::BACK_LEFT);
        let gone = {
            let mut local = 7u32;
            let p = (&raw mut local).cast();
            v.as_mut().set_opaque(p);
            p
        };
        // The local is gone; its address is still only a value.
        assert_eq!(v.as_ref().opaque(), gone);
        for addr in [0, 1, 3, usize::MAX, usize::MAX / 2] {
            v.as_mut().set_opaque(dangling(addr));
            let copy = v.as_ref().get();
            assert_eq!(copy.as_ref().opaque().addr(), addr);
            assert!(!format!("{copy:?}").is_empty());
        }
    }

    #[test]
    fn shared_handles_copy_and_coexist() {
        let mut v = ChannelCustom::new(Channel::LOW_FREQUENCY);
        v.as_mut().set_name(c"sub").unwrap();
        let a = v.as_ref();
        let b = a;
        let c = v.as_ref();
        assert_eq!(fields(a), fields(b));
        assert_eq!(c.name(), c"sub");
    }

    fn reset(mut m: ChannelCustomMut<'_>) {
        m.set(ChannelCustom::default());
    }

    #[test]
    fn exclusive_handle_reborrows_without_aliasing() {
        let mut v = ChannelCustom::new(Channel::SIDE_LEFT);
        let mut m = v.as_mut();
        fill(&mut m, 9, c"side", 0x40);
        reset(m.as_mut());
        assert_eq!(fields(m.as_ref()), (Channel::UNKNOWN, ChannelName::default(), 0));
        m.set_name(c"again").unwrap();
        let shared = m.as_ref();
        assert_eq!(shared.name(), c"again");
        m.set_id(Channel::NONE);
        assert_eq!(v.as_ref().id(), Channel::NONE);
    }

    #[test]
    fn copies_outlive_and_are_independent_of_their_source() {
        let (copy, name) = {
            let mut boxed = HeapBox::<ChannelCustom>::new_zeroed().unwrap();
            fill(&mut boxed.as_mut(), 4, c"rear left", 0x10);
            let copy = boxed.as_ref().get();
            let name = boxed.as_ref().name();
            boxed.as_mut().set_name(c"changed").unwrap();
            (copy, name)
            // `boxed` is freed here; `copy` and `name` hold their own bytes.
        };
        assert_eq!(copy.as_ref().name(), c"rear left");
        assert_eq!(name, c"rear left");
        assert_eq!(name.as_c_str(), c"rear left");
        let mut b = copy;
        b.as_mut().set_id(Channel::BACK_RIGHT);
        assert_eq!(copy.as_ref().id(), Channel::BACK_LEFT);
    }

    #[test]
    fn heap_box_owner_lives_reads_writes_clones_and_frees() {
        let mut a = HeapBox::<ChannelCustom>::new_zeroed().unwrap();
        assert_eq!(a.as_ref().id(), Channel::FRONT_LEFT);
        fill(&mut a.as_mut(), 63, c"user", 0x99);
        let b = a.clone();
        let c = b.try_clone().unwrap();
        drop(a);
        assert_eq!(fields(b.as_ref()), fields(c.as_ref()));
        drop(b);
        assert_eq!(c.as_ref().name(), c"user");
        drop(HeapBox::<ChannelCustom>::new_uninit().unwrap());
    }

    #[test]
    fn heap_array_handles_stay_in_bounds() {
        const N: usize = 5;
        let mut a = alloc_cell_array_zeroed::<ChannelCustom>(N).unwrap();
        {
            let mut handles = a.as_handles_mut();
            for i in 0..N {
                let name = heap_name(i * 4);
                fill(&mut handles.get_mut(i).unwrap(), i as i32, &name, i);
            }
            assert!(handles.get_mut(N).is_none());
        }
        for (i, h) in a.as_handles().iter().enumerate() {
            assert_eq!(h.id().into_raw().0, i as i32);
            assert_eq!(h.name().as_bytes().len(), (i * 4).min(ChannelName::MAX_LEN));
            assert_eq!(h.opaque().addr(), i);
        }
        assert!(a.as_handles().get(N).is_none());
        let b = a.clone();
        drop(a);
        assert_eq!(b.as_handles().get(N - 1).unwrap().name().as_bytes().len(), 15);
        drop(alloc_cell_array_zeroed::<ChannelCustom>(0).unwrap());
    }

    #[test]
    fn values_and_owners_move_across_threads() {
        let mut v = ChannelCustom::new(Channel::TOP_CENTER);
        v.as_mut().set_opaque(dangling(0x1000));
        let b = HeapBox::<ChannelCustom>::new_zeroed().unwrap();
        let (v, b) = thread::spawn(move || {
            let mut b = b;
            b.as_mut().set(v);
            b.as_mut().set_name(c"moved").unwrap();
            (v, b)
        })
        .join()
        .unwrap();
        assert_eq!(b.as_ref().id(), v.as_ref().id());
        assert_eq!(b.as_ref().name(), c"moved");
        thread::spawn(move || drop(b)).join().unwrap();
    }

    #[test]
    fn shared_handles_read_concurrently() {
        let mut v = ChannelCustom::new(Channel::WIDE_LEFT);
        v.as_mut().set_name(c"wide").unwrap();
        let boxed = HeapBox::<ChannelCustom>::new_zeroed().unwrap();
        let (vr, br) = (v.as_ref(), boxed.as_ref());
        thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(move || {
                    for _ in 0..1000 {
                        assert_eq!(vr.name(), c"wide");
                        // All-zero: `FRONT_LEFT`, unnamed, NULL.
                        assert_eq!(br.get(), ChannelCustom::new(Channel::FRONT_LEFT));
                    }
                });
            }
        });
    }

    #[test]
    fn exclusive_handles_write_on_other_threads() {
        let mut values = [ChannelCustom::default(); 4];
        let mut boxed = HeapBox::<ChannelCustom>::new_zeroed().unwrap();
        thread::scope(|s| {
            for (i, v) in values.iter_mut().enumerate() {
                let mut m = v.as_mut();
                s.spawn(move || fill(&mut m, i as i32, c"t", i));
            }
            let mut m = boxed.as_mut();
            s.spawn(move || m.set_name(c"boxed").unwrap());
        });
        let ids: Vec<i32> = values.iter().map(|v| v.as_ref().id().into_raw().0).collect();
        assert_eq!(ids, [0, 1, 2, 3]);
        assert_eq!(boxed.as_ref().name(), c"boxed");
    }

    #[test]
    fn a_shared_exclusive_handle_only_reads() {
        let mut v = ChannelCustom::new(Channel::BINAURAL_LEFT);
        let m = v.as_mut();
        let m = &m;
        thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(move || assert_eq!(m.as_ref().id(), Channel::BINAURAL_LEFT));
            }
        });
    }

    #[test]
    fn heap_array_elements_written_from_separate_threads() {
        let mut a = alloc_cell_array_zeroed::<ChannelCustom>(4).unwrap();
        let mut handles = a.as_handles_mut();
        thread::scope(|s| {
            for (i, m) in handles.iter_mut().enumerate() {
                let mut m = m;
                s.spawn(move || fill(&mut m, i as i32, &heap_name(i * 5), i));
            }
        });
        let names: Vec<usize> = a.as_handles().iter().map(|h| h.name().as_bytes().len()).collect();
        assert_eq!(names, [0, 5, 10, 15]);
    }
}
