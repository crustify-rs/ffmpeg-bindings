//! `libavutil/buffer.h`'s safe pool owner: construction at every size, its
//! handles and reborrows, and its moves and shares across threads.

use std::sync::Barrier;
use std::thread;

use ffmpeg::libavutil::buffer::{BufferPool, BufferPoolBox, BufferPoolRef};

/// Sizes from empty to the largest `size_t`: C allocates no buffer until one
/// is requested, so each builds a pool.
const SIZES: [usize; 7] = [0, 1, 63, 4096, 1 << 30, usize::MAX / 2, usize::MAX];

fn take<T>(_: T) {}

#[test]
fn every_size_builds_and_releases_a_pool() {
    for size in SIZES {
        for _ in 0..4 {
            let pool = BufferPool::new(size).unwrap();
            drop(pool);
        }
    }
}

#[test]
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

    use ffmpeg::libavutil::buffer::default_free;
    use ffmpeg::libavutil::mem::{HeapVec, alloc_array, alloc_array_zeroed};

    #[test]
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

    use ffmpeg::libavutil::buffer::{
        BufferReference, BufferReferenceBox, BufferReferenceRef, UninitBuffer,
    };
    use ffmpeg::libavutil::mem::HeapVec;

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
    fn pattern(len: usize, seed: u8) -> Vec<u8> {
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
    fn every_constructor(size: usize) -> Vec<(BufferReferenceBox, Vec<u8>, bool)> {
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
