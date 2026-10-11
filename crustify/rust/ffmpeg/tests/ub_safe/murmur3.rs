//! Public safe context lifecycle, input lifetimes, and exclusive reborrows.
use ffmpeg::libavutil::murmur3::{Murmur3, Murmur3Box, Murmur3Mut, Murmur3Ref};

#[test]
fn lifecycle_and_foreign_writes() {
    let mut context = Murmur3::with_seed(u64::MAX).unwrap();
    let shared: Murmur3Ref<'_> = context.as_ref();
    let _copy = shared;
    let mut outer = context.as_mut();
    {
        let mut inner: Murmur3Mut<'_> = outer.as_mut();
        let mut input = vec![0xab; 33];
        inner.update(&input[1..]); // deliberately unaligned, two full blocks
        input.fill(0); // update must not retain pointers across Rust reborrows
        drop(input);
        inner.update(&[]);
        let _ = inner.finalize();
        let _ = inner.finalize();
        inner.update(b"after final");
        let _ = inner.finalize();
    }
    outer.reset_seeded(0);
    assert_eq!(outer.finalize(), [0; 16]);
    outer.reset();
    outer.update(b"abc");
    let _ = outer.finalize();
    drop(context);
    let mut context = Murmur3Box::new().unwrap();
    context.as_mut().reset_seeded(0);
    assert_eq!(context.as_mut().finalize(), [0; 16]);
}

#[test]
fn block_edges_and_drop_without_finalizing() {
    for len in 0..=96 {
        let data: Vec<_> = (0..len).map(|i| i as u8).collect();
        let mut context = Murmur3::new().unwrap();
        let mut borrow = context.as_mut();
        for chunk in data.chunks(3) {
            borrow.update(chunk);
            borrow.update(&[]);
        }
        let _ = borrow.finalize();
        borrow.reset_seeded(len as u64);
        borrow.update(&data);
    }
}
