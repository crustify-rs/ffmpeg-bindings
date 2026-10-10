//! UB tests for `libavutil/channel_layout.h`'s [`Channel`], through the safe
//! API only: as a value, its Ambisonic conversions at every boundary, and as
//! the plain element of `av_malloc`-family arrays.

use std::thread;

use ffmpeg::libavutil::channel_layout::Channel;
use ffmpeg::libavutil::ffi;
use ffmpeg::libavutil::mem::{HeapVec, alloc_array, alloc_array_from_fn, alloc_array_zeroed};

/// Which descriptions C's parser may be given under BSan.
#[path = "../common/channel_layout.rs"]
mod parser_inputs;

/// The BSan input filter keeps exactly the descriptions C matches by name or
/// accepts as a channel list, and skips every other spelling.
#[test]
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

/// `Channel::write_name`, `write_description`, `name`, `description` and
/// `from_name`: every id class and the `int` extremes written into exactly
/// sized heap buffers of every length (so a write past the end is an ASan
/// report), names of every shape in exactly sized heap strings, and the
/// calls on several threads at once.
mod channel_names {
    use std::ffi::CString;
    use std::thread;

    use ffmpeg::libavutil::channel_layout::Channel;
    use ffmpeg::libavutil::ffi;

    fn ids() -> Vec<Channel> {
        (-4..70)
            .chain(1020..1030)
            .chain(2040..2060)
            .chain([i32::MAX, i32::MIN, i32::MIN + 1])
            .map(|id| Channel::from_raw(ffi::AVChannel(id)))
            .collect()
    }

    #[test]
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
/// every small length (so a byte past the end is an ASan report), handles
/// reborrowed from owners and from other handles, and buffers on many
/// threads at once.
mod name_appenders {
    use std::thread;

    use ffmpeg::libavutil::bprint::{BPrint, BPrintMut};
    use ffmpeg::libavutil::channel_layout::Channel;

    use super::channel_sample;

    fn append_both(ch: Channel, mut buf: BPrintMut<'_>) {
        ch.append_name(buf.as_mut());
        buf.append_bytes(b"/");
        ch.append_description(buf);
    }

    #[test]
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

    use ffmpeg::libavutil::channel_layout::{
        Channel, ChannelCustom, ChannelLayout, ChannelLayoutRef, ChannelLayoutVal, ChannelOrder,
        LayoutDetails,
    };
    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::mem::{HeapVec, alloc_cell_array_zeroed};

    use super::parser_inputs::{CHANNEL_BITS, LAYOUT_MASKS, STANDARD_LAYOUTS, allowed, under_bsan};

    /// Descriptions C parses, fails to parse, or stops short in: standard
    /// names, channel lists with and without names, masks, counts,
    /// ambisonic orders at the `int` bound, and malformed spellings.
    const DESCRIPTIONS: [&core::ffi::CStr; 37] = [
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
            m.set_id(Channel::from_raw(ffi::AVChannel(i as i32)));
            let _ = m.set_name_truncated(&CString::new(format!("{i:015}")).unwrap());
        }
        map
    }

    #[test]
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

    #[test]
    fn custom_map_handles_stay_in_bounds() {
        let mut l = ChannelLayout::parse(c"FL@0123456789abcde+FR+LFE@lfe").unwrap();
        {
            let mut map = l.as_mut().into_custom_map_mut().unwrap();
            assert_eq!(map.len(), 3);
            assert!(map.get_mut(3).is_none());
            for (i, mut m) in map.iter_mut().enumerate() {
                m.set_id(Channel::from_raw(ffi::AVChannel(i32::MAX - i as i32)));
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
                .all(|c| c.id().into_raw().0 < i32::MAX)
        );
        assert!(map.slice(..4).is_none());
    }

    /// An `av_dynarray2_add` map may hold spare capacity past the count;
    /// taking it over and freeing it frees the whole block.
    #[test]
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
                        .set_id(Channel::from_raw(ffi::AVChannel(arg as i32)));
                }
            }
        }
    }

    #[test]
    fn setter_sequences_free_every_map_once() {
        let mut x = 0x9e37_79b9_7f4a_7c15u64;
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
    fn unterminated_names_in_a_map_are_read_within_the_field() {
        let raw = ffi::AVChannelCustom {
            id: ffi::AVChannel(3),
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
        let owned = ChannelLayoutVal::new(bare);
        let (a, b) = (owned.as_ref(), owned.as_ref());
        assert_eq!(a.channel_count(), b.channel_count());
        drop(owned);
    }

    #[test]
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
    fn exclusive_handles_and_map_views_write_on_other_threads() {
        let mut layouts: Vec<ChannelLayoutVal> =
            (1..=4).map(|n| ChannelLayout::custom(n).unwrap()).collect();
        thread::scope(|s| {
            for (i, l) in layouts.iter_mut().enumerate() {
                let mut m = l.as_mut();
                s.spawn(move || {
                    let mut map = m.custom_map_mut().unwrap();
                    for mut c in map.iter_mut() {
                        c.set_id(Channel::from_raw(ffi::AVChannel(i as i32)));
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
            0x7ff,
            i32::MAX,
        ] {
            let ch = Channel::from_raw(ffi::AVChannel(raw));
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
}
