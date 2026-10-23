//! UB tests for `libavutil/side_data.c`'s side data arrays, through the safe API
//! only: the array routines, the entries they hold, and the descriptor lookups.
#![expect(
    clippy::cast_possible_truncation,
    reason = "test inputs are chosen within range, and the casts mirror the C conversions under comparison"
)]
#![expect(
    clippy::assert_is_empty,
    let_underscore_drop,
    reason = "assertions keep the form of the C behaviour they check"
)]

/// `SideDataType::descriptor` and `SideDataType::name` (buffer-frame wave 1,
/// batch 1): every value, including those the table has no entry for and
/// those past its end; the `'static` borrows outliving the values and copies
/// they came from, read on other threads, and copied into Rust-held
/// descriptors.
mod side_data_lookups {
    use core::ffi::CStr;
    use std::thread;

    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::frame::{
        SideDataDescriptor, SideDataDescriptorRef, SideDataType,
    };

    fn every_value() -> impl Iterator<Item = SideDataType> {
        (0..=256u32)
            .chain((0..32).map(|s| 1u32 << s))
            .chain([u32::MAX, u32::MAX - 1, i32::MAX as u32])
            .map(|raw| SideDataType::from_raw(ffi::AVFrameSideDataType(raw)))
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn every_value_looks_up_without_reading_past_the_table() {
        for t in every_value() {
            let d = t.descriptor();
            let n = t.name();
            assert_eq!(d.and_then(|d| d.name()), n, "{t:?}");
            if let Some(n) = n {
                // The whole literal, through its NUL.
                assert!(!n.to_bytes().is_empty());
                assert!(n.to_str().is_ok());
            }
        }
    }

    /// The `'static` results outlive the value they came from, and cross
    /// threads both ways.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn static_results_outlive_their_value_and_cross_threads() {
        let found: Vec<(SideDataDescriptorRef<'static>, &'static CStr)> = {
            let values: Vec<SideDataType> = every_value().collect();
            values
                .iter()
                .filter_map(|t| Some((t.descriptor()?, t.name()?)))
                .collect()
        };
        let handles: Vec<_> = (0..4)
            .map(|i| {
                let found = found.clone();
                thread::spawn(move || {
                    let mut copies = Vec::new();
                    for (d, n) in found.iter().skip(i).step_by(4) {
                        assert_eq!(d.name(), Some(*n));
                        let mut own = SideDataDescriptor::new();
                        own.as_mut().set(d.get());
                        own.as_mut().set_name(d.name());
                        copies.push(own);
                    }
                    copies
                })
            })
            .collect();
        let copies: Vec<SideDataDescriptor> = handles
            .into_iter()
            .flat_map(|h| h.join().unwrap())
            .collect();
        assert_eq!(copies.len(), found.len());
        for (d, _) in &found {
            assert!(copies.contains(&d.get()));
        }
    }
}

/// [`SideData`] entries and their [`SideDataArray`] (buffer-frame wave 2,
/// batch 0): every getter over payloads from empty to page-sized, sized
/// exactly so that a read past the end is an `ASan` report; writes through
/// sole, shared and read-only buffers; buffer and metadata replacement with
/// the old owners outliving the array; the array's handles and iterators;
/// and entries read, written, cloned and dropped across threads.
///
/// [`SideData`]: ffmpeg_bindings::libavutil::libavutil::frame::SideData
/// [`SideDataArray`]: ffmpeg_bindings::libavutil::libavutil::side_data::SideDataArray
mod side_data {
    use std::sync::Arc;
    use std::thread;

    use ffmpeg_bindings::libavutil::libavutil::buffer::{BufferReference, BufferReferenceBox};
    use ffmpeg_bindings::libavutil::libavutil::dict::{Dict, Flags};
    use ffmpeg_bindings::libavutil::libavutil::frame::{SideDataRef, SideDataType};
    use ffmpeg_bindings::libavutil::libavutil::side_data::{SideDataArray, SideDataFlags};

    use super::side_data_fixture::{SAMPLE, Spec, array, sample};

    /// Whether every byte of `bytes` is `b`, compared as one slice: `BSan`
    /// retags once per element an iterator yields.
    fn is_all(bytes: &[u8], b: u8) -> bool {
        *bytes == *vec![b; bytes.len()]
    }

