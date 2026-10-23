//! UB tests for `libavutil/samplefmt.h`'s [`SampleFormat`], through the safe
//! API only: as values, as table indices, and as the plain elements of
//! `av_malloc`-family arrays.

use std::collections::HashSet;
use std::thread;

use ffmpeg_bindings::libavutil::libavutil::ffi;
use ffmpeg_bindings::libavutil::libavutil::mem::{HeapVec, alloc_array, alloc_array_from_fn, alloc_array_zeroed};
use ffmpeg_bindings::libavutil::libavutil::samplefmt::SampleFormat;

/// Named constants, `AV_SAMPLE_FMT_NB` and the `int` extremes.
fn sample() -> Vec<SampleFormat> {
    [i32::MIN, -2, -1, 0, 1, 5, 10, 11, 12, 13, i32::MAX]
        .map(|raw| SampleFormat::from_raw(ffi::AVSampleFormat(raw)))
        .into()
}

#[test]
fn index_and_debug_accept_every_value() {
    // A table sized by `COUNT` takes every index, once per counted format.
    let mut hits = [0; SampleFormat::COUNT];
    for raw in -3..=SampleFormat::COUNT as i32 + 2 {
        let f = SampleFormat::from_raw(ffi::AVSampleFormat(raw));
        if let Some(i) = f.index() {
            hits[i] += 1;
        }
        assert!(!format!("{f:?}").is_empty());
    }
    assert_eq!(hits, [1; SampleFormat::COUNT]);
    for f in sample() {
        if let Some(i) = f.index() {
            assert!(i < SampleFormat::COUNT);
        }
        assert!(!format!("{f:?}").is_empty());
    }
}

