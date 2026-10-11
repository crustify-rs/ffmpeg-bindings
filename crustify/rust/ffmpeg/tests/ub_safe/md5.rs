//! Public safe context lifecycle, input lifetimes, and exclusive reborrows.
use ffmpeg::libavutil::md5::{Md5, Md5Box, Md5Mut, Md5Ref};

#[test]
fn lifecycle_and_foreign_writes() {
    let mut context = Md5::new().unwrap();
    let shared: Md5Ref<'_> = context.as_ref();
    let _copy = shared;
    let mut outer = context.as_mut();
    {
        let mut inner: Md5Mut<'_> = outer.as_mut();
        let mut input = vec![0xab; 129];
        inner.update(&input[1..]); // deliberately unaligned, two full blocks
        input.fill(0); // no retained C pointer may read the modified input
        drop(input);
        inner.update(&[]);
        let _ = inner.finalize();
        let _ = inner.finalize(); // all C post-final states remain memory-safe
        inner.update(b"after final");
        let _ = inner.finalize();
    }
    outer.reset();
    outer.update(b"abc");
    assert_eq!(
        outer.finalize(),
        [
            0x90, 0x01, 0x50, 0x98, 0x3c, 0xd2, 0x4f, 0xb0, 0xd6, 0x96, 0x3f, 0x7d, 0x28, 0xe1,
            0x7f, 0x72
        ]
    );
    drop(context);
    // Exercise construction through the policy as well as the type constructor.
    let mut context = Md5Box::new().unwrap();
    assert_eq!(
        context.as_mut().finalize(),
        [
            0xd4, 0x1d, 0x8c, 0xd9, 0x8f, 0x00, 0xb2, 0x04, 0xe9, 0x80, 0x09, 0x98, 0xec, 0xf8,
            0x42, 0x7e
        ]
    );
}

#[test]
fn block_edges_and_drop_without_finalizing() {
    for len in 0..=192 {
        let data: Vec<_> = (0..len).map(|i| i as u8).collect();
        let mut context = Md5::new().unwrap();
        let mut borrow = context.as_mut();
        for chunk in data.chunks(7) {
            borrow.update(chunk);
            borrow.update(&[]);
        }
        let _ = borrow.finalize();
        borrow.reset();
        borrow.update(&data);
        // Drop with a partially filled context; no finalizer is required.
    }
}

use ffmpeg::libavutil::md5::context_size;

#[test]
fn batch10_immutable_size_across_threads() {
    let size: &'static i32 = context_size();
    assert!(*size > 0);
    let workers: Vec<_> = (0..4)
        .map(|_| {
            std::thread::spawn(move || {
                for _ in 0..100 {
                    assert!(std::ptr::eq(size, context_size()));
                }
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
}

#[test]
fn batch10_digest_boundary_lengths() {
    use ffmpeg::libavutil::md5::sum;
    let data: Vec<_> = (0..2049).map(|i| i as u8).collect();
    assert_eq!(
        sum(b""),
        [
            0xd4, 0x1d, 0x8c, 0xd9, 0x8f, 0x00, 0xb2, 0x04, 0xe9, 0x80, 0x09, 0x98, 0xec, 0xf8,
            0x42, 0x7e
        ]
    );
    for len in [0, 1, 55, 56, 63, 64, 65, 119, 120, 128, 2048] {
        let independent = data[1..1 + len].to_vec();
        assert_eq!(sum(&data[1..1 + len]), sum(&independent));
    }
}