    /// The entry's metadata as owned pairs, in iteration order.
    fn pairs(e: SideDataRef<'_>) -> Vec<(String, String)> {
        e.metadata().map_or_else(Vec::new, |m| {
            m.iter()
                .map(|p| {
                    (
                        p.key().to_string_lossy().into(),
                        p.value().to_string_lossy().into(),
                    )
                })
                .collect()
        })
    }

    /// Every getter of every entry, checked against its spec.
    fn check(array: &SideDataArray, specs: &[Spec<'_>]) {
        assert_eq!(array.len(), specs.len());
        for (e, spec) in array.as_ref().iter().zip(specs) {
            assert_eq!(e.kind(), spec.kind);
            assert_eq!(e.len(), spec.payload.len());
            assert_eq!(e.is_empty(), spec.payload.is_empty());
            assert_eq!(e.data(), spec.payload);
            assert_eq!(e.buffer().data(), spec.payload);
            assert!(!format!("{e:?}").is_empty());
            let _ = pairs(e);
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn getters_read_exactly_sized_payloads() {
        let a = sample();
        check(&a, &SAMPLE);
        assert_eq!(
            pairs(a.as_ref().get(1).unwrap()),
            [("k".into(), "v".into())]
        );
        assert_eq!(pairs(a.as_ref().get(0).unwrap()), []);
        assert!(a.as_ref().get(SAMPLE.len()).is_none());
        assert!(a.as_ref().get(usize::MAX).is_none());
        assert!(!format!("{a:?}").is_empty());
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn empty_arrays_lend_nothing() {
        let mut a = SideDataArray::new();
        assert!(a.is_empty() && a.as_ref().is_empty() && a.as_mut().is_empty());
        assert!(a.as_ref().get(0).is_none());
        assert!(a.as_mut().get_mut(0).is_none());
        assert_eq!(a.as_ref().iter().count(), 0);
        assert_eq!(a.as_mut().iter_mut().count(), 0);
        assert_eq!(format!("{a:?}"), "[]");
        let b = SideDataArray::default();
        assert_eq!(b.len(), 0);
        let (sd, nb) = b.into_raw_parts();
        assert!(sd.is_null() && nb == 0);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn iterators_walk_both_ends_and_report_their_length() {
        let mut a = sample();
        let kinds: Vec<_> = SAMPLE.iter().map(|s| s.kind).collect();
        let r = a.as_ref();
        assert_eq!(r.iter().map(|e| e.kind()).collect::<Vec<_>>(), kinds);
        assert_eq!(
            r.iter().rev().map(|e| e.kind()).collect::<Vec<_>>(),
            kinds.iter().rev().copied().collect::<Vec<_>>()
        );
        let mut it = r.iter();
        assert_eq!(it.len(), kinds.len());
        it.next();
        it.next_back();
        assert_eq!(it.len(), kinds.len() - 2);
        assert!(!format!("{it:?}").is_empty());
        let all: Vec<_> = (&a).into_iter().collect();
        assert_eq!(all.len(), kinds.len());
        let mut m = a.as_mut().into_iter();
        assert_eq!(m.len(), kinds.len());
        assert_eq!(
            m.next_back().unwrap().as_ref().kind(),
            *kinds.last().unwrap()
        );
        assert_eq!(m.next().unwrap().as_ref().kind(), kinds[0]);
        assert!(!format!("{m:?}").is_empty());
        assert_eq!(m.by_ref().count(), kinds.len() - 2);
        assert!(m.next_back().is_none() && m.next().is_none());
        for _ in &mut a {}
        let mut it = a.as_ref().iter();
        it.by_ref().for_each(drop);
        assert!(it.next().is_none() && it.next_back().is_none());
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn sole_payloads_are_written_in_full_and_read_back() {
        let mut a = sample();
        for (i, mut e) in a.as_mut().iter_mut().enumerate() {
            let bytes = e.data_mut().expect("C's entries hold sole references");
            bytes.fill(i as u8);
            assert!(!format!("{e:?}").is_empty());
        }
        for (i, e) in a.as_ref().iter().enumerate() {
            assert!(is_all(e.data(), i as u8));
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn shared_and_read_only_buffers_refuse_writes() {
        let mut a = sample();
        let mut entries = a.as_mut();
        let mut e = entries.get_mut(2).unwrap();
        let copy = e.as_ref().buffer().try_clone().unwrap();
        assert!(e.data_mut().is_none());
        drop(copy);
        assert!(e.data_mut().is_some());
        e.set_buffer(BufferReference::from_read_only(&b"read only"[..]).unwrap());
        assert!(e.data_mut().is_none());
        assert_eq!(e.as_ref().data(), b"read only");
        e.set_buffer(BufferReference::from_owner(vec![9u8; 5]).unwrap());
        e.data_mut().unwrap()[4] = 1;
        assert_eq!(e.as_ref().data(), [9, 9, 9, 9, 1]);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn replaced_buffers_outlive_the_array() {
        let mut a = sample();
        let mut old = Vec::new();
        for (i, mut e) in a.as_mut().iter_mut().enumerate() {
            let fresh = BufferReference::alloc(i * 33).unwrap().fill(i as u8);
            old.push(e.replace_buffer(fresh));
            assert_eq!(e.as_ref().len(), i * 33);
        }
        let empty = BufferReference::alloc_zeroed(0).unwrap();
        a.as_mut().get_mut(0).unwrap().set_buffer(empty);
        assert!(a.as_ref().get(0).unwrap().data().is_empty());
        let kept: Vec<BufferReferenceBox> = a
            .as_ref()
            .iter()
            .map(|e| e.buffer().try_clone().unwrap())
            .collect();
        drop(a);
        for (b, spec) in old.iter().zip(&SAMPLE) {
            assert_eq!(b.as_ref().data(), spec.payload);
        }
        for (i, b) in kept.iter().enumerate().skip(1) {
            assert!(is_all(b.as_ref().data(), i as u8));
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn metadata_moves_in_and_out_and_outlives_the_array() {
        let mut a = sample();
        let mut entries = a.as_mut();
        let mut e = entries.get_mut(3).unwrap();
        let taken = e.take_metadata();
        assert!(e.as_ref().metadata().is_none());
        assert_eq!(taken.len(), 3);
        let mut d = Dict::new();
        d.set(c"x", c"y", Flags::NONE).unwrap();
        let none = e.replace_metadata(d);
        assert!(none.is_empty());
        e.set_metadata(taken.try_clone().unwrap());
        e.set_metadata(Dict::new());
        assert!(e.as_ref().metadata().is_none());
        let mut f = entries.get_mut(1).unwrap();
        let kept = f.replace_metadata(taken);
        drop(a);
        assert_eq!(kept.get(c"k", Flags::NONE).unwrap().value(), c"v");
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn additions_with_every_flag_keep_or_return_the_buffer() {
        let flags = [
            SideDataFlags::NONE,
            SideDataFlags::UNIQUE,
            SideDataFlags::REPLACE,
            SideDataFlags::UNIQUE | SideDataFlags::REPLACE,
        ];
        let kinds = [
            SideDataType::AFD,
            SideDataType::SEI_UNREGISTERED,
            SideDataType::from_raw(
                ffmpeg_bindings::libavutil::libavutil::ffi::AVFrameSideDataType(77),
            ),
        ];
        let mut a = sample();
        let mut kept = Vec::new();
        for round in 0..40usize {
            let kind = kinds[round % kinds.len()];
            let f = flags[round / kinds.len() % flags.len()];
            let buf = BufferReference::alloc(round).unwrap().fill(round as u8);
            let copy = buf.as_ref().try_clone().unwrap();
            let mut entries = a.as_mut();
            match entries.add(kind, buf, f) {
                Ok(mut e) => {
                    assert_eq!(e.as_ref().kind(), kind);
                    assert_eq!(e.as_ref().len(), round);
                    assert!(e.as_ref().metadata().is_none());
                    assert!(e.data_mut().is_none());
                    e.set_metadata(Dict::from_entries([(c"round", c"x")]).unwrap());
                }
                Err(back) => {
                    assert!(!f.contains(SideDataFlags::REPLACE));
                    assert!(is_all(back.as_ref().data(), round as u8));
                    kept.push(back);
                }
            }
            kept.push(copy);
            check_consistent(&a);
        }
        drop(a);
        assert!(kept.iter().all(|b| b.as_ref().len() < 40));
    }

    /// Reads every byte and pair of every entry.
    fn check_consistent(a: &SideDataArray) {
        for e in a {
            let _ = e.data().to_vec();
            let _ = pairs(e);
            let _ = e.buffer().data();
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn owner_additions_lend_the_entry_until_the_next_use() {
        let mut a = SideDataArray::new();
        for i in 0..20u8 {
            let buf = BufferReference::alloc(usize::from(i)).unwrap().fill(i);
            let mut e = a
                .add(SideDataType::SEI_UNREGISTERED, buf, SideDataFlags::NONE)
                .unwrap();
            e.data_mut().unwrap().fill(i ^ 0xFF);
            e.set_metadata(Dict::from_entries([(c"i", c"x")]).unwrap());
        }
        let buf = BufferReference::alloc_zeroed(2).unwrap();
        a.add(SideDataType::STEREO_3D, buf, SideDataFlags::UNIQUE)
            .unwrap();
        let again = BufferReference::alloc_zeroed(3).unwrap();
        let back = a
            .add(SideDataType::STEREO_3D, again, SideDataFlags::NONE)
            .unwrap_err();
        assert_eq!(back.as_ref().len(), 3);
        assert_eq!(a.len(), 21);
        check_consistent(&a);
        for (i, e) in a.as_ref().iter().take(20).enumerate() {
            assert!(is_all(e.data(), i as u8 ^ 0xFF));
        }
    }

    #[test]
    fn flags_combine_and_compare() {
        let both = SideDataFlags::UNIQUE | SideDataFlags::REPLACE;
        assert!(both.contains(SideDataFlags::UNIQUE) && both.contains(SideDataFlags::REPLACE));
        assert!(!SideDataFlags::NONE.contains(SideDataFlags::UNIQUE));
        let mut f = SideDataFlags::default();
        f |= SideDataFlags::REPLACE;
        assert_eq!(f, SideDataFlags::REPLACE);
        assert_eq!(both.bits(), 3);
        assert!(!format!("{f:?}").is_empty());
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn kinds_take_any_value() {
        let mut a = sample();
        for raw in [0, 1, 37, 38, 1 << 31, u32::MAX] {
            let kind = SideDataType::from_raw(
                ffmpeg_bindings::libavutil::libavutil::ffi::AVFrameSideDataType(raw),
            );
            for mut e in a.as_mut().iter_mut() {
                e.set_kind(kind);
                assert_eq!(e.as_ref().kind(), kind);
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn big_and_many_entries() {
        let page = vec![0x5A; 1 << 16];
        let specs: Vec<Spec<'_>> = (0..200)
            .map(|i| Spec::new(SideDataType::SEI_UNREGISTERED, &page[..i * 97]))
            .collect();
        let a = array(&specs);
        check(&a, &specs);
    }

    /// Payload bytes Rust writes, then C copies (`av_buffer_ref` sharing in
    /// `try_clone`, the new buffer's window in `set_buffer`), alternated with
    /// Rust views that end before C runs again.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn payload_writes_through_rust_reach_c_readers() {
        let mut a = array(&SAMPLE[2..4]);
        for round in 0..4u8 {
            let mut entries = a.as_mut();
            let mut e = entries.get_mut(usize::from(round % 2)).unwrap();
            e.data_mut().unwrap().fill(round);
            let copy = e.as_ref().buffer().try_clone().unwrap();
            assert!(is_all(copy.as_ref().data(), round));
            assert!(e.data_mut().is_none());
            let fresh = BufferReference::alloc(17)
                .unwrap()
                .copy_from_slice(&[round; 17]);
            let old = e.replace_buffer(fresh);
            drop(copy);
            assert!(is_all(old.as_ref().data(), round));
            e.data_mut().unwrap()[16] = 0xEE;
            assert_eq!(e.as_ref().data()[16], 0xEE);
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn shared_arrays_read_and_clone_buffers_on_many_threads() {
        let a = Arc::new(sample());
        let workers: Vec<_> = (0..8)
            .map(|_| {
                let a = Arc::clone(&a);
                thread::spawn(move || {
                    let mut copies = Vec::new();
                    for _ in 0..50 {
                        for (e, spec) in a.as_ref().as_ref().iter().zip(&SAMPLE) {
                            assert_eq!(e.data(), spec.payload);
                            copies.push(e.buffer().try_clone().unwrap());
                            let _ = pairs(e);
                        }
                    }
                    copies
                })
            })
            .collect();
        let copies: Vec<_> = workers
            .into_iter()
            .flat_map(|w| w.join().unwrap())
            .collect();
        drop(a);
        assert!(
            copies
                .iter()
                .all(|c| !format!("{:?}", c.as_ref()).is_empty())
        );
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn entries_are_written_on_scoped_threads_and_arrays_dropped_on_others() {
        let mut a = sample();
        let mut entries = a.as_mut();
        thread::scope(|s| {
            for (i, mut e) in entries.iter_mut().enumerate() {
                s.spawn(move || {
                    if let Some(bytes) = e.data_mut() {
                        bytes.fill(i as u8 + 1);
                    }
                    e.set_kind(SideDataType::from_raw(
                        ffmpeg_bindings::libavutil::libavutil::ffi::AVFrameSideDataType(i as u32),
                    ));
                });
            }
        });
        let r = a.as_ref();
        thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(move || {
                    for (i, e) in r.iter().enumerate() {
                        assert!(is_all(e.data(), i as u8 + 1));
                    }
                });
            }
        });
        let copy = a.as_ref().get(3).unwrap().buffer().try_clone().unwrap();
        thread::spawn(move || drop(a)).join().unwrap();
        assert_eq!(copy.as_ref().len(), SAMPLE[3].payload.len());
    }
}

/// Side data arrays C builds, filled, for the `side_data` tests.
#[path = "../common/side_data.rs"]
mod side_data_fixture;

/// The side data array routines (buffer-frame wave 3, batch 1): payloads
/// `alloc_zeroed` and `alloc_from_slice` allocate, from empty to page-sized,
/// read and written in full so that a run past the end is an `ASan` report;
/// the replacement whose allocation fails, where C's own routine would
/// dereference NULL; clones outliving their source and the entry they
/// replaced; lookups and removals over every kind and property filter, with
/// references the test keeps outliving the removed entries; and clones,
/// lookups and drops across threads.
mod side_data_array_routines {
    use std::thread;

    use ffmpeg_bindings::libavutil::libavutil::buffer::BufferReferenceBox;
    use ffmpeg_bindings::libavutil::libavutil::dict::Dict;
    use ffmpeg_bindings::libavutil::libavutil::frame::{SideDataProps, SideDataRef, SideDataType};
    use ffmpeg_bindings::libavutil::libavutil::side_data::{SideDataArray, SideDataFlags};

    use super::side_data_fixture::{SAMPLE, array, sample};

    /// Whether every byte of `bytes` is `b`, compared as one slice.
    fn is_all(bytes: &[u8], b: u8) -> bool {
        *bytes == *vec![b; bytes.len()]
    }

    fn kinds(a: &SideDataArray) -> Vec<SideDataType> {
        a.as_ref().iter().map(|e| e.kind()).collect()
    }

    /// The payload sizes the allocations use: empty, odd, and a page.
    const SIZES: [usize; 5] = [0, 1, 7, 64, 4096];

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn allocated_payloads_are_initialised_and_exactly_sized() {
        let mut a = SideDataArray::new();
        let multi = SideDataType::SEI_UNREGISTERED;
        for (i, &n) in SIZES.iter().enumerate() {
            let mut e = a.alloc_zeroed(multi, n, SideDataFlags::NONE).unwrap();
            assert!(is_all(e.as_ref().data(), 0));
            e.data_mut().unwrap().fill(i as u8 + 1);
            let bytes: Vec<u8> = (0..n).map(|k| k as u8).collect();
            let mut entries = a.as_mut();
            let mut e = entries
                .alloc_from_slice(multi, &bytes, SideDataFlags::NONE)
                .unwrap();
            assert_eq!(e.as_ref().data(), bytes);
            e.data_mut().unwrap().fill(0xEE);
        }
        assert_eq!(a.len(), 2 * SIZES.len());
        for (i, &n) in SIZES.iter().enumerate() {
            let e = a.as_ref().get(2 * i).unwrap();
            assert_eq!(e.len(), n);
            assert!(is_all(e.data(), i as u8 + 1));
            assert_eq!(e.buffer().data(), e.data());
            assert!(is_all(a.as_ref().get(2 * i + 1).unwrap().data(), 0xEE));
        }
        assert!(is_all(a.as_ref().find(multi).unwrap().data(), 1));
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn failed_allocations_leave_the_array_readable() {
        let mut a = sample();
        let afd = SideDataType::AFD;
        let mut e = a.find_mut(afd).unwrap();
        e.set_metadata(Dict::from_entries([(c"keep", c"me")]).unwrap());
        for huge in [1 << 40, usize::MAX] {
            // Replacing: C's routine would free the entry's buffer and
            // metadata, then dereference its failed allocation.
            assert!(a.alloc_zeroed(afd, huge, SideDataFlags::REPLACE).is_none());
            assert!(
                a.as_mut()
                    .alloc_zeroed(afd, huge, SideDataFlags::REPLACE)
                    .is_none()
            );
            // Refused, or appended, for lack of memory.
            assert!(a.alloc_zeroed(afd, huge, SideDataFlags::NONE).is_none());
            let multi = SideDataType::SEI_UNREGISTERED;
            assert!(
                a.alloc_zeroed(multi, huge, SideDataFlags::REPLACE)
                    .is_none()
            );
        }
        let e = a.as_ref().find(afd).unwrap();
        assert_eq!(e.data(), SAMPLE[0].payload);
        assert_eq!(e.metadata().unwrap().len(), 1);
        assert_eq!(a.len(), SAMPLE.len());
        assert!(
            a.alloc_zeroed(afd, 1 << 40, SideDataFlags::UNIQUE | SideDataFlags::REPLACE)
                .is_none()
        );
        assert!(a.as_ref().find(afd).is_none());
        assert_eq!(a.len(), SAMPLE.len() - 1);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn replacements_free_what_they_replace() {
        let mut a = sample();
        let icc = SideDataType::ICC_PROFILE;
        let mut e = a.find_mut(icc).unwrap();
        e.set_metadata(Dict::from_entries([(c"a", c"1"), (c"b", c"2")]).unwrap());
        let kept: BufferReferenceBox = e.as_ref().buffer().try_clone().unwrap();
        for (i, &n) in SIZES.iter().enumerate() {
            let flags = SideDataFlags::REPLACE;
            let e = if i % 2 == 0 {
                a.alloc_zeroed(icc, n, flags).unwrap()
            } else {
                a.alloc_from_slice(icc, &vec![i as u8; n], flags).unwrap()
            };
            assert_eq!(e.as_ref().len(), n);
            assert!(e.as_ref().metadata().is_none());
        }
        assert_eq!(kinds(&a), SAMPLE.map(|s| s.kind));
        assert_eq!(kept.as_ref().data(), SAMPLE[4].payload);
        assert_eq!(kept.as_ref().ref_count(), 1);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn clones_outlive_their_source() {
        let mut src = sample();
        let mut dst = SideDataArray::new();
        for e in src.as_ref() {
            let flags = SideDataFlags::REPLACE;
            dst.as_mut().add_clone(e, flags).unwrap();
        }
        // Every entry is shared now, so neither side writes.
        assert!(src.as_mut().iter_mut().all(|mut e| e.data_mut().is_none()));
        // A unique kind present: refused without REPLACE.
        let afd = src.as_ref().find(SideDataType::AFD).unwrap();
        assert!(dst.as_mut().add_clone(afd, SideDataFlags::NONE).is_err());
        drop(src);
        for (e, spec) in dst.as_ref().iter().zip(&SAMPLE) {
            assert_eq!(e.kind(), spec.kind);
            assert_eq!(e.data(), spec.payload);
        }
        let mut sei = dst.find_mut(SideDataType::SEI_UNREGISTERED).unwrap();
        sei.data_mut().expect("the source is gone").fill(1);
        assert_eq!(dst.len(), SAMPLE.len());
        let page = dst.as_ref().get(3).unwrap();
        assert_eq!(page.metadata().unwrap().len(), 3);
        assert_eq!(page.data(), SAMPLE[3].payload);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn clones_replacing_entries_free_their_buffer_and_metadata() {
        let src = array(&[SAMPLE[1]]);
        let mut dst = sample();
        let a53 = SideDataType::A53_CC;
        let mut e = dst.find_mut(a53).unwrap();
        // Sharing the source's buffer first: C's replacement then only
        // takes its window.
        e.set_buffer(src.as_ref().get(0).unwrap().buffer().try_clone().unwrap());
        let entry = src.as_ref().get(0).unwrap();
        for flags in [
            SideDataFlags::REPLACE,
            SideDataFlags::UNIQUE,
            SideDataFlags::UNIQUE | SideDataFlags::REPLACE,
        ] {
            dst.as_mut().add_clone(entry, flags).unwrap();
            let e = dst.as_ref().find(a53).unwrap();
            assert!(e.data().is_empty());
            assert_eq!(e.metadata().unwrap().len(), 1);
        }
        drop(src);
        assert_eq!(dst.as_ref().find(a53).unwrap().buffer().ref_count(), 1);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn removals_keep_the_rest_and_what_the_caller_holds() {
        let mut a = sample();
        let held: Vec<BufferReferenceBox> = a
            .as_ref()
            .iter()
            .map(|e| e.buffer().try_clone().unwrap())
            .collect();
        a.as_mut().remove(SideDataType::SEI_UNREGISTERED);
        assert_eq!(a.len(), SAMPLE.len() - 2);
        assert!(a.as_ref().find(SideDataType::SEI_UNREGISTERED).is_none());
        a.as_mut().remove(SideDataType::SEI_UNREGISTERED);
        a.as_mut().remove_by_props(SideDataProps::GLOBAL);
        assert!(a.as_ref().find(SideDataType::ICC_PROFILE).is_none());
        a.as_mut()
            .remove_by_props(SideDataProps::from_bits_retain(u32::MAX));
        // AFD and A53_CC have no properties, the last kind no descriptor.
        assert_eq!(a.len(), 3);
        for e in a.as_ref() {
            let spec = SAMPLE.iter().find(|s| s.kind == e.kind()).unwrap();
            assert_eq!(e.data(), spec.payload);
        }
        for k in [SideDataType::AFD, SideDataType::A53_CC, SAMPLE[5].kind] {
            a.as_mut().remove(k);
        }
        assert!(a.is_empty());
        a.as_mut().remove(SideDataType::AFD);
        a.as_mut().remove_by_props(SideDataProps::all());
        for (b, spec) in held.iter().zip(&SAMPLE) {
            assert_eq!(b.as_ref().data(), spec.payload);
            assert_eq!(b.as_ref().ref_count(), 1);
        }
        assert!(
            a.alloc_zeroed(SideDataType::AFD, 1, SideDataFlags::NONE)
                .is_some()
        );
        let mut empty = SideDataArray::new();
        empty.as_mut().remove(SideDataType::AFD);
        empty.as_mut().remove_by_props(SideDataProps::all());
        assert!(empty.as_ref().find(SideDataType::AFD).is_none());
        assert!(empty.find_mut(SideDataType::AFD).is_none());
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn lookups_find_the_first_entry_of_a_kind() {
        let mut a = sample();
        for (i, spec) in SAMPLE.iter().enumerate() {
            let first = SAMPLE.iter().position(|s| s.kind == spec.kind).unwrap();
            let found: SideDataRef<'_> = a.as_ref().find(spec.kind).unwrap();
            assert!(found.data() == SAMPLE[first].payload, "{i}");
        }
        let r = a.as_ref();
        let afd = r.find(SideDataType::AFD).unwrap();
        let again = r.find(SideDataType::AFD).unwrap();
        assert_eq!(afd.data(), again.data());
        let mut entries = a.as_mut();
        let mut e = entries
            .find_mut(SideDataType::from_raw(
                ffmpeg_bindings::libavutil::libavutil::ffi::AVFrameSideDataType(u32::MAX),
            ))
            .unwrap();
        e.data_mut().unwrap().fill(0);
        assert!(entries.find_mut(SideDataType::DISPLAY_MATRIX).is_none());
        assert!(is_all(a.as_ref().get(5).unwrap().data(), 0));
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn clones_and_lookups_cross_threads() {
        let src = sample();
        let r = src.as_ref();
        let arrays: Vec<SideDataArray> = thread::scope(|s| {
            let workers: Vec<_> = (0..4)
                .map(|_| {
                    s.spawn(move || {
                        let mut a = SideDataArray::new();
                        for e in r {
                            a.as_mut().add_clone(e, SideDataFlags::NONE).unwrap();
                            assert!(r.find(e.kind()).is_some());
                        }
                        a
                    })
                })
                .collect();
            workers.into_iter().map(|w| w.join().unwrap()).collect()
        });
        assert_eq!(r.get(0).unwrap().buffer().ref_count(), 5);
        thread::spawn(move || drop(src)).join().unwrap();
        thread::scope(|s| {
            for mut a in arrays {
                s.spawn(move || {
                    assert_eq!(
                        a.as_ref().find(SideDataType::ICC_PROFILE).unwrap().data(),
                        SAMPLE[4].payload
                    );
                    a.as_mut().remove_by_props(SideDataProps::COLOR_DEPENDENT);
                    a.as_mut().remove(SideDataType::AFD);
                    drop(a);
                });
            }
        });
    }

    /// Clones replacing an entry in place whose metadata copy fails part
    /// way, at every `av_max_alloc` limit up to one that copies it all:
    /// C's own path leaks the entries it already copied (`LeakSanitizer`, at
    /// the isolated child's exit), the wrapper frees them and leaves the
    /// entry as it was (buffer-frame wave-3 symbol review,
    /// `ub_safe/side-data-clone-replace-leaks-partial-metadata`).
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn in_place_clones_free_a_partial_metadata_copy() {
        let name = "frame::side_data_array_routines::in_place_clones_free_a_partial_metadata_copy";
        if !crate::log::isolated(name) {
            return;
        }
        let set_max = ffmpeg_bindings::libavutil::libavutil::mem::set_max_alloc;
        let afd = SideDataType::AFD;
        let mut src = SideDataArray::new();
        let mut e = src
            .alloc_from_slice(afd, b"src", SideDataFlags::NONE)
            .unwrap();
        let pairs = [(c"a", c"1"), (c"b", c"2"), (c"c", c"3"), (c"d", c"4")];
        e.set_metadata(Dict::from_entries(pairs).unwrap());
        let src = src.as_ref().get(0).unwrap();
        let mut failed = 0;
        for limit in 0..=96 {
            let mut dst = SideDataArray::new();
            let mut old = dst
                .alloc_from_slice(afd, b"old", SideDataFlags::NONE)
                .unwrap();
            old.set_metadata(Dict::from_entries([(c"old", c"x")]).unwrap());
            set_max(limit);
            let ret = dst.as_mut().add_clone(src, SideDataFlags::REPLACE);
            set_max(i32::MAX as usize);
            let e = dst.as_ref().get(0).unwrap();
            assert_eq!(dst.len(), 1, "limit {limit}");
            if let Ok(()) = ret {
                assert_eq!(e.data(), b"src", "limit {limit}");
                assert_eq!(e.metadata().unwrap().len(), pairs.len(), "limit {limit}");
            } else {
                failed += 1;
                assert_eq!(e.data(), b"old", "limit {limit}");
                assert_eq!(e.metadata().unwrap().len(), 1, "limit {limit}");
                assert_eq!(e.buffer().ref_count(), 1, "limit {limit}");
            }
        }
        // Copies fail after one, two and three entries on the way.
        assert!(failed > 3 && failed <= 96, "{failed}");
        assert_eq!(src.buffer().ref_count(), 1);
    }

    /// Rust reborrows of an array's entries interleaved with C writing the
    /// array and reading the bytes Rust wrote: C grows and shrinks the
    /// pointer block, moves entries within it, writes the entries it
    /// replaces and the counts of shared buffers, while handles and views
    /// Rust took before are re-derived after each call (the `BSan` target of
    /// this batch).
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn foreign_writes_across_reborrows() {
        let mut src = sample();
        let mut a = SideDataArray::new();
        for round in 0..3u8 {
            let mut e = a
                .alloc_zeroed(SideDataType::SEI_UNREGISTERED, 16, SideDataFlags::NONE)
                .unwrap();
            e.data_mut().unwrap().fill(round);
            // The round's own entry: the last round's went with MULTI.
            let view = a
                .as_ref()
                .find(SideDataType::SEI_UNREGISTERED)
                .unwrap()
                .data();
            assert!(is_all(view, round));
            for e in src.as_ref() {
                let _ = a.as_mut().add_clone(e, SideDataFlags::REPLACE);
            }
            let mut afd = a.find_mut(SideDataType::AFD).unwrap();
            assert!(afd.data_mut().is_none());
            let _ = afd.as_ref().data();
            let mut replaced = a
                .alloc_from_slice(SideDataType::AFD, &[round; 2], SideDataFlags::REPLACE)
                .unwrap();
            replaced.data_mut().unwrap()[1] = 0xFF;
            a.as_mut().remove(SideDataType::ICC_PROFILE);
            src.find_mut(SideDataType::AFD)
                .unwrap()
                .set_kind(SideDataType::AFD);
            a.as_mut().remove_by_props(SideDataProps::MULTI);
            assert_eq!(
                a.as_ref().find(SideDataType::AFD).unwrap().data(),
                [round, 0xFF]
            );
        }
        drop(src);
        assert!(a.as_mut().iter_mut().all(|mut e| e.data_mut().is_some()));
    }
}
