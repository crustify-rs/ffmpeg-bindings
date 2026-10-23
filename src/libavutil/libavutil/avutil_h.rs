//! Safe wrappers over what `libavutil/avutil.h` implements itself: `av_x_if_null`.

use core::ffi::c_void;
use core::ptr;

use super::ffi;

/// Wraps: `av_x_if_null`
///
/// `p`, or `x` when `p` is `None`: C's `p ? p : x`, over shared borrows.
#[must_use]
pub fn x_if_null<'a, T>(p: Option<&'a T>, x: Option<&'a T>) -> Option<&'a T> {
    let p: *const c_void = p.map_or(ptr::null(), |r| ptr::from_ref(r).cast());
    let x: *const c_void = x.map_or(ptr::null(), |r| ptr::from_ref(r).cast());
    // SAFETY: `av_x_if_null` compares `p` with NULL and returns `p` or `x`
    // unchanged, dereferencing neither.
    let chosen = unsafe { ffi::av_x_if_null(p, x) };
    // SAFETY: `chosen` is NULL or one of the two pointers above, each from a
    // `&'a T`, so it addresses a `T` shared for `'a`.
    unsafe { chosen.cast_const().cast::<T>().as_ref() }
}

/// Wraps: `av_x_if_null`
///
/// `p`, or `x` when `p` is `None`, as [`x_if_null`], over exclusive borrows:
/// the one not returned is given up with the call.
#[must_use]
pub fn x_if_null_mut<'a, T>(p: Option<&'a mut T>, x: Option<&'a mut T>) -> Option<&'a mut T> {
    let p: *mut c_void = p.map_or(ptr::null_mut(), |r| ptr::from_mut(r).cast());
    let x: *mut c_void = x.map_or(ptr::null_mut(), |r| ptr::from_mut(r).cast());
    // SAFETY: as for `x_if_null`.
    let chosen = unsafe { ffi::av_x_if_null(p, x) };
    // SAFETY: `chosen` is NULL or one of the two pointers above, each from a
    // `&'a mut T` this call took by value, so it addresses a `T` exclusively
    // borrowed for `'a`, and no other reference to it remains.
    unsafe { chosen.cast::<T>().as_mut() }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn x_if_null_picks_the_first_present_borrow() {
        let (a, b) = (1u64, 2u64);
        assert_eq!(x_if_null(Some(&a), Some(&b)), Some(&1));
        assert_eq!(x_if_null(None, Some(&b)), Some(&2));
        assert_eq!(x_if_null(Some(&a), None), Some(&1));
        assert_eq!(x_if_null::<u64>(None, None), None);
        assert!(ptr::eq(x_if_null(None, Some(&b)).unwrap(), &raw const b));
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn x_if_null_mut_hands_back_a_writable_borrow() {
        let (mut a, mut b) = ([0u8; 3], [0u8; 3]);
        x_if_null_mut(Some(&mut a), Some(&mut b)).unwrap()[0] = 7;
        x_if_null_mut(None, Some(&mut b)).unwrap()[2] = 9;
        assert_eq!((a, b), ([7, 0, 0], [0, 0, 9]));
        assert!(x_if_null_mut::<()>(None, None).is_none());
        let mut unit = ();
        assert!(x_if_null_mut(Some(&mut unit), None).is_some());
    }
}
