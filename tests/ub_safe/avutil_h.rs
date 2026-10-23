//! UB tests for `libavutil/avutil.h`'s `av_x_if_null` and the borrows its results carry.

use ffmpeg_bindings::libavutil::libavutil::avutil_h::{x_if_null, x_if_null_mut};

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn x_if_null_results_borrow_their_sources() {
    let a = Box::new([1u64; 4]);
    let b = Box::new([2u64; 4]);
    let r = x_if_null(None, Some(&*b)).unwrap();
    assert_eq!(r, &[2; 4]);
    let r = x_if_null(Some(&*a), Some(&*b)).unwrap();
    assert_eq!(r, &[1; 4]);
    drop((a, b));
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn x_if_null_mut_results_write_their_source_only() {
    let mut a = Box::new([0u8; 8]);
    let mut b = Box::new([0u8; 8]);
    x_if_null_mut(None, Some(&mut *b)).unwrap().fill(1);
    x_if_null_mut(Some(&mut *a), Some(&mut *b)).unwrap()[7] = 9;
    assert_eq!((*a, *b), ([0, 0, 0, 0, 0, 0, 0, 9], [1; 8]));
    let mut zst = ();
    assert!(x_if_null_mut(Some(&mut zst), None).is_some());
    assert!(x_if_null::<String>(None, None).is_none());
}
