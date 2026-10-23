//! `libavutil/buffer.h`'s safe pool owner: construction at every size, its
//! handles and reborrows, and its moves and shares across threads.

use std::sync::Barrier;
use std::thread;

use ffmpeg_bindings::libavutil::libavutil::buffer::{BufferPool, BufferPoolBox, BufferPoolRef};

/// Sizes from empty to the largest `size_t`: C allocates no buffer until one
/// is requested, so each builds a pool.
const SIZES: [usize; 7] = [0, 1, 63, 4096, 1 << 30, usize::MAX / 2, usize::MAX];

fn take<T>(_: T) {}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn every_size_builds_and_releases_a_pool() {
    for size in SIZES {
        for _ in 0..4 {
            let pool = BufferPool::new(size).unwrap();
            drop(pool);
        }
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn shared_handles_and_exclusive_reborrows() {
    let mut pool = BufferPool::new(64).unwrap();
    let shared = pool.as_ref();
    let copy = shared;
    take(shared);
    take(copy);
    let mut exclusive = pool.as_mut();
    let reborrowed = exclusive.as_mut();
    take(reborrowed.as_ref());
    take(reborrowed);
    take(exclusive.as_ref());
    take(exclusive);
    // Many owners at once, released out of creation order.
    let mut pools: Vec<BufferPoolBox> =
        SIZES.iter().map(|&s| BufferPool::new(s).unwrap()).collect();
    pools.swap(0, 4);
    while let Some(p) = pools.pop() {
        take(p.as_ref());
        drop(p);
        if let Some(first) = pools.first_mut() {
            take(first.as_mut());
        }
    }
    drop(pool);
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn owners_move_to_and_drop_on_other_threads() {
    let pools: Vec<BufferPoolBox> = (0..8).map(|i| BufferPool::new(i * 32).unwrap()).collect();
    let handles: Vec<_> = pools
        .into_iter()
        .map(|pool| thread::spawn(move || drop(pool)))
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    // Built on one thread each and dropped on another.
    let built: Vec<BufferPoolBox> = (0..8)
        .map(|i| thread::spawn(move || BufferPool::new(i).unwrap()))
        .map(|h| h.join().unwrap())
        .collect();
    thread::scope(|s| {
        for pool in built {
            s.spawn(move || drop(pool));
        }
    });
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn shared_handles_cross_threads_while_the_owner_lives() {
    let pool = BufferPool::new(128).unwrap();
    let barrier = Barrier::new(8);
    thread::scope(|s| {
        for _ in 0..8 {
            let shared: BufferPoolRef<'_> = pool.as_ref();
            let (pool, barrier) = (&pool, &barrier);
            s.spawn(move || {
                barrier.wait();
                take(shared);
                take(pool.as_ref());
            });
        }
    });
    drop(pool);
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn concurrent_construction_and_release() {
    let barrier = Barrier::new(8);
    thread::scope(|s| {
        for t in 0..8 {
            let barrier = &barrier;
            s.spawn(move || {
                barrier.wait();
                for i in 0..200 {
                    let pool = BufferPool::new(SIZES[(t + i) % SIZES.len()]).unwrap();
                    take(pool.as_ref());
                }
            });
        }
    });
}

/// Buffer-frame wave 0, symbol batch 5: `av_buffer_default_free`'s release
/// of every array shape, on any thread.
mod default_release {
    use std::mem::MaybeUninit;
    use std::thread;

    use ffmpeg_bindings::libavutil::libavutil::buffer::default_free;
    use ffmpeg_bindings::libavutil::libavutil::mem::{HeapVec, alloc_array, alloc_array_zeroed};

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn default_free_releases_every_array_shape() {
        let filled = HeapVec::from_slice(&[0xa5u8; 4096]).unwrap();
        assert_eq!(filled.as_slice()[4095], 0xa5);
        default_free(filled);
        default_free(alloc_array::<u8>(17).unwrap());
        default_free(alloc_array_zeroed::<u64>(3).unwrap());
        default_free(HeapVec::<u8>::empty());
        default_free(HeapVec::<MaybeUninit<u8>>::empty());
        for len in [0, 1, 63, 64, 65] {
            default_free(alloc_array_zeroed::<u8>(len).unwrap_or_else(HeapVec::empty));
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn default_free_releases_on_another_thread() {
        let arrays: Vec<_> = (1..64)
            .map(|n| HeapVec::from_slice(&vec![n as u8; n]).unwrap())
            .collect();
        thread::spawn(move || arrays.into_iter().for_each(default_free))
            .join()
            .unwrap();
    }
}

/// Buffer-frame wave 1, batch 0: `BufferReference` (`AVBufferRef`) owners
/// from every constructor, at sizes from empty to 64 KiB, each read exactly
/// (so a read past a window is an ASan report); writes only through a sole
/// writable reference; clones, narrowed windows and swapped references;
/// shared handles and owners across threads with the original dropped first;
/// and Rust owners released once, by the last reference, on whichever thread
/// drops it.
mod reference {
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Barrier};
    use std::thread;

    use ffmpeg_bindings::libavutil::libavutil::buffer::{
        BufferReference, BufferReferenceBox, BufferReferenceRef, UninitBuffer,
    };
    use ffmpeg_bindings::libavutil::libavutil::mem::HeapVec;

    const SIZES: [usize; 6] = [0, 1, 7, 64, 4097, 1 << 16];

    /// A byte cycle in which neighbours differ, built at compile time.
    const CYCLE: [u8; 256] = {
        let mut t = [0u8; 256];
        let mut i = 0;
        while i < 256 {
            t[i] = (i as u8).wrapping_mul(31) ^ 0x5c;
            i += 1;
        }
        t
    };

    /// `len` bytes of the cycle from offset `seed`, copied a slice at a
    /// time (one BSan retag per copy, not per byte).
    pub(super) fn pattern(len: usize, seed: u8) -> Vec<u8> {
        let mut v = Vec::with_capacity(len);
        let mut at = usize::from(seed);
        while v.len() < len {
            let n = (len - v.len()).min(256 - at);
            v.extend_from_slice(&CYCLE[at..at + n]);
            at = 0;
        }
        v
    }

    /// Bytes that count the drops of their owner.
    #[derive(Debug)]
    struct Counted {
        bytes: Vec<u8>,
        drops: Arc<AtomicUsize>,
    }

    impl AsMut<[u8]> for Counted {
        fn as_mut(&mut self) -> &mut [u8] {
            &mut self.bytes
        }
    }

    impl AsRef<[u8]> for Counted {
        fn as_ref(&self) -> &[u8] {
            &self.bytes
        }
    }

    impl Drop for Counted {
        fn drop(&mut self) {
            self.drops.fetch_add(1, Ordering::SeqCst);
        }
    }

    /// One reference of `size` bytes from each constructor, with the bytes
    /// it must read and whether it is writable.
    pub(super) fn every_constructor(size: usize) -> Vec<(BufferReferenceBox, Vec<u8>, bool)> {
        let p = pattern(size, size as u8);
        let heap = HeapVec::from_slice(&p).unwrap_or_else(HeapVec::empty);
        vec![
            (
                BufferReference::alloc_zeroed(size).unwrap(),
                vec![0; size],
                true,
            ),
            (
                BufferReference::alloc(size).unwrap().fill(0x5a),
                vec![0x5a; size],
                true,
            ),
            (
                BufferReference::alloc(size).unwrap().copy_from_slice(&p),
                p.clone(),
                true,
            ),
            (
                BufferReference::from_heap_vec(heap).unwrap(),
                p.clone(),
                true,
            ),
            (
                BufferReference::from_owner(p.clone()).unwrap(),
                p.clone(),
                true,
            ),
            (
                BufferReference::from_owner(p.clone().into_boxed_slice()).unwrap(),
                p.clone(),
                true,
            ),
            (
                BufferReference::from_read_only(Arc::<[u8]>::from(p.clone())).unwrap(),
                p.clone(),
                false,
            ),
            (
                BufferReference::from_read_only(p.clone()).unwrap(),
                p,
                false,
            ),
        ]
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn every_constructor_reads_exactly_its_bytes() {
        for size in SIZES {
            for (buf, want, _) in every_constructor(size) {
                let r = buf.as_ref();
                assert_eq!((r.len(), r.is_empty()), (size, size == 0));
                assert!(r.data() == want.as_slice());
                assert!(r.buffer().ptr_eq(&r.buffer()));
            }
        }
        // Inline and static owners, empty ones included.
        let inline = BufferReference::from_owner([7u8; 33]).unwrap();
        assert!(inline.as_ref().data() == [7u8; 33].as_slice());
        let empty = BufferReference::from_owner([0u8; 0]).unwrap();
        assert!(empty.as_ref().data().is_empty());
        let lit = BufferReference::from_read_only(&b"static bytes"[..]).unwrap();
        assert_eq!(lit.as_ref().data(), b"static bytes");
        let none = BufferReference::from_heap_vec(HeapVec::empty()).unwrap();
        assert!(none.as_ref().is_empty());
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn writes_only_through_a_sole_writable_reference() {
        for size in SIZES {
            for (mut buf, _, writable) in every_constructor(size) {
                let fill = vec![0xc3; size];
                match buf.as_mut().data_mut() {
                    Some(bytes) => bytes.copy_from_slice(&fill),
                    None => assert!(!writable),
                }
                let copy = buf.clone();
                assert!(buf.as_mut().data_mut().is_none());
                let mut copy2 = copy.as_ref().try_clone().unwrap();
                assert!(copy2.as_mut().data_mut().is_none());
                assert!(copy.as_ref().buffer().ptr_eq(&buf.as_ref().buffer()));
                assert!(copy2.as_ref().data() == buf.as_ref().data());
                drop(copy);
                drop(copy2);
                if writable {
                    assert!(buf.as_ref().data() == fill.as_slice());
                    let mut m = buf.as_mut();
                    let bytes = m.data_mut().unwrap();
                    bytes.copy_from_slice(&pattern(size, 1));
                    assert!(buf.as_ref().data() == pattern(size, 1).as_slice());
                } else {
                    assert!(buf.as_mut().data_mut().is_none());
                }
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn clones_outlive_the_original_on_other_threads() {
        for size in [0, 3, 4097] {
            for (buf, want, _) in every_constructor(size) {
                let clones: Vec<_> = (0..4).map(|_| buf.clone()).collect();
                drop(buf);
                let want = Arc::new(want);
                let handles: Vec<_> = clones
                    .into_iter()
                    .map(|c| {
                        let want = want.clone();
                        thread::spawn(move || {
                            assert!(c.as_ref().data() == want.as_slice());
                            drop(c);
                        })
                    })
                    .collect();
                for h in handles {
                    h.join().unwrap();
                }
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn concurrent_clones_reads_and_releases_of_shared_references() {
        const THREADS: usize = 8;
        let mut buf = BufferReference::from_owner(pattern(256, 9)).unwrap();
        let barrier = Barrier::new(THREADS);
        thread::scope(|s| {
            for t in 0..THREADS {
                let (shared, barrier): (BufferReferenceRef<'_>, _) = (buf.as_ref(), &barrier);
                let owner = &buf;
                s.spawn(move || {
                    barrier.wait();
                    let mut kept = Vec::new();
                    for i in 0..200 {
                        let c = if i % 2 == 0 {
                            shared.try_clone().unwrap()
                        } else {
                            owner.clone()
                        };
                        assert_eq!(c.as_ref().data()[255], pattern(256, 9)[255]);
                        kept.push(c);
                        if (i + t) % 5 == 0 {
                            kept.clear();
                        }
                    }
                });
            }
        });
        // Every clone is gone: the reference is sole again.
        assert!(buf.as_mut().data_mut().is_some());
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn rust_owners_are_released_once_by_the_last_reference() {
        for read_only in [false, true] {
            let drops = Arc::new(AtomicUsize::new(0));
            let owner = Counted {
                bytes: pattern(100, 4),
                drops: drops.clone(),
            };
            let buf = if read_only {
                BufferReference::from_read_only(owner).unwrap()
            } else {
                BufferReference::from_owner(owner).unwrap()
            };
            let clones: Vec<_> = (0..3).map(|_| buf.clone()).collect();
            drop(buf);
            for c in clones.iter().skip(1) {
                assert!(c.as_ref().data() == pattern(100, 4).as_slice());
            }
            let mut clones = clones.into_iter();
            let last = clones.next().unwrap();
            drop(clones);
            assert_eq!(drops.load(Ordering::SeqCst), 0);
            thread::spawn(move || drop(last)).join().unwrap();
            assert_eq!(drops.load(Ordering::SeqCst), 1);
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn narrowed_windows_stay_inside_their_bytes() {
        for size in [0, 1, 9, 4097] {
            for (mut buf, want, _) in every_constructor(size) {
                let mut m = buf.as_mut();
                m.truncate(size + 1);
                m.advance(0);
                assert_eq!(m.as_ref().len(), size);
                let cut = size / 3;
                m.advance(cut);
                assert!(m.as_ref().data() == &want[cut..]);
                m.truncate((size - cut) / 2);
                let end = cut + (size - cut) / 2;
                assert!(m.as_ref().data() == &want[cut..end]);
                // Past the end: a panic, and the window stays.
                let len = m.as_ref().len();
                assert!(catch_unwind(AssertUnwindSafe(|| m.advance(len + 1))).is_err());
                assert!(buf.as_ref().data() == &want[cut..end]);
                // A clone keeps the window; narrowing it leaves the original.
                let mut c = buf.clone();
                assert!(c.as_ref().data() == buf.as_ref().data());
                c.as_mut().advance(len);
                assert!(c.as_ref().is_empty());
                assert_eq!(buf.as_ref().len(), len);
                drop(buf);
                c.as_mut().truncate(0);
                assert!(c.as_ref().data().is_empty());
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn replaced_references_move_whole() {
        let drops = Arc::new(AtomicUsize::new(0));
        let counted = |seed| Counted {
            bytes: pattern(48, seed),
            drops: drops.clone(),
        };
        let mut a = BufferReference::from_owner(counted(1)).unwrap();
        let b = BufferReference::from_owner(counted(2)).unwrap();
        let b_copy = b.clone();
        let old = a.as_mut().replace(b);
        assert!(a.as_ref().data() == pattern(48, 2).as_slice());
        assert!(a.as_ref().buffer().ptr_eq(&b_copy.as_ref().buffer()));
        assert!(old.as_ref().data() == pattern(48, 1).as_slice());
        assert_eq!(drops.load(Ordering::SeqCst), 0);
        // `set` releases what it replaces: the only reference to `1`.
        let mut old = old;
        old.as_mut().set(a.clone());
        assert_eq!(drops.load(Ordering::SeqCst), 1);
        // Replacing a reference by a clone of itself keeps its buffer.
        let same = old.clone();
        old.as_mut().set(same);
        assert!(old.as_ref().buffer().ptr_eq(&a.as_ref().buffer()));
        drop((a, b_copy, old));
        assert_eq!(drops.load(Ordering::SeqCst), 2);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn uninitialised_buffers_fill_in_place() {
        for size in SIZES {
            let mut u = BufferReference::alloc(size).unwrap();
            assert_eq!((u.len(), u.is_empty()), (size, size == 0));
            assert_eq!(u.as_slice().len(), size);
            // Element writes are one BSan retag each: keep them short.
            let (head, tail) = u.as_mut_slice().split_at_mut(size.min(64) / 2);
            head.fill(std::mem::MaybeUninit::new(1));
            tail[..size.min(64) / 2].fill(std::mem::MaybeUninit::new(2));
            let buf = u.fill(3);
            assert!(buf.as_ref().data() == vec![3; size].as_slice());
            // Back to uninitialised while sole, then refilled.
            let u = UninitBuffer::try_from(buf).unwrap();
            let buf = u.copy_from_slice(&pattern(size, 5));
            assert!(buf.as_ref().data() == pattern(size, 5).as_slice());
            let copy = buf.clone();
            let buf = UninitBuffer::try_from(buf).unwrap_err();
            drop(copy);
            assert!(UninitBuffer::try_from(buf).is_ok());
        }
        let u = BufferReference::alloc(4).unwrap();
        assert!(catch_unwind(AssertUnwindSafe(|| u.copy_from_slice(&[0; 3]))).is_err());
        assert!(format!("{:?}", BufferReference::alloc(2).unwrap()).contains("len: 2"));
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn handles_reborrow_and_cross_threads() {
        let mut buf = BufferReference::alloc_zeroed(32).unwrap();
        let shared = buf.as_ref();
        let copy = shared;
        thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(move || assert!(copy.data() == [0u8; 32].as_slice()));
            }
        });
        assert_eq!(shared.len(), 32);
        let mut m = buf.as_mut();
        let mut again = m.as_mut();
        again.data_mut().unwrap()[31] = 1;
        assert_eq!(again.as_ref().data()[31], 1);
        thread::scope(|s| {
            s.spawn(move || {
                again.truncate(31);
                assert!(again.data_mut().is_some());
            });
        });
        assert_eq!(m.as_ref().len(), 31);
        assert!(format!("{:?}", buf.as_ref()).contains("len: 31"));
        // Owners built on one thread each and dropped on another.
        let built: Vec<BufferReferenceBox> = (0..8)
            .map(|i| thread::spawn(move || BufferReference::from_owner(vec![i as u8; i]).unwrap()))
            .map(|h| h.join().unwrap())
            .collect();
        thread::scope(|s| {
            for b in built {
                s.spawn(move || drop(b));
            }
        });
    }

    /// The BSan target: bytes written and read through Rust views between C's
    /// accesses to the same allocations (C copies a reference's struct, holds
    /// the bytes' pointer, swaps nothing but frees the struct, and releases the
    /// bytes: `av_free` for heap data, a Rust drop for owners).
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn foreign_pointers_across_rust_reborrows() {
        let drops = Arc::new(AtomicUsize::new(0));
        let owner = Counted {
            bytes: pattern(512, 3),
            drops: drops.clone(),
        };
        let heap = HeapVec::from_slice(&pattern(512, 4)).unwrap();
        for mut buf in [
            BufferReference::from_owner(owner).unwrap(),
            BufferReference::from_heap_vec(heap).unwrap(),
            BufferReference::alloc(512).unwrap().fill(1),
        ] {
            buf.as_mut().data_mut().unwrap()[..256].copy_from_slice(&[0xab; 256]);
            let mut copy = buf.clone();
            assert!(copy.as_ref().data()[..256] == [0xab; 256]);
            copy.as_mut().advance(128);
            assert!(copy.as_mut().data_mut().is_none());
            drop(copy);
            let mut m = buf.as_mut();
            let bytes = m.data_mut().unwrap();
            bytes[256..].copy_from_slice(&[0xcd; 256]);
            let mut other = BufferReference::alloc_zeroed(16).unwrap();
            let old = other.as_mut().replace(buf);
            assert!(other.as_ref().data()[256..] == [0xcd; 256]);
            drop(old);
            other.as_mut().data_mut().unwrap()[..2].copy_from_slice(&[1, 2]);
            drop(other);
        }
        assert_eq!(drops.load(Ordering::SeqCst), 1);
    }
}

/// Buffer-frame wave 2, batch 1: buffers taken from pools at every size,
/// filled and read exactly (a read past one is an ASan report), released
/// before and after their pool's owner, from many threads at once;
/// references' counts, writability and opaques read while other threads
/// clone and release them; and the mutators that change a reference's
/// buffer in place (`make_mut`, `resize`, `assign` and its slot form) over
/// every constructor's references, sole or shared, read-only or not, their
/// windows advanced or not, each result read exactly.
mod mutation {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Barrier};
    use std::thread;

    use ffmpeg_bindings::libavutil::libavutil::buffer::{self, BufferPool, BufferReference, BufferReferenceBox};

    use super::reference::{every_constructor, pattern};

    const SIZES: [usize; 6] = [0, 1, 7, 64, 4097, 1 << 16];

    /// `want[..len]`, extended to `len` bytes of `fill`.
    fn resized(want: &[u8], len: usize, fill: u8) -> Vec<u8> {
        let mut v = want[..len.min(want.len())].to_vec();
        v.resize(len, fill);
        v
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn pool_buffers_at_every_size_fill_and_read_exactly() {
        for size in SIZES {
            let pool = BufferPool::new(size).unwrap();
            let mut kept = Vec::new();
            for round in 0..4u8 {
                let u = pool.as_ref().get().unwrap();
                assert_eq!((u.len(), u.is_empty()), (size, size == 0));
                let at = u.as_slice().as_ptr();
                let buf = u.copy_from_slice(&pattern(size, round));
                assert!(buf.as_ref().data() == pattern(size, round).as_slice());
                assert_eq!(buf.as_ref().ref_count(), 1);
                assert!(buf.as_ref().is_writable() && !buf.as_ref().opaque().is_null());
                if round % 2 == 0 {
                    // Back to the pool: the next request reuses its bytes.
                    drop(buf);
                    let (again, opaque) = pool.as_ref().get_with_opaque().unwrap();
                    assert!(opaque.is_null());
                    assert_eq!(again.as_slice().as_ptr(), at);
                    kept.push(again.fill(round));
                } else {
                    kept.push(buf);
                }
            }
            // The buffers keep the pool alive past its owner.
            drop(pool);
            for (i, buf) in kept.iter_mut().enumerate() {
                let copy = buf.clone();
                assert!(!buf.as_ref().is_writable());
                assert!(copy.as_ref().data() == buf.as_ref().data());
                drop(copy);
                buf.as_mut().data_mut().unwrap().fill(i as u8);
                assert!(buf.as_ref().data() == vec![i as u8; size].as_slice());
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn pool_buffers_cross_threads_and_outlive_the_owner() {
        const THREADS: usize = 8;
        let pool = BufferPool::new(96).unwrap();
        let barrier = Barrier::new(THREADS);
        let outstanding = thread::scope(|s| {
            let handles: Vec<_> = (0..THREADS)
                .map(|t| {
                    let (pool, barrier) = (pool.as_ref(), &barrier);
                    s.spawn(move || {
                        barrier.wait();
                        let mut held = Vec::new();
                        for i in 0..150 {
                            let buf = if i % 2 == 0 {
                                pool.get().unwrap()
                            } else {
                                pool.get_with_opaque().unwrap().0
                            };
                            let buf = buf.fill((t + i) as u8);
                            assert_eq!(buf.as_ref().data(), &[(t + i) as u8; 96]);
                            held.push(buf.clone());
                            held.push(buf);
                            if i % 4 == 3 {
                                held.clear();
                            }
                        }
                        held
                    })
                })
                .collect();
            handles
                .into_iter()
                .flat_map(|h| h.join().unwrap())
                .collect::<Vec<BufferReferenceBox>>()
        });
        thread::spawn(move || drop(pool)).join().unwrap();
        thread::scope(|s| {
            for chunk in outstanding.chunks(5) {
                let chunk = chunk.to_vec();
                s.spawn(move || {
                    for buf in &chunk {
                        assert_eq!(buf.as_ref().len(), 96);
                    }
                });
            }
        });
        drop(outstanding);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn counts_writability_and_opaques_read_while_others_clone() {
        const THREADS: usize = 6;
        for (buf, want, writable) in every_constructor(33) {
            let opaque = buf.as_ref().opaque() as usize;
            let barrier = Barrier::new(THREADS);
            thread::scope(|s| {
                for t in 0..THREADS {
                    let (buf, barrier, want) = (&buf, &barrier, &want);
                    s.spawn(move || {
                        barrier.wait();
                        let mut kept = Vec::new();
                        for i in 0..100 {
                            let shared = buf.as_ref();
                            assert!(shared.ref_count() >= 1);
                            assert_eq!(shared.opaque() as usize, opaque);
                            if t % 2 == 0 {
                                kept.push(buf.clone());
                                assert!(!kept[0].as_ref().is_writable());
                                assert!(kept[0].as_ref().data() == want.as_slice());
                            }
                            if i % 7 == 0 {
                                kept.clear();
                            }
                        }
                    });
                }
            });
            assert_eq!(buf.as_ref().ref_count(), 1);
            assert_eq!(buf.as_ref().is_writable(), writable);
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn make_mut_copies_only_what_others_can_see() {
        for size in SIZES {
            for (mut buf, want, writable) in every_constructor(size) {
                // Sole and writable: the bytes in place.
                if writable {
                    let at = buf.as_ref().data().as_ptr();
                    let mut m = buf.as_mut();
                    let bytes = m.make_mut().unwrap();
                    assert_eq!(bytes.as_ptr(), at);
                    assert!(bytes == want.as_slice());
                }
                // Shared or read-only: a private copy, the others untouched.
                let copy = buf.clone();
                let mut m = buf.as_mut();
                let bytes = m.make_mut().unwrap();
                assert!(bytes == want.as_slice());
                bytes.copy_from_slice(&pattern(size, 77));
                assert!(buf.as_ref().data() == pattern(size, 77).as_slice());
                assert!(!buf.as_ref().buffer().ptr_eq(&copy.as_ref().buffer()));
                assert!(copy.as_ref().data() == want.as_slice());
                assert_eq!(
                    (buf.as_ref().ref_count(), copy.as_ref().ref_count()),
                    (1, 1)
                );
                assert!(buf.as_ref().opaque().is_null());
                // The copy of a window is its bytes only.
                let mut narrow = copy.clone();
                let mut n = narrow.as_mut();
                n.advance(size / 3);
                n.truncate(size / 3);
                assert!(n.make_mut().unwrap() == &want[size / 3..2 * (size / 3)]);
                drop(narrow);
                // Released on another thread while this one copies.
                let mut sole = copy.clone();
                let other = thread::spawn(move || drop(copy));
                assert!(sole.as_mut().make_mut().unwrap() == want.as_slice());
                other.join().unwrap();
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn resize_keeps_the_prefix_and_fills_the_tail() {
        for size in [0, 1, 7, 64, 4097] {
            for (buf, want, _) in every_constructor(size) {
                for len in [0, size / 2, size, size + 1, 2 * size + 3] {
                    for (shared, cut) in [(false, 0), (true, 0), (false, 1), (true, 1)] {
                        let mut r = buf.clone();
                        let cut = cut.min(size);
                        r.as_mut().advance(cut);
                        let others = shared.then(|| r.clone());
                        r.as_mut().resize(len, 0xe7).unwrap();
                        let got = r.as_ref().data();
                        assert!(got == resized(&want[cut..], len, 0xe7).as_slice());
                        assert!(buf.as_ref().data() == want.as_slice());
                        if let Some(o) = others {
                            assert!(o.as_ref().data() == &want[cut..]);
                        }
                        if len != size - cut {
                            assert_eq!(r.as_ref().ref_count(), 1);
                            assert!(r.as_ref().is_writable());
                        }
                    }
                }
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn resizable_buffers_resize_while_sole_and_copy_when_shared() {
        for size in SIZES {
            let mut buf = BufferReference::alloc_resizable(size)
                .unwrap()
                .copy_from_slice(&pattern(size, 3));
            assert!(buf.as_ref().opaque().is_null());
            for (len, fill) in [
                (size + 9, 1),
                (size / 2, 2),
                (2 * size + 1, 3),
                (0, 4),
                (5, 5),
            ] {
                let before = buf.as_ref().data().to_vec();
                buf.as_mut().resize(len, fill).unwrap();
                assert!(buf.as_ref().data() == resized(&before, len, fill).as_slice());
                assert!(buf.as_ref().is_writable());
            }
            // A clone or an advanced window makes C copy into a new buffer.
            let clone = buf.clone();
            buf.as_mut().resize(64, 6).unwrap();
            assert!(!buf.as_ref().buffer().ptr_eq(&clone.as_ref().buffer()));
            assert_eq!(clone.as_ref().data(), &[5; 5]);
            assert_eq!(buf.as_ref().data(), &resized(&[5; 5], 64, 6)[..]);
            drop(clone);
            buf.as_mut().advance(1);
            buf.as_mut().resize(70, 7).unwrap();
            assert_eq!(
                buf.as_ref().data(),
                &resized(&resized(&[5; 5], 64, 6)[1..], 70, 7)[..]
            );
        }
    }

    /// Sizes C's allocator refuses (above `av_max_alloc`'s default) fail
    /// every resizing path cleanly, leaving the reference as it was.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn unallocatable_sizes_leave_references_as_they_were() {
        for huge in [usize::MAX, usize::MAX / 2, i32::MAX as usize + 1] {
            assert!(BufferReference::alloc_resizable(huge).is_none());
            let mut u = BufferReference::alloc_resizable(4).unwrap();
            assert!(u.resize(huge).is_err());
            assert_eq!(u.len(), 4);
            let mut buf = u.fill(1);
            let copy = buf.clone();
            for r in [&mut buf, &mut copy.clone()] {
                assert!(r.as_mut().resize(huge, 0).is_err());
                assert_eq!(r.as_ref().data(), &[1; 4]);
            }
            assert!(copy.as_ref().buffer().ptr_eq(&buf.as_ref().buffer()));
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn uninitialised_buffers_resize_before_they_are_filled() {
        for size in SIZES {
            for mut u in [
                BufferReference::alloc_resizable(size).unwrap(),
                BufferReference::alloc(size).unwrap(),
                BufferPool::new(size).unwrap().as_ref().get().unwrap(),
            ] {
                u.resize(size / 2).unwrap();
                assert_eq!(u.as_slice().len(), size / 2);
                u.resize(size + 13).unwrap();
                assert_eq!((u.len(), u.as_mut_slice().len()), (size + 13, size + 13));
                let buf = u.copy_from_slice(&pattern(size + 13, 9));
                assert!(buf.as_ref().data() == pattern(size + 13, 9).as_slice());
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn assignments_move_windows_and_counts() {
        for size in [0, 1, 64, 4097] {
            for (src, want, _) in every_constructor(size) {
                let (other, other_want, _) = every_constructor(size + 1).swap_remove(3);
                // Another buffer: a new count of `src`'s, the old released.
                let mut dst = other.clone();
                dst.as_mut().assign(src.as_ref()).unwrap();
                assert!(dst.as_ref().buffer().ptr_eq(&src.as_ref().buffer()));
                assert!(dst.as_ref().data() == want.as_slice());
                assert_eq!(
                    (src.as_ref().ref_count(), other.as_ref().ref_count()),
                    (2, 1)
                );
                // The same buffer: the window only.
                let mut narrow = src.clone();
                narrow.as_mut().advance(size / 2);
                dst.as_mut().assign(narrow.as_ref()).unwrap();
                assert!(dst.as_ref().data() == &want[size / 2..]);
                narrow.as_mut().assign(src.as_ref()).unwrap();
                assert!(narrow.as_ref().data() == want.as_slice());
                assert_eq!(src.as_ref().ref_count(), 3);
                // The slot form: filled, emptied, refilled and moved.
                let mut slot = None;
                buffer::assign(&mut slot, Some(dst.as_ref())).unwrap();
                assert!(slot.as_ref().unwrap().as_ref().data() == &want[size / 2..]);
                buffer::assign(&mut slot, Some(src.as_ref())).unwrap();
                assert!(slot.as_ref().unwrap().as_ref().data() == want.as_slice());
                assert_eq!(src.as_ref().ref_count(), 4);
                buffer::assign(&mut slot, Some(other.as_ref())).unwrap();
                assert!(slot.as_ref().unwrap().as_ref().data() == other_want.as_slice());
                assert_eq!(
                    (src.as_ref().ref_count(), other.as_ref().ref_count()),
                    (3, 2)
                );
                buffer::assign(&mut slot, None).unwrap();
                assert!(slot.is_none());
                buffer::assign(&mut slot, None).unwrap();
                assert_eq!(other.as_ref().ref_count(), 1);
                drop((dst, narrow));
                assert_eq!(src.as_ref().ref_count(), 1);
            }
        }
    }

    /// Read-only bytes that count the drops of their owner.
    struct Owner(Vec<u8>, Arc<AtomicUsize>);

    impl AsRef<[u8]> for Owner {
        fn as_ref(&self) -> &[u8] {
            &self.0
        }
    }

    impl Drop for Owner {
        fn drop(&mut self) {
            self.1.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn assignments_race_with_clones_and_releases_of_their_source() {
        const THREADS: usize = 6;
        let drops = Arc::new(AtomicUsize::new(0));
        let src = BufferReference::from_read_only(Owner(pattern(200, 5), drops.clone()))
            .map_err(drop)
            .unwrap();
        let barrier = Barrier::new(THREADS);
        thread::scope(|s| {
            for t in 0..THREADS {
                let (src, barrier) = (src.as_ref(), &barrier);
                s.spawn(move || {
                    barrier.wait();
                    let mut mine = BufferReference::alloc_zeroed(t).unwrap();
                    let mut slot = None;
                    for i in 0..100 {
                        mine.as_mut().assign(src).unwrap();
                        buffer::assign(&mut slot, Some(src)).unwrap();
                        assert_eq!(mine.as_ref().data()[199], pattern(200, 5)[199]);
                        if i % 3 == 0 {
                            mine.as_mut().set(BufferReference::alloc_zeroed(i).unwrap());
                            buffer::assign(&mut slot, None).unwrap();
                        }
                        // Read-only: make_mut always copies.
                        let mut copy = src.try_clone().unwrap();
                        assert!(copy.as_mut().make_mut().unwrap()[..2] == pattern(2, 5)[..]);
                    }
                });
            }
        });
        assert_eq!(src.as_ref().ref_count(), 1);
        drop(src);
        assert_eq!(drops.load(Ordering::SeqCst), 1);
    }

    /// The BSan target: Rust views of a reference's bytes taken and dropped
    /// between C's copies, reallocations and struct rewrites of the same
    /// allocations (`av_buffer_make_writable`'s memcpy, `av_buffer_realloc`'s
    /// in-place `av_realloc` and copy, `av_buffer_replace`'s window copy).
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn foreign_mutations_across_rust_reborrows() {
        let mut buf = BufferReference::alloc_resizable(512).unwrap().fill(1);
        buf.as_mut().data_mut().unwrap()[..256].copy_from_slice(&[0xab; 256]);
        let copy = buf.clone();
        let mut m = buf.as_mut();
        let bytes = m.make_mut().unwrap();
        assert!(bytes[..256] == [0xab; 256]);
        bytes[256..].copy_from_slice(&[0xcd; 256]);
        drop(copy);
        // Sole, resizable and at its start: C reallocates in place.
        m.resize(1024, 0xef).unwrap();
        let data = m.as_ref().data();
        assert!(data[256..512] == [0xcd; 256] && data[512..] == [0xef; 512]);
        m.data_mut().unwrap()[..8].copy_from_slice(&[9; 8]);
        m.resize(300, 0).unwrap();
        assert!(m.as_ref().data()[..8] == [9; 8]);
        let mut other = BufferReference::alloc_resizable(16).unwrap().fill(2);
        other.as_mut().assign(buf.as_ref()).unwrap();
        let mut narrow = other.clone();
        narrow.as_mut().advance(100);
        other.as_mut().assign(narrow.as_ref()).unwrap();
        assert!(other.as_ref().data()[..200] == buf.as_ref().data()[100..]);
        drop(narrow);
        drop(other);
        buf.as_mut().make_mut().unwrap()[299] = 3;
        assert_eq!(buf.as_ref().data()[299], 3);
    }
}

/// Review of buffer-frame wave 2: `UninitBuffer::from_owner`, buffers over
/// Rust owners of uninitialised bytes, at sizes from empty to 64 KiB and in
/// every owner shape: their views are exactly the owner's bytes (a read
/// past one is an ASan report), filled, resized (a copy, as the buffer has
/// an opaque) and released once, on whichever thread drops the last
/// reference; the conversion back refuses them, as it does initialised
/// owners.
mod uninit_owner {
    use core::mem::MaybeUninit;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;

    use ffmpeg_bindings::libavutil::libavutil::buffer::{BufferReference, UninitBuffer};

    use super::reference::pattern;

    const SIZES: [usize; 5] = [0, 1, 15, 4096, 1 << 16];

    /// Uninitialised bytes that count the drops of their owner.
    struct Counted {
        bytes: Vec<MaybeUninit<u8>>,
        drops: Arc<AtomicUsize>,
    }

    impl AsMut<[MaybeUninit<u8>]> for Counted {
        fn as_mut(&mut self) -> &mut [MaybeUninit<u8>] {
            &mut self.bytes
        }
    }

    impl Drop for Counted {
        fn drop(&mut self) {
            self.drops.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn every_owner_shape_views_exactly_its_bytes() {
        for size in SIZES {
            let want = pattern(size, 3);
            let owners = [
                UninitBuffer::from_owner(vec![MaybeUninit::uninit(); size]).unwrap(),
                UninitBuffer::from_owner(Box::<[u8]>::new_uninit_slice(size)).unwrap(),
            ];
            for u in owners {
                assert_eq!((u.len(), u.as_slice().len()), (size, size));
                let buf = u.copy_from_slice(&want);
                assert!(buf.as_ref().data() == want.as_slice());
                assert!(!buf.as_ref().opaque().is_null());
            }
        }
        // An inline array, moved into the buffer with its owner.
        let inline = UninitBuffer::from_owner([MaybeUninit::new(7u8); 33]).unwrap();
        assert!(inline.fill(8).as_ref().data() == [8; 33].as_slice());
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn owners_resize_by_copy_and_release_once_anywhere() {
        for size in SIZES {
            let drops = Arc::new(AtomicUsize::new(0));
            let owner = Counted {
                bytes: vec![MaybeUninit::uninit(); size],
                drops: drops.clone(),
            };
            let Ok(mut u) = UninitBuffer::from_owner(owner) else {
                panic!("a buffer over the owner");
            };
            let at = u.as_slice().as_ptr();
            u.resize(size + 9).unwrap();
            // The copy released the owner's buffer, its only reference.
            assert_eq!(drops.load(Ordering::SeqCst), 1);
            assert!(size == 0 || u.as_slice().as_ptr() != at);
            let mut buf = u.fill(0x42);
            assert!(buf.as_ref().data() == vec![0x42; size + 9].as_slice());
            assert!(buf.as_ref().opaque().is_null());
            buf.as_mut().resize(size, 0).unwrap();
            assert!(buf.as_ref().data() == vec![0x42; size].as_slice());

            let drops = Arc::new(AtomicUsize::new(0));
            let owner = Counted {
                bytes: vec![MaybeUninit::uninit(); size],
                drops: drops.clone(),
            };
            let Ok(u) = UninitBuffer::from_owner(owner) else {
                panic!("a buffer over the owner");
            };
            let buf = u.fill(1);
            let copy = buf.clone();
            let buf = UninitBuffer::try_from(buf).unwrap_err();
            drop(buf);
            thread::spawn(move || {
                assert!(copy.as_ref().data() == vec![1; size].as_slice());
                let copy = UninitBuffer::try_from(copy).unwrap_err();
                assert_eq!(copy.as_ref().ref_count(), 1);
            })
            .join()
            .unwrap();
            assert_eq!(drops.load(Ordering::SeqCst), 1);
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn initialised_owners_never_convert() {
        let mut buf = BufferReference::from_owner(vec![5u8; 12]).unwrap();
        assert!(buf.as_mut().data_mut().is_some());
        let buf = UninitBuffer::try_from(buf).unwrap_err();
        assert!(buf.as_ref().data() == [5; 12].as_slice());
    }
}
