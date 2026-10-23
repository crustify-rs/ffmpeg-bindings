//! Equivalence of `libavutil/avutil.h`'s `av_x_if_null`, against its raw C symbol.
#![expect(
    clippy::many_single_char_names,
    reason = "names follow the C fields, formulas and fixtures they mirror"
)]

use core::ffi::c_void;
use core::ptr;

use ffmpeg_bindings::libavutil::libavutil::avutil_h::{x_if_null, x_if_null_mut};
use ffmpeg_bindings::libavutil::libavutil::ffi;

/// `av_x_if_null(p, x)` on raw pointers.
fn c_pick<T>(p: Option<&T>, x: Option<&T>) -> *const c_void {
    let raw = |r: Option<&T>| r.map_or(ptr::null(), |r| ptr::from_ref(r).cast::<c_void>());
    // SAFETY: `av_x_if_null` reads neither pointer.
    unsafe { ffi::av_x_if_null(raw(p), raw(x)) }.cast_const()
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn x_if_null_picks_as_c() {
    let (a, b) = (0x11u32, 0x22u32);
    for (p, x) in [
        (Some(&a), Some(&b)),
        (None, Some(&b)),
        (Some(&a), None),
        (None, None),
    ] {
        let r = x_if_null(p, x).map_or(ptr::null(), |r| ptr::from_ref(r).cast::<c_void>());
        assert_eq!(r, c_pick(p, x));
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn x_if_null_mut_writes_land_where_c_points() {
    let mut mine = [[0u16; 2]; 2];
    let mut theirs = [[0u16; 2]; 2];
    for (step, (has_p, has_x)) in [(true, true), (false, true), (true, false)]
        .into_iter()
        .enumerate()
    {
        let [a, b] = &mut mine;
        let r = x_if_null_mut(has_p.then_some(a), has_x.then_some(b)).unwrap();
        r[step % 2] += 1;
        let [a, b] = &theirs;
        let c = c_pick(has_p.then_some(a), has_x.then_some(b));
        let i = usize::from(c != ptr::from_ref(a).cast());
        theirs[i][step % 2] += 1;
        assert_eq!(mine, theirs, "step {step}");
    }
}
