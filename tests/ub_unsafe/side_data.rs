//! UB tests for the unsafe API of `libavutil/side_data.c`'s side data arrays,
//! called within their `# Safety` contracts.
#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "test inputs are chosen within range, and the casts mirror the C conversions under comparison"
)]

/// `SideDataArray::from_raw_parts`: adopting arrays C built, within the
/// contract, at its edges (NULL and 0, empty payloads, page-sized ones,
/// entries sharing buffers across two arrays, references C keeps for the
/// caller, arrays C shrank), round trips through `into_raw_parts` with C
/// changing the array in between, and adopted arrays read, written and
/// released on other threads, in every drop order (buffer-frame wave 2,
/// batch 0).
mod side_data_array_from_raw_parts {
    use core::ffi::c_int;
    use core::ptr;
    use std::thread;

    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::frame::{SideDataProps, SideDataType};
    use ffmpeg_bindings::libavutil::libavutil::side_data::{SideDataArray, SideDataFlags};

    type Parts = (*mut *mut ffi::AVFrameSideData, c_int);

    /// Whether every byte of `bytes` is `b`, compared as one slice: `BSan`
    /// retags once per element an iterator yields.
    fn is_all(bytes: &[u8], b: u8) -> bool {
        *bytes == *vec![b; bytes.len()]
    }

    /// C appends an entry of `kind` holding `len` bytes, all set to `byte`,
    /// and a `key` metadata pair when given one.
    fn push(
        parts: &mut Parts,
        kind: SideDataType,
        len: usize,
        byte: u8,
        key: Option<&core::ffi::CStr>,
    ) {
        // SAFETY: the array C built so far, or NULL and 0; the specs never
        // repeat a non-MULTI kind.
        let e = unsafe {
            ffi::av_frame_side_data_new(&raw mut parts.0, &raw mut parts.1, kind.into_raw(), len, 0)
        };
        assert!(!e.is_null());
        // For the unsafe operations below: the fresh entry's sole payload of `len` bytes, written
        // in full, and its metadata slot.
        {
            // SAFETY: as the comment below states, for this part of the operation.
            let data_3 = unsafe { (*e).data };
            // SAFETY: as the comment above this block states.
            unsafe { ptr::write_bytes(data_3, byte, len) };
            // SAFETY: `e` is to a live object, as the block below relies on, so the
            // place of its `metadata` field is in bounds; nothing is accessed here.
            let metadata_ptr = unsafe { &raw mut (*e).metadata };
            // SAFETY: as the comment above this block states.
            unsafe {
                if let Some(key) = key {
                    assert!(ffi::av_dict_set(metadata_ptr, key.as_ptr(), c"v".as_ptr(), 0) >= 0);
                }
            }
        }
    }

    /// A C array of one entry per length, alternating `MULTI` and unique
    /// kinds C does not name.
    fn built(lens: &[usize]) -> Parts {
        let mut parts: Parts = (ptr::null_mut(), 0);
        for (i, &len) in lens.iter().enumerate() {
            let kind = if i % 2 == 0 {
                SideDataType::SEI_UNREGISTERED
            } else {
                SideDataType::from_raw(ffi::AVFrameSideDataType(5000 + i as u32))
            };
            push(
                &mut parts,
                kind,
                len,
                i as u8,
                (i % 3 == 0).then_some(c"key"),
            );
        }
        parts
    }

