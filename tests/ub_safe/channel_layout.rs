//! UB tests for `libavutil/channel_layout.h`'s [`Channel`], through the safe
//! API only: as a value, its Ambisonic conversions at every boundary, and as
//! the plain element of `av_malloc`-family arrays.
#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    trivial_casts,
    reason = "test inputs are chosen within range, and the casts mirror the C conversions under comparison"
)]
#![expect(
    clippy::assert_is_empty,
    let_underscore_drop,
    reason = "assertions keep the form of the C behaviour they check"
)]

use std::thread;

use ffmpeg_bindings::libavutil::libavutil::channel_layout::Channel;
use ffmpeg_bindings::libavutil::libavutil::ffi;
use ffmpeg_bindings::libavutil::libavutil::mem::{
    HeapVec, alloc_array, alloc_array_from_fn, alloc_array_zeroed,
};

/// Which descriptions C's parser may be given under BSan.
#[path = "../common/channel_layout.rs"]
mod parser_inputs;

/// The `BSan` input filter keeps exactly the descriptions C matches by name or
/// accepts as a channel list, and skips every other spelling.
#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn parser_input_filter_classifies_cs_paths() {
    use parser_inputs::parsed_before_strtol;
    for s in [
        c"mono",
        c"5.1(side)",
        c"7.1(wide-side)",
        c"22.2",
        c"FL+FR",
        c"FL@left+FR@right+LFE",
        c"USR63@x+AMBI4@amb",
        c"FL@a+FL@a+FL@b",
        c"USR0@\xc3\xa9+BL",
        c"FL@",
        c"2 channels (FL+FR)",
        c"2 channels (FL+FR)x",
        c"3 channels (FL+FR)",
    ] {
        assert!(parsed_before_strtol(s), "{s:?}");
    }
    for s in [
        c"ambisonic 0",
        c"ambisonic 1+FL@x",
        c"ambisonic ",
        c"0x3",
        c"-1",
        c"3c",
        c"4C",
        c"4 channels",
        c"2 channels ()",
        c"2 channels (FL+NOPE)",
        c"@x",
        c"+",
        c"",
        c"FL+NOPE",
        c"FL + FR",
        c"'FL'+FR",
        c"FL\\+FR",
        c"Stereo",
        c"\xff",
    ] {
        assert!(!parsed_before_strtol(s), "{s:?}");
    }
}

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
        0x3FF,
        0x400,
        0x7FF,
        0x800,
        i32::MAX - 1,
        i32::MAX,
    ]
    .map(Channel::from_raw)
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
#[cfg_attr(miri, ignore = "calls C")]
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
#[cfg_attr(miri, ignore = "calls C")]
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

    use ffmpeg_bindings::libavutil::libavutil::channel_layout::MatrixEncoding;
    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::mem::{alloc_array_from_fn, alloc_array_zeroed};

    /// Raw values at every edge: each named one, the `NB` sentinel, and the
    /// `unsigned int` limits a C `int` field's negative values map onto.
    fn raws() -> impl Iterator<Item = u32> {
        (0..=8).chain([i32::MAX as u32, i32::MIN as u32, u32::MAX - 1, u32::MAX])
    }

    #[test]
    fn every_raw_value_converts_indexes_and_formats() {
        for raw in raws() {
            let e = MatrixEncoding::from_raw(raw);
            assert_eq!(e.into_raw(), raw);
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
    #[cfg_attr(miri, ignore = "calls C")]
    fn heap_arrays_read_any_stored_value() {
        let named = alloc_array_from_fn(MatrixEncoding::COUNT, |i| {
            MatrixEncoding::from_raw(i as u32)
        })
        .unwrap();
        let idx: Vec<Option<usize>> = named.as_slice().iter().map(|e| e.index()).collect();
        assert_eq!(
            idx,
            (0..MatrixEncoding::COUNT).map(Some).collect::<Vec<_>>()
        );
        let mut zeroed = alloc_array_zeroed::<MatrixEncoding>(3).unwrap();
        assert!(zeroed.as_slice().iter().all(|&e| e == MatrixEncoding::NONE));
        zeroed.as_mut_slice()[2] = MatrixEncoding::from_raw(u32::MAX);
        let copy = zeroed.clone();
        drop(zeroed);
        assert_eq!(copy.as_slice()[2].index(), None);
    }

    #[test]
    fn values_cross_threads() {
        let all: Vec<MatrixEncoding> = raws().map(MatrixEncoding::from_raw).collect();
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

    use ffmpeg_bindings::libavutil::libavutil::channel_layout::ChannelOrder;
    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::mem::{
        HeapVec, alloc_array, alloc_array_from_fn, alloc_array_zeroed,
    };

    /// Every value C names, the values just past them and the `unsigned`
    /// extremes, which C may store in `AVChannelLayout.order`.
    fn sample() -> Vec<ChannelOrder> {
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 1 << 31, u32::MAX - 1, u32::MAX]
            .map(ChannelOrder::from_raw)
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
    #[cfg_attr(miri, ignore = "calls C")]
    fn zeroed_array_reads_default_and_takes_every_value() {
        let values = sample();
        let mut a = alloc_array_zeroed::<ChannelOrder>(values.len()).unwrap();
        assert!(a.as_slice().iter().all(|&t| t == ChannelOrder::default()));
        a.as_mut_slice().copy_from_slice(&values);
        assert_eq!(a.as_slice(), &values[..]);
        assert!(a.as_slice().get(values.len()).is_none());
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
    fn clone_is_independent_of_its_source() {
        let values = sample();
        let mut a = HeapVec::from_slice(&values).unwrap();
        let b = a.clone();
        a.as_mut_slice().fill(ChannelOrder::AMBISONIC);
        drop(a);
        assert_eq!(b.as_slice(), &values[..]);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn empty_arrays_allocate_and_free() {
        drop(alloc_array_zeroed::<ChannelOrder>(0).unwrap());
        drop(HeapVec::<ChannelOrder>::from_slice(&[]).unwrap());
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
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

    use ffmpeg_bindings::libavutil::libavutil::channel_layout::{
        Channel, ChannelCustom, ChannelCustomMut, ChannelCustomRef, ChannelName, NameTooLong,
    };
    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::mem::{HeapBox, alloc_cell_array_zeroed};

    /// A name of `len` bytes in an exactly sized heap allocation, so a read
    /// past its NUL is an `ASan` report.
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
        m.set_id(Channel::from_raw(id));
        m.set_name_truncated(name);
        m.set_opaque(dangling(opaque));
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn names_of_every_length_stay_in_the_field() {
        let mut boxed = HeapBox::<ChannelCustom>::new_zeroed().unwrap();
        let mut value = ChannelCustom::default();
        for len in 0..=40 {
            let name = heap_name(len);
            for m in [&mut boxed.as_mut(), &mut value.as_mut()] {
                m.set_opaque(dangling(0xDEAD));
                m.set_id(Channel::FRONT_LEFT);
                let strict = m.set_name(&name);
                assert_eq!(strict.is_ok(), len <= ChannelName::MAX_LEN);
                if let Err(e) = strict {
                    assert_eq!(e, NameTooLong { len });
                    assert!(!e.to_string().is_empty());
                }
                assert_eq!(m.set_name_truncated(&name), len <= ChannelName::MAX_LEN);
                let read = m.as_ref().name();
                assert_eq!(
                    read.as_bytes(),
                    &name.as_bytes()[..len.min(ChannelName::MAX_LEN)]
                );
                // The neighbouring fields are untouched.
                assert_eq!(m.as_ref().opaque().addr(), 0xDEAD);
                assert_eq!(m.as_ref().id(), Channel::FRONT_LEFT);
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn unterminated_name_fields_read_within_the_field() {
        let raw = ffi::AVChannelCustom {
            id: i32::MIN,
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
        assert_eq!(
            fields(m.as_ref()),
            (Channel::UNKNOWN, ChannelName::default(), 0)
        );
        m.set_name(c"again").unwrap();
        let shared = m.as_ref();
        assert_eq!(shared.name(), c"again");
        m.set_id(Channel::NONE);
        assert_eq!(v.as_ref().id(), Channel::NONE);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
            assert_eq!(h.id().into_raw(), i as i32);
            assert_eq!(h.name().as_bytes().len(), (i * 4).min(ChannelName::MAX_LEN));
            assert_eq!(h.opaque().addr(), i);
        }
        assert!(a.as_handles().get(N).is_none());
        let b = a.clone();
        drop(a);
        assert_eq!(
            b.as_handles().get(N - 1).unwrap().name().as_bytes().len(),
            15
        );
        drop(alloc_cell_array_zeroed::<ChannelCustom>(0).unwrap());
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
        let ids: Vec<i32> = values.iter().map(|v| v.as_ref().id().into_raw()).collect();
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
    #[cfg_attr(miri, ignore = "calls C")]
    fn heap_array_elements_written_from_separate_threads() {
        let mut a = alloc_cell_array_zeroed::<ChannelCustom>(4).unwrap();
        let mut handles = a.as_handles_mut();
        thread::scope(|s| {
            for (i, m) in handles.iter_mut().enumerate() {
                let mut m = m;
                s.spawn(move || fill(&mut m, i as i32, &heap_name(i * 5), i));
            }
        });
        let names: Vec<usize> = a
            .as_handles()
            .iter()
            .map(|h| h.name().as_bytes().len())
            .collect();
        assert_eq!(names, [0, 5, 10, 15]);
    }
}

/// `Channel::write_name`, `write_description`, `name`, `description` and
/// `from_name`: every id class and the `int` extremes written into exactly
/// sized heap buffers of every length (so a write past the end is an `ASan`
/// report), names of every shape in exactly sized heap strings, and the
/// calls on several threads at once.
mod channel_names {
    use std::ffi::CString;
    use std::thread;

    use ffmpeg_bindings::libavutil::libavutil::channel_layout::Channel;

    fn ids() -> Vec<Channel> {
        (-4..70)
            .chain(1020..1030)
            .chain(2040..2060)
            .chain([i32::MAX, i32::MIN, i32::MIN + 1])
            .map(Channel::from_raw)
            .collect()
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn writers_stay_inside_every_buffer_length() {
        type Write = fn(Channel, &mut [u8]) -> Result<usize, i32>;
        for write in [Channel::write_name as Write, Channel::write_description] {
            for ch in ids() {
                for len in 0..32 {
                    // An exactly sized heap block: one byte past it is
                    // outside the allocation.
                    let mut buf = vec![0xAAu8; len].into_boxed_slice();
                    let need = write(ch, &mut buf).unwrap();
                    assert!(need >= 1);
                    if len > 0 {
                        let end = (need - 1).min(len - 1);
                        assert_eq!(buf[end], 0, "{ch:?} {len}");
                        assert!(!buf[..end].contains(&0));
                    }
                }
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn owned_strings_have_the_measured_length() {
        for ch in ids() {
            let name = ch.name();
            assert_eq!(ch.write_name(&mut []), Ok(name.as_bytes_with_nul().len()));
            let desc = ch.description();
            assert_eq!(
                ch.write_description(&mut []),
                Ok(desc.as_bytes_with_nul().len())
            );
            // The owned strings outlive the channel value and later calls.
            let _ = Channel::from_name(&name);
            assert!(!name.as_bytes().is_empty() && !desc.as_bytes().is_empty());
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn lookups_read_names_of_every_shape_in_bounds() {
        let mut names: Vec<Vec<u8>> = [
            "", "A", "AMBI", "USR", "AMBI-", "USR-", "USR0x", "AMBI0x", "\u{ff}",
        ]
        .map(|s| s.as_bytes().to_vec())
        .into();
        for len in [1, 3, 4, 20, 40, 4096] {
            names.push([b"AMBI".as_slice(), &vec![b'9'; len]].concat());
            names.push([b"USR".as_slice(), &vec![b'7'; len]].concat());
            names.push(vec![b'F'; len]);
        }
        for ch in ids() {
            names.push(ch.name().into_bytes());
        }
        for bytes in names {
            let name = CString::new(bytes).unwrap();
            if let Some(ch) = Channel::from_name(&name) {
                assert_ne!(ch, Channel::NONE);
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn calls_run_on_many_threads() {
        let ids = ids();
        let expect: Vec<_> = ids.iter().map(|c| (c.name(), c.description())).collect();
        thread::scope(|s| {
            for t in 0..8 {
                let (ids, expect) = (&ids, &expect);
                s.spawn(move || {
                    let mut buf = [0u8; 8];
                    for (ch, e) in ids.iter().zip(expect).skip(t) {
                        assert_eq!(&(ch.name(), ch.description()), e);
                        let _ = ch.write_name(&mut buf);
                        let _ = Channel::from_name(&e.0);
                    }
                });
            }
        });
    }
}

/// `Channel::append_name` / `append_description` into every print buffer
/// owner: growing, automatic, count-only, and exactly sized caller buffers of
/// every small length (so a byte past the end is an `ASan` report), handles
/// reborrowed from owners and from other handles, and buffers on many
/// threads at once.
mod name_appenders {
    use std::thread;

    use ffmpeg_bindings::libavutil::libavutil::bprint::{BPrint, BPrintMut};
    use ffmpeg_bindings::libavutil::libavutil::channel_layout::Channel;

    use super::channel_sample;

    fn append_both(ch: Channel, mut buf: BPrintMut<'_>) {
        ch.append_name(buf.as_mut());
        buf.append_bytes(b"/");
        ch.append_description(buf);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn every_owner_takes_every_channel() {
        for size_max in [
            BPrint::SIZE_UNLIMITED,
            BPrint::SIZE_AUTOMATIC,
            BPrint::SIZE_COUNT_ONLY,
            7,
        ] {
            let mut buf = BPrint::new(0, size_max).unwrap();
            for ch in channel_sample() {
                append_both(ch, buf.as_mut());
                let r = buf.as_ref();
                assert!(r.as_bytes().len() <= r.len() as usize);
            }
            if size_max == BPrint::SIZE_UNLIMITED {
                assert!(buf.as_ref().is_complete());
                let s = BPrint::into_string(buf).unwrap();
                assert!(!s.as_bytes().is_empty());
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn exactly_sized_caller_buffers_truncate_in_bounds() {
        for len in 1..24 {
            for ch in channel_sample() {
                let mut data = vec![0xAAu8; len].into_boxed_slice();
                let mut buf = BPrint::for_buffer(&mut data).unwrap();
                append_both(ch, buf.as_mut());
                let stored = buf.as_ref().as_bytes().len();
                assert!(stored < len);
                drop(buf);
                assert_eq!(data[stored], 0);
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn buffers_on_many_threads_at_once() {
        thread::scope(|s| {
            for t in 0..4 {
                s.spawn(move || {
                    let mut buf = BPrint::new(0, BPrint::SIZE_UNLIMITED).unwrap();
                    for ch in channel_sample().into_iter().skip(t) {
                        append_both(ch, buf.as_mut());
                        if buf.as_ref().len() > 512 {
                            buf.as_mut().clear();
                        }
                    }
                });
            }
        });
    }
}

/// `ChannelLayout` through its owner and handles: every C initialiser on
/// edge inputs, the tagged union read in every order, the custom map's
/// element handles at its bounds, random sequences of every setter (each map
/// freed exactly once, none leaked), C-left states reached only through the
/// safe API, and owners, views and handles across threads.
mod layout {
    use core::ffi::c_void;
    use std::ffi::CString;
    use std::thread;

    use ffmpeg_bindings::libavutil::libavutil::channel_layout::{
        Channel, ChannelCustom, ChannelLayout, ChannelLayoutRef, ChannelLayoutVal, ChannelOrder,
        LayoutDetails,
    };
    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::mem::{HeapVec, alloc_cell_array_zeroed};

    use super::parser_inputs::{CHANNEL_BITS, LAYOUT_MASKS, STANDARD_LAYOUTS, allowed, under_bsan};

    /// Descriptions C parses, fails to parse, or stops short in: standard
    /// names, channel lists with and without names, masks, counts,
    /// ambisonic orders at the `int` bound, and malformed spellings.
    const DESCRIPTIONS: [&core::ffi::CStr; 42] = [
        c"mono",
        c"stereo",
        c"5.1(side)",
        c"22.2",
        c"FL+FR+LFE",
        c"FL@front+FR@0123456789abcdefghij+USR63@x+AMBI4@amb",
        c"FL@a+FL@a+FL@b",
        c"USR0@\xc3\xa9+BL",
        c"0x3",
        c"0xffffffffffffffff",
        c"-1",
        c"4c",
        c"9c",
        c"4C",
        c"4 channels",
        c"2 channels (FL+FR)",
        c"2 channels (FL+FR)x",
        c"3 channels (FL+FR)",
        c"ambisonic 0",
        c"ambisonic 2+stereo",
        c"ambisonic 1+FL@x+FR",
        c"ambisonic 1+AMBI0",
        c"ambisonic 46339",
        c"ambisonic 46340",
        // The extras at and one below the `int` bound: C refuses the first,
        // and runs out of memory mapping the second.
        c"ambisonic 46339+88047C",
        c"ambisonic 46339+88046C",
        c"ambisonic 1+3C",
        c"ambisonic -1",
        c"ambisonic ",
        c"FL@",
        c"@x",
        c"+",
        c"",
        c"0",
        c"0c",
        c"2147483648C",
        c"FL+NOPE",
        // Orders C narrows from `long` to `int` modulo 2^32: 1 and 2.
        c"ambisonic 4294967297",
        c"ambisonic 0x100000002+FL",
        // A byte other than `+` after the order, a mask `strtoull` cannot
        // hold, and a channel list missing its closing parenthesis.
        c"ambisonic 1x",
        c"18446744073709551616",
        c"2 channels (FL+FR",
    ];

    /// A dangling user pointer nothing may follow.
    fn dangling(addr: usize) -> *mut c_void {
        std::ptr::without_provenance_mut(addr)
    }

    /// Reads everything the getters reach, every map element included, and
    /// checks the invariant the safe paths keep.
    fn walk(l: ChannelLayoutRef<'_>) -> usize {
        let count = l.channel_count();
        assert!(!format!("{l:?}").is_empty());
        let _ = l.opaque();
        match l.details() {
            LayoutDetails::Custom(map) => {
                assert_eq!(l.order(), ChannelOrder::CUSTOM);
                assert_eq!(map.len(), usize::try_from(count).unwrap());
                assert!(count > 0);
                let mut seen = 0;
                for c in map.iter() {
                    let _ = (c.id(), c.name(), c.opaque(), c.get());
                    seen += 1;
                }
                assert!(map.get(map.len()).is_none());
                seen
            }
            LayoutDetails::Native(mask) => {
                assert_eq!(mask.count_ones() as i32, count);
                0
            }
            LayoutDetails::Ambisonic(mask) => {
                assert!(count > mask.count_ones() as i32);
                0
            }
            LayoutDetails::Unspecified => 0,
            LayoutDetails::Unknown(order) => panic!("no safe path stores {order:?}"),
        }
    }

    /// A map of `n` channels, each named after its index.
    fn map(n: usize) -> HeapVec<ChannelCustom> {
        let mut map = alloc_cell_array_zeroed::<ChannelCustom>(n).unwrap();
        for (i, mut m) in map.as_handles_mut().iter_mut().enumerate() {
            m.set_id(Channel::from_raw(i as i32));
            let _ = m.set_name_truncated(&CString::new(format!("{i:015}")).unwrap());
        }
        map
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn every_initialiser_accepts_edge_inputs() {
        for mask in [0, 1, 0x3, 1 << 63, u64::MAX, 0x8000_0000_0000_0001] {
            match ChannelLayout::from_mask(mask) {
                Ok(l) => {
                    walk(l.as_ref());
                    drop(l.as_ref().try_clone().unwrap());
                }
                Err(e) => assert!(e < 0 && mask == 0),
            }
        }
        // `i32::MAX` channels exceed `av_max_alloc`: C reports ENOMEM.
        for n in [i32::MIN, -1, 0, 1, 2, 64, 1000, i32::MAX] {
            match ChannelLayout::custom(n) {
                Ok(l) => assert_eq!(walk(l.as_ref()), n as usize),
                Err(e) => assert!(e < 0 && (n <= 0 || n == i32::MAX), "{n}"),
            }
        }
        for n in (-3..=70).chain([i32::MIN, i32::MAX]) {
            let l = ChannelLayout::default_for(n);
            assert_eq!(l.as_ref().channel_count(), n);
            if n >= 0 {
                walk(l.as_ref());
            }
        }
        let mut parsed = 0;
        for s in allowed(&DESCRIPTIONS) {
            if let Ok(l) = ChannelLayout::parse(s) {
                walk(l.as_ref());
                let copy = l.as_ref().try_clone().unwrap();
                drop(l);
                walk(copy.as_ref());
                parsed += 1;
            }
        }
        assert!(parsed > if under_bsan() { 8 } else { 15 }, "{parsed}");
        for n in [1, 2, 63, 64, 65, 300] {
            let l = ChannelLayout::from_custom_map(map(n)).unwrap();
            assert_eq!(walk(l.as_ref()), n);
        }
        assert!(ChannelLayout::from_custom_map(HeapVec::empty()).is_err());
        drop(ChannelLayout::unspecified(i32::MIN));
    }

    /// An `ambisonic` order of exactly `INT_MAX`, in any spelling `strtol`
    /// narrows to it and anywhere in the `+`-chain C reads orders, is
    /// refused before C adds one to it (a signed overflow `UBSan` reports at
    /// `channel_layout.c:341`, after which C accepts a layout of no
    /// channels; wave-3 batch-1 review, `ub_safe/parse-ambisonic-int-max-
    /// order-overflow`). Its neighbours still go to C.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn parse_refuses_the_ambisonic_order_c_cannot_add_one_to() {
        for s in [
            c"ambisonic 2147483647",
            c"ambisonic 0x7fffffff",
            c"ambisonic 017777777777",
            c"ambisonic  +2147483647",
            c"ambisonic -2147483649",
            c"ambisonic 6442450943",
            c"ambisonic 2147483647+stereo",
            c"ambisonic 2147483647x",
            c"ambisonic 1+ambisonic 2147483647",
            c"ambisonic 0+ambisonic 1+ambisonic 2147483647",
        ] {
            assert_eq!(ChannelLayout::parse(s).map(drop), Err(-22), "{s:?}");
        }
        if under_bsan() {
            return;
        }
        // C's own refusals: the next order down, and `ambisonic` spellings
        // inside a channel list, which are read as channel names.
        for s in [
            c"ambisonic 2147483646",
            c"ambisonic 1+ambisonic 2147483646",
            c"FL+ambisonic 2147483647",
            c"ambisonic 1+FL+ambisonic 2147483647",
        ] {
            assert_eq!(ChannelLayout::parse(s).map(drop), Err(-22), "{s:?}");
        }
        let l = ChannelLayout::parse(c"ambisonic 4294967297").unwrap();
        let r = l.as_ref();
        assert_eq!((r.ambisonic_order(), r.channel_count()), (Some(1), 4));
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn custom_map_handles_stay_in_bounds() {
        let mut l = ChannelLayout::parse(c"FL@0123456789abcde+FR+LFE@lfe").unwrap();
        {
            let mut map = l.as_mut().into_custom_map_mut().unwrap();
            assert_eq!(map.len(), 3);
            assert!(map.get_mut(3).is_none());
            for (i, mut m) in map.iter_mut().enumerate() {
                m.set_id(Channel::from_raw(i32::MAX - i as i32));
                assert!(!m.set_name_truncated(c"0123456789abcdefghijklmnop"));
                m.set_opaque(dangling(i + 1));
            }
            // The last element's last field ends the C block.
            let last = map.get(2).unwrap();
            assert_eq!(last.opaque(), dangling(3));
        }
        assert_eq!(walk(l.as_ref()), 3);
        let LayoutDetails::Custom(map) = l.as_ref().details() else {
            panic!()
        };
        assert_eq!(map.get(0).unwrap().name().as_bytes(), b"0123456789abcde");
        assert!(
            map.slice(1..)
                .unwrap()
                .iter()
                .all(|c| c.id().into_raw() < i32::MAX)
        );
        assert!(map.slice(..4).is_none());
    }

    /// An `av_dynarray2_add` map may hold spare capacity past the count;
    /// taking it over and freeing it frees the whole block.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn parsed_maps_with_spare_capacity_move_out_and_free() {
        let mut l = ChannelLayout::parse(c"FL@a+FR@b+FC@c").unwrap();
        let taken = l.as_mut().take_custom_map().unwrap();
        assert_eq!(taken.len(), 3);
        assert_eq!(l.as_ref().order(), ChannelOrder::UNSPEC);
        assert_eq!(l.as_ref().channel_count(), 3);
        // The taken map goes back in, under a new layout, and is freed there.
        let back = ChannelLayout::from_custom_map(taken).unwrap();
        assert_eq!(walk(back.as_ref()), 3);
    }

    /// One state change of a random sequence.
    fn step(l: &mut ChannelLayoutVal, other: &mut ChannelLayoutVal, r: u64) {
        let arg = r >> 8;
        match r % 13 {
            0 => l.as_mut().set_unspecified(arg as i32),
            1 => l.as_mut().set_native(arg),
            2 => {
                let ok = l
                    .as_mut()
                    .set_ambisonic((arg % 50_000) as u32, arg.rotate_left(7));
                assert_eq!(ok.is_ok(), arg % 50_000 < 46_340);
            }
            3 => {
                let n = 1 + (arg % 9) as usize;
                l.as_mut().set_custom_map(map(n)).unwrap();
            }
            4 => {
                // An odd `arg` drops the taken map.
                if let Some(m) = l.as_mut().take_custom_map()
                    && arg.is_multiple_of(2)
                {
                    other.as_mut().set_custom_map(m).unwrap();
                }
            }
            5 => {
                let descriptions: Vec<_> = allowed(&DESCRIPTIONS).collect();
                if let Ok(parsed) =
                    ChannelLayout::parse(descriptions[arg as usize % descriptions.len()])
                {
                    drop(l.as_mut().replace(parsed));
                }
            }
            6 => *other = l.as_mut().take(),
            7 => l.as_mut().clear(),
            8 => l.as_mut().copy_from(other.as_ref()).unwrap(),
            9 => *other = l.as_ref().try_clone().unwrap(),
            10 => l.as_mut().set_opaque(dangling(arg as usize | 1)),
            11 => core::mem::swap(l, other),
            _ => {
                if let Some(mut map) = l.as_mut().custom_map_mut() {
                    let i = (arg as usize) % map.len();
                    map.get_mut(i)
                        .unwrap()
                        .set_id(Channel::from_raw(arg as i32));
                }
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn setter_sequences_free_every_map_once() {
        let mut x = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = move || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x
        };
        for _ in 0..40 {
            let mut l = ChannelLayout::unspecified(0);
            let mut other = ChannelLayout::custom(2).unwrap();
            for _ in 0..60 {
                step(&mut l, &mut other, next());
                walk(l.as_ref());
                walk(other.as_ref());
            }
        }
    }

    /// `ChannelCustom::from_raw` can carry a name with no NUL into a map;
    /// the getters read it within its field, and copies copy it as is.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn unterminated_names_in_a_map_are_read_within_the_field() {
        let raw = ffi::AVChannelCustom {
            id: 3,
            name: [b'x' as core::ffi::c_char; 16],
            opaque: dangling(8),
        };
        let mut m = alloc_cell_array_zeroed::<ChannelCustom>(2).unwrap();
        m.as_handles_mut()
            .get_mut(1)
            .unwrap()
            .set(ChannelCustom::from_raw(raw));
        let l = ChannelLayout::from_custom_map(m).unwrap();
        let copy = l.as_ref().try_clone().unwrap();
        drop(l);
        let LayoutDetails::Custom(map) = copy.as_ref().details() else {
            panic!()
        };
        assert_eq!(map.get(1).unwrap().name().as_bytes(), [b'x'; 16]);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn user_pointers_are_stored_never_followed() {
        let mut l = ChannelLayout::parse(c"FL@a+FR").unwrap();
        for addr in [1, 0x1000, usize::MAX] {
            l.as_mut().set_opaque(dangling(addr));
            let copy = l.as_ref().try_clone().unwrap();
            assert_eq!(copy.as_ref().opaque(), dangling(addr));
            l.as_mut().set_native(0x7);
            assert_eq!(l.as_ref().opaque(), dangling(addr));
            l.as_mut().set_custom_map(map(2)).unwrap();
            drop(l.as_mut().take_custom_map());
            assert_eq!(l.as_ref().opaque(), dangling(addr));
            l.as_mut().copy_from(copy.as_ref()).unwrap();
        }
        l.as_mut().clear();
        assert!(l.as_ref().opaque().is_null());
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn bare_values_rewrap_and_handles_reborrow() {
        let l = ChannelLayout::parse(c"FL@a+FR@b").unwrap();
        // Given up for disposal, the value is a plain layout held inline.
        let mut bare = l.into_inner();
        {
            let mut m = bare.as_mut();
            let mut again = m.as_mut();
            again
                .custom_map_mut()
                .unwrap()
                .get_mut(0)
                .unwrap()
                .set_id(Channel::BACK_LEFT);
            assert_eq!(walk(m.as_ref()), 2);
        }
        let owned = ChannelLayoutVal::from(bare);
        let (a, b) = (owned.as_ref(), owned.as_ref());
        assert_eq!(a.channel_count(), b.channel_count());
        drop(owned);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn owners_move_and_drop_across_threads() {
        let layouts: Vec<ChannelLayoutVal> = allowed(&DESCRIPTIONS)
            .filter_map(|s| ChannelLayout::parse(s).ok())
            .collect();
        let back = thread::spawn(move || {
            layouts
                .iter()
                .map(|l| l.as_ref().try_clone().unwrap())
                .collect::<Vec<_>>()
        })
        .join()
        .unwrap();
        for l in &back {
            walk(l.as_ref());
        }
        thread::spawn(move || drop(back)).join().unwrap();
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn shared_views_read_and_copy_concurrently() {
        let l = ChannelLayout::parse(c"FL@front+FR@right+LFE+USR40@u").unwrap();
        let r = l.as_ref();
        let LayoutDetails::Custom(map) = r.details() else {
            panic!()
        };
        thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(move || {
                    for _ in 0..200 {
                        assert_eq!(walk(r), 4);
                        assert_eq!(map.get(3).unwrap().name(), c"u");
                        drop(r.try_clone().unwrap());
                    }
                });
            }
        });
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn exclusive_handles_and_map_views_write_on_other_threads() {
        let mut layouts: Vec<ChannelLayoutVal> =
            (1..=4).map(|n| ChannelLayout::custom(n).unwrap()).collect();
        thread::scope(|s| {
            for (i, l) in layouts.iter_mut().enumerate() {
                let mut m = l.as_mut();
                s.spawn(move || {
                    let mut map = m.custom_map_mut().unwrap();
                    for mut c in map.iter_mut() {
                        c.set_id(Channel::from_raw(i as i32));
                    }
                    m.set_opaque(dangling(i + 1));
                    if i % 2 == 0 {
                        m.set_native(0x3);
                    }
                });
            }
        });
        for (i, l) in layouts.iter().enumerate() {
            walk(l.as_ref());
            assert_eq!(l.as_ref().opaque(), dangling(i + 1));
        }
        // A shared exclusive handle only reads.
        let mut l = ChannelLayout::parse(c"FL@a+FR").unwrap();
        let m = l.as_mut();
        let m = &m;
        thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(move || assert_eq!(walk(m.as_ref()), 2));
            }
        });
    }

    /// `Channel::mask` at every id class, and the mask constants as inputs
    /// to every mask-taking path.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn masks_and_bits_feed_every_mask_path() {
        for raw in [
            i32::MIN,
            -1,
            0,
            1,
            63,
            64,
            0x200,
            0x300,
            0x400,
            0x7FF,
            i32::MAX,
        ] {
            let ch = Channel::from_raw(raw);
            assert_eq!(ch.mask().is_some(), (0..64).contains(&raw), "{raw}");
        }
        let mut all = 0;
        for (name, ch) in CHANNEL_BITS {
            let bit = ch.mask().unwrap_or_else(|| panic!("{name}"));
            assert_eq!(all & bit, 0, "{name}");
            all |= bit;
        }
        for (name, mask) in LAYOUT_MASKS {
            assert_eq!(mask & !all, 0, "{name}");
            let l = ChannelLayout::from_mask(mask).unwrap();
            assert_eq!(walk(l.as_ref()), 0, "{name}");
            let mut set = ChannelLayout::custom(3).unwrap();
            set.as_mut().set_native(mask);
            assert_eq!(
                l.as_ref().channel_count(),
                set.as_ref().channel_count(),
                "{name}"
            );
            assert!(set.as_mut().set_ambisonic(1, mask).is_ok(), "{name}");
            walk(set.as_ref());
        }
    }

    /// Every standard layout is read, copied, owned, overwritten, moved
    /// across threads and dropped without touching anything it does not own.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn standard_layouts_are_plain_values() {
        for (name, layout) in STANDARD_LAYOUTS {
            assert_eq!(walk(layout.as_ref()), 0, "{name}");
            let copy = layout.as_ref().try_clone().unwrap();
            walk(copy.as_ref());
            let mut owned = ChannelLayoutVal::from(layout);
            owned.as_mut().set_opaque(dangling(0x40));
            owned.as_mut().set(ChannelLayout::custom(2).unwrap());
            assert_eq!(walk(owned.as_ref()), 2);
            owned.as_mut().set(ChannelLayout::STEREO.into());
            assert_eq!(owned.as_ref().channel_count(), 2);
            drop(owned.as_mut().take());
            // The constant is untouched by whatever its owners did.
            assert_eq!(walk(ChannelLayout::STEREO.as_ref()), 0);
        }
        let owners: Vec<ChannelLayoutVal> = STANDARD_LAYOUTS
            .into_iter()
            .map(|(_, l)| ChannelLayoutVal::from(l))
            .collect();
        let back = thread::spawn(move || {
            owners
                .iter()
                .map(|l| l.as_ref().try_clone().unwrap())
                .collect::<Vec<_>>()
        })
        .join()
        .unwrap();
        assert_eq!(back.len(), STANDARD_LAYOUTS.len());
        thread::scope(|s| {
            for l in &back {
                s.spawn(move || walk(l.as_ref()));
            }
        });
        // A bare native value given up by its owner owns nothing to leak.
        let bare = ChannelLayoutVal::from(ChannelLayout::L22POINT2).into_inner();
        assert_eq!(bare.as_ref().channel_count(), 24);
        let custom = ChannelLayout::native(u64::MAX);
        assert_eq!(walk(custom.as_ref()), 0);
        assert_eq!(walk(ChannelLayout::native(0).as_ref()), 0);
    }

    /// `ChannelLayoutMut::retype` and its siblings, `ChannelLayoutRef::subset`
    /// and `standard_layouts` (formats wave-3 batch 2).
    mod order_changes {
        use ffmpeg_bindings::libavutil::libavutil::channel_layout::{
            Conversion, masks, standard_layouts,
        };

        use super::*;

        /// Every order C knows, and some it does not.
        fn orders() -> impl Iterator<Item = ChannelOrder> {
            [0, 1, 2, 3, 4, 7, u32::MAX]
                .into_iter()
                .map(ChannelOrder::from_raw)
        }

        /// A map of exactly `ids.len()` unnamed channels.
        fn ids_map(ids: &[i32]) -> HeapVec<ChannelCustom> {
            let mut map = alloc_cell_array_zeroed::<ChannelCustom>(ids.len()).unwrap();
            for (mut m, &id) in map.as_handles_mut().iter_mut().zip(ids) {
                m.set_id(Channel::from_raw(id));
            }
            map
        }

        /// A map whose last element's name fills its field with no NUL.
        fn unterminated_map(ids: &[i32]) -> HeapVec<ChannelCustom> {
            let mut map = ids_map(ids);
            let last = ids.len() - 1;
            let raw = ffi::AVChannelCustom {
                id: (ids[last]),
                name: [b'x' as core::ffi::c_char; 16],
                opaque: dangling(8),
            };
            map.as_handles_mut()
                .get_mut(last)
                .unwrap()
                .set(ChannelCustom::from_raw(raw));
            map
        }

        /// Starting layouts of every order, map shape and size the safe
        /// API builds, the extreme counts included.
        pub(super) fn starts() -> Vec<ChannelLayoutVal> {
            let mut out: Vec<ChannelLayoutVal> = allowed(&DESCRIPTIONS)
                .filter_map(|s| ChannelLayout::parse(s).ok())
                .collect();
            // BSan pays per element read, and every start is walked after
            // each of its 21 conversions: the largest maps stay out there.
            let sizes: &[usize] = if under_bsan() {
                &[1, 2, 64]
            } else {
                &[1, 2, 63, 64, 300]
            };
            for &n in sizes {
                out.push(ChannelLayout::from_custom_map(map(n)).unwrap());
                out.push(ChannelLayout::custom(n as i32).unwrap());
            }
            for ids in [
                &[0, 1, 3][..],
                &[0x400, 0x401, 0x402, 0x403, 2, 62],
                &[0x400, 0x401],
                &[0x300, 0x300],
                &[-1, 0],
                &[i32::MIN, i32::MAX],
                &[0x7FF, 0x400],
            ] {
                out.push(ChannelLayout::from_custom_map(ids_map(ids)).unwrap());
                out.push(ChannelLayout::from_custom_map(unterminated_map(ids)).unwrap());
            }
            for n in [i32::MIN, -1, 0, 1, 64, i32::MAX] {
                out.push(ChannelLayout::unspecified(n));
            }
            for mask in [0, 1, masks::L22POINT2, 1 << 63, u64::MAX] {
                out.push(ChannelLayout::native(mask).into());
            }
            for (order, extra) in [(0, 0), (1, 0x3), (3, u64::MAX), (46339, 0), (46339, 1)] {
                let mut l = ChannelLayout::unspecified(0);
                l.as_mut().set_ambisonic(order, extra).unwrap();
                out.push(l);
            }
            for l in &mut out {
                l.as_mut().set_opaque(dangling(0x30));
            }
            out
        }

        /// Order, count and user pointer, which every refused conversion
        /// leaves as they were.
        fn summary(l: &ChannelLayoutVal) -> (ChannelOrder, i32, usize) {
            let r = l.as_ref();
            (r.order(), r.channel_count(), r.opaque() as usize)
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn every_conversion_frees_and_allocates_maps_once() {
            let mut converted = 0;
            for start in starts() {
                for order in orders() {
                    for variant in 0..3 {
                        let mut l = start.as_ref().try_clone().unwrap();
                        let before = summary(&l);
                        let res = match variant {
                            0 => l.as_mut().retype(order).map(|c| c == Conversion::Lossy),
                            1 => l.as_mut().retype_lossless(order).map(|()| false),
                            _ => l.as_mut().canonicalize().map(|()| false),
                        };
                        let after = summary(&l);
                        match res {
                            Ok(_) => {
                                converted += 1;
                                assert_eq!(after.1, before.1);
                                if variant != 2 {
                                    assert_eq!(after.0, order);
                                }
                            }
                            Err(e) => {
                                assert!(e < 0);
                                assert_eq!(after, before);
                            }
                        }
                        assert_eq!(after.2, 0x30);
                        walk(l.as_ref());
                        for mask in [0, u64::MAX, 0x3, 1 << 63] {
                            assert_eq!(l.as_ref().subset(mask) & !mask, 0);
                        }
                    }
                }
                // The source is untouched by its copies' conversions.
                walk(start.as_ref());
            }
            assert!(converted > 100, "{converted}");
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn conversions_interleave_with_every_setter() {
            let mut x = 0x2545_F491_4F6C_DD1Du64;
            let mut next = move || {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                x
            };
            let orders: Vec<_> = orders().collect();
            for _ in 0..30 {
                let mut l = ChannelLayout::unspecified(2);
                let mut other = ChannelLayout::custom(3).unwrap();
                for _ in 0..60 {
                    let r = next();
                    let mut order = orders[(r >> 4) as usize % orders.len()];
                    // `step` leaves counts up to `i32::MAX`: a custom map of
                    // up to `av_max_alloc` bytes (89M channels) would be
                    // allocated and walked, so large layouts convert only
                    // to orders holding no map. Counts past that limit
                    // still reach C's ENOMEM path.
                    let count = l.as_ref().channel_count();
                    if order == ChannelOrder::CUSTOM && (1024..=i32::MAX / 24).contains(&count) {
                        order = ChannelOrder::UNSPEC;
                    }
                    match r % 4 {
                        0 => drop(l.as_mut().retype(order)),
                        1 => drop(l.as_mut().retype_lossless(order)),
                        2 => drop(l.as_mut().canonicalize()),
                        _ => step(&mut l, &mut other, next()),
                    }
                    walk(l.as_ref());
                    walk(other.as_ref());
                    let _ = l.as_ref().subset(next());
                }
            }
        }

        /// A name with no NUL is only tested for emptiness by the
        /// conversion and never read by the subset query.
        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn unterminated_names_are_read_within_the_field() {
            for ids in [&[0, 1][..], &[0x400, 0x401, 0x402, 0x403, 5]] {
                let mut l = ChannelLayout::from_custom_map(unterminated_map(ids)).unwrap();
                assert_eq!(
                    l.as_ref().subset(u64::MAX),
                    ids.iter().filter(|&&i| i < 64).fold(0, |m, &i| m | 1 << i)
                );
                assert_eq!(l.as_mut().retype_lossless(ChannelOrder::NATIVE), Err(-38));
                assert_eq!(l.as_mut().canonicalize(), Ok(()));
                assert_eq!(l.as_ref().order(), ChannelOrder::CUSTOM);
                assert_eq!(
                    l.as_mut().retype(ChannelOrder::UNSPEC),
                    Ok(Conversion::Lossy)
                );
                assert_eq!(walk(l.as_ref()), 0);
            }
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn subset_reads_every_map_size_in_bounds() {
            for n in [1, 2, 63, 64, 65, 300] {
                let l = ChannelLayout::from_custom_map(map(n)).unwrap();
                // `map` designates ids 0..n, of which 0..63 are native.
                let want = if n >= 64 { u64::MAX } else { (1u64 << n) - 1 };
                assert_eq!(l.as_ref().subset(u64::MAX), want, "{n}");
                assert_eq!(l.as_ref().subset(1 << 63), want & 1 << 63, "{n}");
            }
            for s in allowed(&DESCRIPTIONS) {
                if let Ok(l) = ChannelLayout::parse(s) {
                    for mask in [0, 1, 0x3, masks::L5POINT1, 1 << 63, u64::MAX] {
                        assert_eq!(l.as_ref().subset(mask) & !mask, 0, "{s:?}");
                    }
                }
            }
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn standard_layouts_outlive_their_iterator_and_copies() {
            // The handles are the table's, not the iterator's, which the
            // walk consumes.
            let kept: Vec<ChannelLayoutRef<'static>> = {
                let mut it = standard_layouts();
                let first = it.next().unwrap();
                core::iter::once(first).chain(it).collect()
            };
            assert_eq!(kept.len(), 40);
            let mut owners = vec![];
            for l in &kept {
                walk(*l);
                let mut owned = l.try_clone().unwrap();
                owned.as_mut().set_opaque(dangling(0x10));
                assert_eq!(
                    owned.as_mut().retype(ChannelOrder::CUSTOM),
                    Ok(Conversion::Lossless)
                );
                owners.push(owned);
            }
            drop(owners);
            // The table is untouched by whatever its copies did.
            for l in standard_layouts() {
                assert!(l.opaque().is_null());
                assert_eq!(walk(l), 0);
            }
            let mut done = standard_layouts();
            done.by_ref().for_each(drop);
            for _ in 0..3 {
                assert!(done.next().is_none());
            }
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn standard_layouts_are_walked_and_shared_across_threads() {
            let kept: Vec<_> = standard_layouts().collect();
            thread::scope(|s| {
                for t in 0..4 {
                    let kept = &kept;
                    s.spawn(move || {
                        for _ in 0..20 {
                            let walked: Vec<_> = standard_layouts().collect();
                            assert_eq!(walked.len(), kept.len());
                            for (a, b) in walked.iter().zip(kept) {
                                assert_eq!(a.channel_count(), b.channel_count());
                                assert_eq!(a.subset(u64::MAX), b.subset(u64::MAX));
                            }
                            let mut l = kept[t * 9].try_clone().unwrap();
                            l.as_mut().retype(ChannelOrder::CUSTOM).unwrap();
                            l.as_mut().canonicalize().unwrap();
                            walk(l.as_ref());
                        }
                    });
                }
            });
            let it = standard_layouts();
            let n = thread::spawn(move || it.count()).join().unwrap();
            assert_eq!(n, 40);
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn owners_convert_on_other_threads() {
            let mut layouts = starts();
            thread::scope(|s| {
                for (i, l) in layouts.iter_mut().enumerate() {
                    let mut m = l.as_mut();
                    s.spawn(move || {
                        let order = [ChannelOrder::CUSTOM, ChannelOrder::UNSPEC][i % 2];
                        let _ = m.retype(order);
                        let _ = m.canonicalize();
                        let _ = m.as_ref().subset(u64::MAX);
                    });
                }
            });
            for l in &layouts {
                walk(l.as_ref());
            }
            // Shared handles query one layout concurrently.
            let l = ChannelLayout::parse(c"FL@a+FR+LFE+USR40").unwrap();
            let r = l.as_ref();
            thread::scope(|s| {
                for _ in 0..4 {
                    s.spawn(move || {
                        for _ in 0..100 {
                            assert_eq!(r.subset(u64::MAX), 0b1011 | 1 << 40);
                        }
                    });
                }
            });
            thread::spawn(move || drop(layouts)).join().unwrap();
        }
    }

    /// The readers of `ChannelLayoutRef` (`is_valid`, `ambisonic_order`,
    /// `channel`, `index_of`, `index_by_name`, `channel_by_name`, the three
    /// describers) and `==`: every start state of every order, map shape and
    /// extreme count, names and output buffers in exactly sized heap blocks,
    /// unterminated map names at the end of their C block, random setter
    /// sequences, and shared handles read on many threads (formats wave-3
    /// batch 1).
    mod readers {
        use core::ffi::{CStr, c_char};
        use std::ffi::CString;

        use ffmpeg_bindings::libavutil::libavutil::bprint::BPrint;

        use super::order_changes::starts;
        use super::*;

        /// Channels at every id class, `NONE` and the `int` extremes.
        fn channels() -> Vec<Channel> {
            [
                -1,
                0,
                1,
                3,
                40,
                62,
                63,
                64,
                0x200,
                0x300,
                0x400,
                0x403,
                0x404,
                0x7FF,
                0x800,
                i32::MAX,
                i32::MIN,
            ]
            .map(Channel::from_raw)
            .into()
        }

        /// Names of every lookup shape, each in an exactly sized heap block
        /// (`CString` keeps none spare), so a read past its NUL is an `ASan`
        /// report.
        fn names() -> Vec<CString> {
            [
                "",
                "@",
                "FL",
                "LFE",
                "USR40",
                "AMBI3",
                "UNK",
                "NONE",
                "FL@",
                "@x",
                "@front",
                "FL@front",
                "FR@front",
                "XX@front",
                "@0123456789abcde",
                "@0123456789abcdefghij",
                "USR63@0123456789abcde",
                "@xxxxxxxxxxxxxxxx",
                "0@1@2",
            ]
            .map(|s| CString::new(s).unwrap())
            .into()
        }

        /// Calls every reader on `l` and checks what they return against one
        /// another; returns the description, if any.
        fn read_all(l: ChannelLayoutRef<'_>) -> Option<CString> {
            let _ = l.is_valid();
            if let Some(order) = l.ambisonic_order() {
                assert!(order < 46_340);
            }
            let n = usize::try_from(l.channel_count()).unwrap_or(0).min(70);
            for i in 0..=n + 1 {
                if let Some(ch) = l.channel(i) {
                    // The channel's first position, which holds it.
                    let j = l.index_of(ch).unwrap();
                    assert!(j <= i);
                    assert_eq!(l.channel(j), Some(ch));
                }
            }
            for i in [u32::MAX as usize, u32::MAX as usize + 1, usize::MAX] {
                assert_eq!(l.channel(i), None);
            }
            for ch in channels() {
                if let Some(i) = l.index_of(ch) {
                    assert_eq!(l.channel(i), Some(ch));
                }
            }
            assert_eq!(l.index_of(Channel::NONE), None);
            for name in names() {
                let at = l.index_by_name(&name);
                let ch = l.channel_by_name(&name);
                assert_eq!(at.and_then(|i| l.channel(i)), ch, "{name:?}");
            }
            let same = l;
            assert_eq!(l, same);
            let description = l.description();
            match &description {
                Ok(d) => {
                    let need = d.as_bytes_with_nul().len();
                    assert_eq!(l.write_description(&mut []), Ok(need));
                    for len in (1..need.min(24)).chain([need - 1, need, need + 1]) {
                        if len == 0 {
                            continue;
                        }
                        // An exactly sized heap block: one byte past it is
                        // outside the allocation.
                        let mut buf = vec![0xAAu8; len].into_boxed_slice();
                        assert_eq!(l.write_description(&mut buf), Ok(need));
                        let end = (need - 1).min(len - 1);
                        assert_eq!(buf[end], 0);
                        assert_eq!(buf[..end], d.as_bytes()[..end]);
                    }
                    for len in [1, need / 2 + 1, need] {
                        let mut data = vec![0xAAu8; len].into_boxed_slice();
                        let mut buf = BPrint::for_buffer(&mut data).unwrap();
                        l.append_description(buf.as_mut()).unwrap();
                        assert_eq!(buf.as_ref().len() as usize, need - 1);
                        let stored = buf.as_ref().as_bytes().len();
                        drop(buf);
                        assert_eq!(data[stored], 0);
                    }
                }
                Err(e) => {
                    assert!(*e < 0);
                    let mut buf = vec![0xAAu8; 8].into_boxed_slice();
                    assert_eq!(l.write_description(&mut buf), Err(*e));
                    let mut b = BPrint::new(0, BPrint::SIZE_UNLIMITED).unwrap();
                    b.as_mut().append_bytes(b"kept");
                    assert_eq!(l.append_description(b.as_mut()), Err(*e));
                    assert_eq!(b.as_ref().as_bytes(), b"kept");
                }
            }
            description.ok()
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn every_reader_reads_every_start_in_bounds() {
            let mut described = 0;
            for l in starts() {
                let d = read_all(l.as_ref());
                described += usize::from(d.is_some());
                // A copy reads alike, and the source is left as it was.
                let copy = l.as_ref().try_clone().unwrap();
                assert_eq!(read_all(copy.as_ref()), d);
                assert_eq!(copy.as_ref(), l.as_ref());
                walk(l.as_ref());
            }
            for ((_, layout), (_, twin)) in STANDARD_LAYOUTS.into_iter().zip(STANDARD_LAYOUTS) {
                assert!(read_all(layout.as_ref()).is_some());
                assert_eq!(layout, twin);
            }
            assert!(described > 20, "{described}");
        }

        /// A map whose last element's name fills its field with no NUL, at
        /// the end of the C block: C's string readers would run past the
        /// field, into the padding and user pointer after it (all-ones
        /// here) and past the block if those held no zero byte. The padding
        /// is not the safe API's to set, so the guard's verdict, not a
        /// sanitizer, is what this checks.
        fn unterminated_at_block_end(ids: &[i32]) -> ChannelLayoutVal {
            let mut map = alloc_cell_array_zeroed::<ChannelCustom>(ids.len()).unwrap();
            for (i, (mut m, &id)) in map.as_handles_mut().iter_mut().zip(ids).enumerate() {
                m.set_id(Channel::from_raw(id));
                if i + 1 == ids.len() {
                    m.set(ChannelCustom::from_raw(ffi::AVChannelCustom {
                        id,
                        name: [b'x' as c_char; 16],
                        opaque: dangling(usize::MAX),
                    }));
                } else {
                    m.set_name(c"named").unwrap();
                }
            }
            ChannelLayout::from_custom_map(map).unwrap()
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn unterminated_names_are_never_read_as_strings() {
            for ids in [&[0, 1][..], &[1][..], &[0x400, 0x401, 0x402, 0x403, 5]] {
                let l = unterminated_at_block_end(ids);
                let r = l.as_ref();
                // The describers would print the name with `%s`.
                assert_eq!(r.description(), Err(-22));
                assert_eq!(read_all(r), None);
                // A search by name would `strcmp` it; one by abbreviation
                // only does not read names.
                for name in [c"@xxxxxxxxxxxxxxxx", c"@x", c"FL@named", c"FR@"] {
                    let compares = name
                        .to_bytes()
                        .split(|&b| b == b'@')
                        .nth(1)
                        .is_some_and(|s| !s.is_empty());
                    if compares {
                        assert_eq!(r.index_by_name(name), None, "{name:?}");
                        assert_eq!(r.channel_by_name(name), None, "{name:?}");
                    }
                }
                let last = Channel::from_raw(*ids.last().unwrap());
                let name = last.name();
                assert_eq!(r.index_by_name(&name), Some(ids.len() - 1));
                assert_eq!(r.channel_by_name(&name), Some(last));
                // The other readers never read names.
                assert!(r.is_valid());
                assert_eq!(r.channel(ids.len() - 1), Some(last));
                assert_eq!(r.index_of(last), Some(ids.len() - 1));
                assert_eq!(r, l.as_ref().try_clone().unwrap().as_ref());
                let _ = r.ambisonic_order();
            }
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn layouts_compare_with_every_start() {
            let all = starts();
            for (i, a) in all.iter().enumerate() {
                for (j, b) in all.iter().enumerate() {
                    let eq = a.as_ref() == b.as_ref();
                    assert_eq!(eq, b.as_ref() == a.as_ref(), "{i} {j}");
                    if eq {
                        assert_eq!(a.as_ref().channel_count(), b.as_ref().channel_count());
                    }
                }
                let (x, y) = (a.as_ref(), a.as_ref());
                assert!(x == y, "{i}");
            }
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn readers_interleave_with_every_setter() {
            let mut x = 0x5851_F42D_4C95_7F2Du64;
            let mut next = move || {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                x
            };
            let names = names();
            for _ in 0..20 {
                let mut l = ChannelLayout::unspecified(2);
                let mut other = ChannelLayout::custom(3).unwrap();
                for _ in 0..40 {
                    step(&mut l, &mut other, next());
                    let r = l.as_ref();
                    let i = (next() % 80) as usize;
                    let _ = (r.channel(i), r.is_valid(), r.ambisonic_order());
                    let _ = r.index_of(Channel::from_raw(next() as i32 % 0x900));
                    let name: &CStr = &names[next() as usize % names.len()];
                    let _ = (r.index_by_name(name), r.channel_by_name(name));
                    let _ = r == other.as_ref();
                    if let Ok(d) = r.description() {
                        assert!(!d.as_bytes().is_empty());
                    }
                    walk(r);
                }
            }
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn shared_handles_read_on_many_threads() {
            let layouts: Vec<ChannelLayoutVal> = [
                c"FL@front+FR@right+LFE+USR40@u",
                c"5.1(side)",
                c"AMBI0+AMBI1+AMBI2+AMBI3+FL@x",
            ]
            .into_iter()
            .map(|s| ChannelLayout::parse(s).unwrap())
            .collect();
            let expect: Vec<_> = layouts.iter().map(|l| l.as_ref().description()).collect();
            let names = names();
            thread::scope(|s| {
                for t in 0..4 {
                    let (layouts, expect, names) = (&layouts, &expect, &names);
                    s.spawn(move || {
                        let mut buf = BPrint::new(0, BPrint::SIZE_UNLIMITED).unwrap();
                        for k in 0..50 {
                            let l = layouts[(t + k) % layouts.len()].as_ref();
                            assert_eq!(&l.description(), &expect[(t + k) % layouts.len()]);
                            l.append_description(buf.as_mut()).unwrap();
                            let _ = l.index_by_name(&names[k % names.len()]);
                            let _ = (l.channel(k % 6), l.ambisonic_order(), l.is_valid());
                            assert_eq!(l, layouts[(t + k) % layouts.len()].as_ref());
                            buf.as_mut().clear();
                        }
                    });
                }
            });
        }
    }
}
