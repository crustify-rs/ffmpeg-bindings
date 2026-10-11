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