    fn check(a: &SideDataArray, lens: &[usize]) {
        assert_eq!(a.len(), lens.len());
        for (i, (e, &len)) in a.as_ref().iter().zip(lens).enumerate() {
            assert_eq!(e.len(), len);
            assert!(is_all(e.data(), i as u8));
            assert_eq!(e.metadata().is_some(), i % 3 == 0);
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn null_and_zero_is_the_empty_array() {
        // SAFETY: NULL and 0, the empty array C's routines start from.
        let a = unsafe { SideDataArray::from_raw_parts(ptr::null_mut(), 0) };
        assert!(a.is_empty());
        let (sd, nb) = a.into_raw_parts();
        assert!(sd.is_null() && nb == 0);
        // SAFETY: as above, released on drop.
        drop(unsafe { SideDataArray::from_raw_parts(sd, nb) });
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn exactly_sized_payloads_read_and_write_in_full() {
        let lens = [0, 1, 2, 15, 16, 17, 4095, 4096, 65537];
        let (sd, nb) = built(&lens);
        // SAFETY: an array C built and nothing else holds, every payload
        // written by `push`.
        let mut a = unsafe { SideDataArray::from_raw_parts(sd, nb) };
        check(&a, &lens);
        for (i, mut e) in a.as_mut().iter_mut().enumerate() {
            e.data_mut().unwrap().fill(i as u8);
        }
        check(&a, &lens);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn round_trips_with_c_shrinking_the_array_between() {
        let lens = [3, 0, 9, 1, 30, 7];
        let (mut sd, mut nb) = built(&lens);
        for expected in [6, 3, 3] {
            // SAFETY: the array C built or changed last, every payload
            // initialised.
            let a = unsafe { SideDataArray::from_raw_parts(sd, nb) };
            assert_eq!(a.len(), expected);
            (sd, nb) = a.into_raw_parts();
            // SAFETY: the array given back; C frees its MULTI entries (the
            // even ones) the first time round, and none after.
            unsafe {
                ffi::av_frame_side_data_remove_by_props(
                    &raw mut sd,
                    &raw mut nb,
                    SideDataProps::MULTI.bits() as c_int,
                );
            };
        }
        // SAFETY: as above.
        let a = unsafe { SideDataArray::from_raw_parts(sd, nb) };
        assert!(
            a.as_ref()
                .iter()
                .all(|e| e.kind() != SideDataType::SEI_UNREGISTERED)
        );
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn arrays_sharing_buffers_drop_in_either_order() {
        for first in [false, true] {
            let lens = [8, 0, 100, 5];
            let (sd, nb) = built(&lens);
            let mut copy: Parts = (ptr::null_mut(), 0);
            for i in 0..nb as usize {
                // SAFETY: as the comment below states, for this part of the operation.
                let sd_add = unsafe { sd.add(i) };
                // SAFETY: entry `i` of the C array, initialised.
                let entry = unsafe { *sd_add };
                // SAFETY: a live entry, whose buffer C shares with the new entry and whose
                // metadata it copies.
                let r = unsafe {
                    ffi::av_frame_side_data_clone(&raw mut copy.0, &raw mut copy.1, entry, 0)
                };
                assert_eq!(r, 0);
            }
            // SAFETY: two arrays C built, each held by one owner; their
            // entries are distinct and share only buffers, by count.
            let (mut a, mut b) = (unsafe { SideDataArray::from_raw_parts(sd, nb) }, unsafe {
                SideDataArray::from_raw_parts(copy.0, copy.1)
            });
            check(&a, &lens);
            check(&b, &lens);
            assert!(a.as_mut().iter_mut().all(|mut e| e.data_mut().is_none()));
            if first {
                drop(a);
                assert!(b.as_mut().iter_mut().all(|mut e| e.data_mut().is_some()));
                check(&b, &lens);
            } else {
                drop(b);
                assert!(a.as_mut().iter_mut().all(|mut e| e.data_mut().is_some()));
                check(&a, &lens);
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn references_c_keeps_for_the_caller_make_payloads_read_only() {
        // For the unsafe operations below: a fresh C buffer of 6 bytes, filled.
        let mut buf = {
            // SAFETY: as the comment above this block states.
            let b = unsafe { ffi::av_buffer_alloc(6) };
            // SAFETY: as the comment below states, for this part of the operation.
            let data_2 = unsafe { (*b).data };
            // SAFETY: as the comment above this block states.
            unsafe { ptr::write_bytes(data_2, 0x42, 6) };
            b
        };
        let mut parts: Parts = (ptr::null_mut(), 0);
        let flags = ffi::AV_FRAME_SIDE_DATA_FLAG_NEW_REF;
        // SAFETY: NULL and 0, and the caller's reference, to which C adds
        // one for the entry and which it leaves the caller.
        let e = unsafe {
            ffi::av_frame_side_data_add(
                &raw mut parts.0,
                &raw mut parts.1,
                SideDataType::AFD.into_raw(),
                &raw mut buf,
                flags,
            )
        };
        assert!(!e.is_null() && !buf.is_null());
        // SAFETY: the array C built, its payload initialised.
        let mut a = unsafe { SideDataArray::from_raw_parts(parts.0, parts.1) };
        let mut entries = a.as_mut();
        let mut entry = entries.get_mut(0).unwrap();
        assert!(entry.data_mut().is_none());
        assert_eq!(entry.as_ref().data(), [0x42; 6]);
        // SAFETY: the caller's reference, released once.
        unsafe { ffi::av_buffer_unref(&raw mut buf) };
        entry.data_mut().unwrap()[0] = 1;
        assert_eq!(a.as_ref().get(0).unwrap().data()[..2], [1, 0x42]);
        let fresh = ffmpeg_bindings::libavutil::libavutil::buffer::BufferReference::alloc_zeroed(1)
            .unwrap();
        assert!(
            a.as_mut()
                .add(SideDataType::AFD, fresh, SideDataFlags::NONE)
                .is_err()
        );
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn adopted_arrays_move_to_other_threads() {
        let lens = [10, 20, 0, 40];
        let (sd, nb) = built(&lens);
        // SAFETY: an array C built and nothing else holds.
        let a = unsafe { SideDataArray::from_raw_parts(sd, nb) };
        let a = thread::spawn(move || {
            check(&a, &lens);
            a
        })
        .join()
        .unwrap();
        let (sd, nb) = a.into_raw_parts();
        let addr = (sd as usize, nb);
        thread::spawn(move || {
            // SAFETY: the array given up above, now adopted again here, its
            // only holder.
            let a = unsafe { SideDataArray::from_raw_parts(addr.0 as *mut _, addr.1) };
            check(&a, &lens);
        })
        .join()
        .unwrap();
    }
}