#[test]
fn values_hash_and_compare_by_encoding() {
    let set: HashSet<SampleFormat> = sample().into_iter().chain(sample()).collect();
    assert_eq!(set.len(), sample().len());
    for f in sample() {
        let raw: ffi::AVSampleFormat = f.into();
        assert_eq!(SampleFormat::from(raw), f);
        assert!(set.contains(&f));
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn none_terminated_list_stops_at_its_terminator() {
    // The shape of `AVCodec.sample_fmts`: formats up to `NONE`, then storage
    // past it that a reader must not reach.
    let list = [
        SampleFormat::FLTP,
        SampleFormat::S16,
        SampleFormat::from_raw(ffi::AVSampleFormat(99)),
        SampleFormat::NONE,
        SampleFormat::U8,
    ];
    let a = HeapVec::from_slice(&list).unwrap();
    let head: Vec<_> = a
        .as_slice()
        .iter()
        .copied()
        .take_while(|&f| f != SampleFormat::NONE)
        .collect();
    assert_eq!(head, list[..3]);
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn zeroed_array_reads_u8_and_takes_every_value() {
    let values = sample();
    let mut a = alloc_array_zeroed::<SampleFormat>(values.len()).unwrap();
    assert!(a.as_slice().iter().all(|&f| f == SampleFormat::U8));
    a.as_mut_slice().copy_from_slice(&values);
    assert_eq!(a.as_slice(), &values[..]);
    assert!(a.as_slice().get(values.len()).is_none());
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn uninit_and_from_fn_arrays_fill_end_to_end() {
    let values = sample();
    let mut u = alloc_array::<SampleFormat>(values.len()).unwrap();
    for (slot, &f) in u.as_mut_slice().iter_mut().zip(&values) {
        slot.write(f);
    }
    let f = alloc_array_from_fn(values.len(), |i| values[i]).unwrap();
    assert_eq!(f.as_slice(), &values[..]);
    drop(u);
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn clone_is_independent_of_its_source() {
    let values = sample();
    let mut a = HeapVec::from_slice(&values).unwrap();
    let b = a.clone();
    a.as_mut_slice().fill(SampleFormat::NONE);
    drop(a);
    assert_eq!(b.as_slice(), &values[..]);
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn empty_arrays_allocate_and_free() {
    drop(alloc_array_zeroed::<SampleFormat>(0).unwrap());
    drop(alloc_array::<SampleFormat>(0).unwrap());
    drop(HeapVec::<SampleFormat>::from_slice(&[]).unwrap());
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

/// The sample buffer API: every owner and view, at the edges of their
/// planes and of C's `int` counts, through error paths, and across threads.
mod buffers {
    use std::thread;

    use ffmpeg_bindings::libavutil::libavutil::samplefmt::{
        SampleBuffer, SampleBufferWithPointers, SampleFormat, SamplesError, SamplesMut, SamplesRef,
        samples_buffer_size,
    };

    const FORMATS: [SampleFormat; 12] = [
        SampleFormat::U8,
        SampleFormat::S16,
        SampleFormat::S32,
        SampleFormat::FLT,
        SampleFormat::DBL,
        SampleFormat::U8P,
        SampleFormat::S16P,
        SampleFormat::S32P,
        SampleFormat::FLTP,
        SampleFormat::DBLP,
        SampleFormat::S64,
        SampleFormat::S64P,
    ];

    /// Writes every byte of every plane, then runs every operation at the
    /// last samples and just past them.
    fn exercise(mut v: SamplesMut<'_>) {
        let n = v.as_ref().samples();
        for (i, plane) in v.planes_mut().enumerate() {
            plane.fill((i as u8).wrapping_add(1));
        }
        let planes = v.as_ref().plane_count();
        assert!(v.plane_mut(planes).is_none());
        assert!(v.as_ref().plane(planes).is_none());
        for i in 0..planes {
            let p = v.plane_mut(i).unwrap();
            let last = p.len() - 1;
            p[last] = 0xee;
        }
        assert_eq!(v.set_silence(n - 1, 1), Ok(()));
        assert_eq!(v.set_silence(0, n), Ok(()));
        assert_eq!(v.set_silence(n, 0), Ok(()));
        assert_eq!(v.set_silence(n, 1), Err(SamplesError::OutOfRange));
        assert_eq!(v.set_silence(usize::MAX, 2), Err(SamplesError::OutOfRange));
        assert_eq!(v.copy_within(0, 1, n - 1), Ok(()));
        assert_eq!(v.copy_within(1, 0, n - 1), Ok(()));
        assert_eq!(v.copy_within(0, 0, n), Ok(()));
        assert_eq!(v.copy_within(0, 1, n), Err(SamplesError::OutOfRange));
        assert_eq!(v.copy_within(1, 0, n), Err(SamplesError::OutOfRange));
        assert_eq!(
            v.copy_within(0, usize::MAX, 2),
            Err(SamplesError::OutOfRange)
        );
        // Compared as whole slices: a reference per byte is a retag per
        // byte under BSan.
        let silent = v.as_ref().planes().all(|p| p == &vec![p[0]; p.len()][..]);
        assert!(silent);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn every_owner_and_view_stays_inside_its_planes() {
        for f in FORMATS {
            for channels in [1, 2, 5, 9] {
                for samples in [1, 2, 31, 33] {
                    for align in [0, 1, 3, 32] {
                        exercise(
                            SampleBuffer::new(channels, samples, f, align)
                                .unwrap()
                                .as_mut(),
                        );
                        let mut b =
                            SampleBufferWithPointers::new(channels, samples, f, align).unwrap();
                        exercise(b.as_mut());
                        exercise(b.into_buffer().as_mut());
                        // A caller's buffer of exactly the size C needs.
                        let size = samples_buffer_size(channels, samples, f, align)
                            .unwrap()
                            .size;
                        let mut bytes = vec![0u8; size];
                        exercise(
                            SamplesMut::from_buffer(&mut bytes, channels, samples, f, align)
                                .unwrap(),
                        );
                        let r =
                            SamplesRef::from_buffer(&bytes, channels, samples, f, align).unwrap();
                        assert_eq!(
                            r.planes().map(<[u8]>::len).sum::<usize>(),
                            r.plane_count() * r.plane(0).unwrap().len()
                        );
                        assert!(
                            SamplesRef::from_buffer(
                                &bytes[..size - 1],
                                channels,
                                samples,
                                f,
                                align
                            )
                            .is_err()
                        );
                    }
                }
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn copies_between_layouts_stay_inside_both() {
        for f in FORMATS {
            let mut dst = SampleBuffer::new(3, 40, f, 64).unwrap();
            let src = SampleBufferWithPointers::new(3, 7, f, 1).unwrap();
            let mut bytes = vec![0x5a; samples_buffer_size(3, 100, f, 0).unwrap().size];
            let caller = SamplesMut::from_buffer(&mut bytes, 3, 100, f, 0).unwrap();
            assert_eq!(dst.as_mut().copy_from(src.as_ref(), 0, 33, 7), Ok(()));
            assert_eq!(
                dst.as_mut().copy_from(src.as_ref(), 0, 34, 7),
                Err(SamplesError::OutOfRange)
            );
            assert_eq!(
                dst.as_mut().copy_from(src.as_ref(), 1, 0, 7),
                Err(SamplesError::OutOfRange)
            );
            assert_eq!(dst.as_mut().copy_from(caller.as_ref(), 60, 0, 40), Ok(()));
            assert_eq!(
                dst.as_mut().copy_from(caller.as_ref(), 61, 0, 40),
                Err(SamplesError::OutOfRange)
            );
            let other = SampleBuffer::new(2, 40, f, 64).unwrap();
            assert_eq!(
                dst.as_mut().copy_from(other.as_ref(), 0, 0, 1),
                Err(SamplesError::ChannelsMismatch)
            );
        }
        let a = SampleBuffer::new(1, 4, SampleFormat::S16, 1).unwrap();
        let mut b = SampleBuffer::new(1, 4, SampleFormat::U8, 1).unwrap();
        assert_eq!(
            b.as_mut().copy_from(a.as_ref(), 0, 0, 1),
            Err(SamplesError::FormatMismatch)
        );
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn counts_at_c_s_int_limits_are_refused_before_allocating() {
        let max = i32::MAX as usize;
        for f in FORMATS {
            for (c, n, a) in [
                (max, max, 1),
                (1, max, 0),
                (max + 1, 1, 1),
                (1, max + 1, 1),
                (usize::MAX, usize::MAX, u32::MAX),
                (2, 1 << 30, 1),
                (1 << 16, 1 << 16, 1),
            ] {
                assert!(samples_buffer_size(c, n, f, a).is_err(), "{c} {n} {f:?}");
                assert!(SampleBuffer::new(c, n, f, a).is_err());
                assert!(SampleBufferWithPointers::new(c, n, f, a).is_err());
                assert!(SamplesRef::from_buffer(&[], c, n, f, a).is_err());
                assert!(SamplesMut::from_buffer(&mut [0; 16], c, n, f, a).is_err());
            }
        }
        for raw in [-1, 12, 13, i32::MAX, i32::MIN] {
            let f = SampleFormat::from_raw(ffmpeg_bindings::libavutil::libavutil::ffi::AVSampleFormat(raw));
            assert!(SampleBuffer::new(1, 1, f, 1).is_err());
            assert!(SampleBufferWithPointers::new(1, 1, f, 1).is_err());
            assert!(SamplesRef::from_buffer(&[0; 64], 1, 1, f, 1).is_err());
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn many_planar_channels_spill_the_plane_pointers() {
        // More planes than the inline pointer array holds (8), and a C
        // pointer array of 4096 entries; a few dozen under BSan, where the
        // thousands of per-plane slices over one block make each retag
        // cost the block's whole history (>15 min and 50 GB at 4096).
        let channels = if std::env::var("CRUSTIFY_FFMPEG_VARIANT").as_deref() == Ok("bsan") {
            24
        } else {
            4096
        };
        let mut b = SampleBufferWithPointers::new(channels, 3, SampleFormat::S16P, 0).unwrap();
        exercise(b.as_mut());
        let mut c = SampleBuffer::new(channels, 3, SampleFormat::S16P, 0).unwrap();
        assert_eq!(c.as_mut().copy_from(b.as_ref(), 0, 0, 3), Ok(()));
        assert_eq!(c.as_ref().planes().len(), channels);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn owners_cross_and_are_shared_between_threads() {
        let mut b = SampleBuffer::new(4, 64, SampleFormat::FLTP, 0).unwrap();
        b.as_mut().planes_mut().for_each(|p| p.fill(7));
        let w = SampleBufferWithPointers::new(4, 64, SampleFormat::FLTP, 0).unwrap();
        // Shared views read from several threads at once.
        thread::scope(|s| {
            for _ in 0..4 {
                let (r, wr) = (b.as_ref(), w.as_ref());
                s.spawn(move || {
                    assert!(r.planes().flatten().all(|&x| x == 7));
                    assert!(wr.planes().flatten().all(|&x| x == 0));
                });
            }
        });
        // Owners moved to other threads, written and freed there.
        let t = thread::spawn(move || {
            let mut w = w;
            w.as_mut().copy_from(b.as_ref(), 0, 0, 64).unwrap();
            (w.as_ref().plane(3).unwrap().to_vec(), b)
        });
        let (plane, b) = t.join().unwrap();
        assert_eq!(plane, b.as_ref().plane(3).unwrap());
        thread::spawn(move || drop(b)).join().unwrap();
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn disjoint_caller_buffers_are_written_from_threads_at_once() {
        let size = samples_buffer_size(2, 256, SampleFormat::S32P, 0)
            .unwrap()
            .size;
        let mut bytes = vec![1u8; 4 * size];
        let src = SampleBuffer::new(2, 256, SampleFormat::S32P, 0).unwrap();
        thread::scope(|s| {
            for (i, chunk) in bytes.chunks_mut(size).enumerate() {
                let src = src.as_ref();
                s.spawn(move || {
                    let mut v =
                        SamplesMut::from_buffer(chunk, 2, 256, SampleFormat::S32P, 0).unwrap();
                    for _ in 0..20 {
                        v.planes_mut().for_each(|p| p.fill(i as u8));
                        v.copy_within(0, 128, 128).unwrap();
                        v.copy_from(src, 0, 0, 64).unwrap();
                        v.set_silence(200, 56).unwrap();
                    }
                });
            }
        });
        for chunk in bytes.chunks(size) {
            let v = SamplesRef::from_buffer(chunk, 2, 256, SampleFormat::S32P, 0).unwrap();
            assert!(v.planes().all(|p| p[..256].iter().all(|&x| x == 0)));
        }
    }

    /// C rounds the default alignment's `INT_MAX - 31` samples up through a
    /// signed overflow (`FFALIGN(nb_samples, 32)`); the owners get the same
    /// 2 GiB layout without it. Skipped under BSan, whose cost per byte
    /// makes a 2 GiB buffer impractical.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn default_alignment_of_int_max_minus_31_samples_does_not_overflow() {
        if std::env::var("CRUSTIFY_FFMPEG_VARIANT").as_deref() == Ok("bsan") {
            return;
        }
        let n = (i32::MAX - 31) as usize;
        let enomem = -(ffmpeg_bindings::libavutil::libavutil::ffi::ENOMEM as i32);
        match SampleBufferWithPointers::new(1, n, SampleFormat::U8, 0) {
            Ok(mut b) => {
                assert_eq!(b.as_ref().plane(0).unwrap().len(), n);
                assert_eq!(b.as_mut().set_silence(n - 1, 1), Ok(()));
            }
            Err(e) => assert_eq!(e, enomem),
        }
        match SampleBuffer::new(1, n, SampleFormat::U8P, 0) {
            Ok(b) => assert_eq!((b.size(), b.as_ref().linesize()), (n, n)),
            Err(e) => assert_eq!(e, enomem),
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn is_planar_accepts_every_value() {
        for raw in [i32::MIN, -2, -1, 0, 5, 11, 12, 13, i32::MAX] {
            let f = SampleFormat::from_raw(ffmpeg_bindings::libavutil::libavutil::ffi::AVSampleFormat(raw));
            assert_eq!(f.is_planar(), (5..=9).contains(&raw) || raw == 11, "{raw}");
        }
    }
}

/// `SampleFormat::alt`, `packed` and `bytes_per_sample` on every value, the
/// `int` extremes included, and on several threads at once.
mod sample_format_forms {
    use std::thread;

    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::samplefmt::SampleFormat;

    fn values() -> Vec<SampleFormat> {
        (-3..16)
            .chain([i32::MIN, i32::MIN + 1, i32::MAX])
            .map(|r| SampleFormat::from_raw(ffi::AVSampleFormat(r)))
            .collect()
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn every_value_is_accepted() {
        for f in values() {
            for planar in [false, true] {
                if let Some(g) = f.alt(planar) {
                    assert!(g.index().is_some());
                    assert_eq!(g.bytes_per_sample(), f.bytes_per_sample());
                }
            }
            if let Some(p) = f.packed() {
                assert_eq!(p.packed(), Some(p));
            }
            assert!([0, 1, 2, 4, 8].contains(&f.bytes_per_sample()));
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn calls_run_on_many_threads() {
        let all = values();
        let expect: Vec<_> = all
            .iter()
            .map(|f| (f.alt(true), f.alt(false), f.packed(), f.bytes_per_sample()))
            .collect();
        thread::scope(|s| {
            for t in 0..8 {
                let (all, expect) = (&all, &expect);
                s.spawn(move || {
                    for (f, e) in all.iter().zip(expect).cycle().skip(t).take(100) {
                        assert_eq!(
                            &(f.alt(true), f.alt(false), f.packed(), f.bytes_per_sample()),
                            e
                        );
                    }
                });
            }
        });
    }
}

/// UB tests for `SampleFormat`'s C lookups and description line: every
/// value, named or not, into exactly sized heap buffers of every length (so
/// a write past the end is an ASan report), and on several threads.
mod lookups {
    use std::thread;

    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::samplefmt::SampleFormat;

    fn values() -> impl Iterator<Item = SampleFormat> {
        (-3..16)
            .chain([i32::MIN, i32::MAX])
            .map(|raw| SampleFormat::from_raw(ffi::AVSampleFormat(raw)))
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn lookups_of_every_value_stay_in_bounds() {
        for f in values() {
            let _ = f.name();
            let _ = f.planar();
            if let Some(name) = f.name() {
                assert_eq!(SampleFormat::from_name(name), Some(f));
            }
            for len in 0..16 {
                let mut buf = vec![b'#'; len].into_boxed_slice();
                if let Some(s) = f.describe(&mut buf) {
                    assert!(s.to_bytes().len() < len);
                }
            }
        }
        assert_eq!(SampleFormat::from_name(c"x"), None);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn lookups_on_many_threads() {
        thread::scope(|s| {
            for _ in 0..8 {
                s.spawn(|| {
                    let mut buf = [0u8; 16];
                    for f in values() {
                        let _ = f.describe(&mut buf);
                        let _ = f.name();
                    }
                });
            }
        });
    }
}
