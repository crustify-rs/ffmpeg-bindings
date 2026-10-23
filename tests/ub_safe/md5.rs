//! Public safe context lifecycle, input lifetimes, and exclusive reborrows.
#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "test inputs are chosen within range, and the casts mirror the C conversions under comparison"
)]
#![expect(
    clippy::no_effect_underscore_binding,
    reason = "assertions keep the form of the C behaviour they check"
)]
use ffmpeg_bindings::libavutil::libavutil::md5::{Md5, Md5Box, Md5Mut, Md5Ref};

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn lifecycle_and_foreign_writes() {
    let mut context = Md5::new().unwrap();
    let shared: Md5Ref<'_> = context.as_ref();
    let _copy = shared;
    let mut outer = context.as_mut();
    {
        let mut inner: Md5Mut<'_> = outer.as_mut();
        let mut input = vec![0xAB; 129];
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
            0x90, 0x01, 0x50, 0x98, 0x3C, 0xD2, 0x4F, 0xB0, 0xD6, 0x96, 0x3F, 0x7D, 0x28, 0xE1,
            0x7F, 0x72
        ]
    );
    drop(context);
    // Exercise construction through the policy as well as the type constructor.
    let mut context = Md5Box::new().unwrap();
    assert_eq!(
        context.as_mut().finalize(),
        [
            0xD4, 0x1D, 0x8C, 0xD9, 0x8F, 0x00, 0xB2, 0x04, 0xE9, 0x80, 0x09, 0x98, 0xEC, 0xF8,
            0x42, 0x7E
        ]
    );
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
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

use ffmpeg_bindings::libavutil::libavutil::md5::context_size;

#[test]
#[cfg_attr(miri, ignore = "reads a C global")]
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
#[cfg_attr(miri, ignore = "calls C")]
fn batch10_digest_boundary_lengths() {
    use ffmpeg_bindings::libavutil::libavutil::md5::sum;
    let data: Vec<_> = (0..2049).map(|i| i as u8).collect();
    assert_eq!(
        sum(b""),
        [
            0xD4, 0x1D, 0x8C, 0xD9, 0x8F, 0x00, 0xB2, 0x04, 0xE9, 0x80, 0x09, 0x98, 0xEC, 0xF8,
            0x42, 0x7E
        ]
    );
    for len in [0, 1, 55, 56, 63, 64, 65, 119, 120, 128, 2048] {
        let independent = data[1..=len].to_vec();
        assert_eq!(sum(&data[1..=len]), sum(&independent));
    }
}
