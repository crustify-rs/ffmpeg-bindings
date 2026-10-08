//! UB tests of `libavutil/log.h`'s category and state-flag values through the
//! safe API: every raw value, named or not, at the integer extremes.

use core::ffi::c_uint;

use ffmpeg::libavutil::ffi;
use ffmpeg::libavutil::log::{ClassCategory, ClassStateFlags};

/// Raw values at and around every boundary the wrappers distinguish.
const EDGES: [c_uint; 10] = [0, 1, 11, 12, 39, 40, 45, 46, c_uint::MAX - 1, c_uint::MAX];

#[test]
fn any_category_value_is_valid() {
    for v in EDGES {
        let c = ClassCategory::from_value(v);
        let raw: ffi::AVClassCategory = c.into();
        assert_eq!(ClassCategory::from(raw).value(), v);
        let _ = c.name();
        let _ = format!("{c:?}");
    }
}

#[test]
fn any_state_flag_bits_are_valid() {
    for a in EDGES {
        let fa = ClassStateFlags::from_bits_retain(a);
        let _ = format!("{fa:?}");
        let _ = ClassStateFlags::from_bits(a);
        let _ = ClassStateFlags::from_bits_truncate(a);
        let raw: ffi::AVClassStateFlags = fa.into();
        assert_eq!(ClassStateFlags::from(raw), fa);
        for b in EDGES {
            let fb = ClassStateFlags::from_bits_retain(b);
            assert_eq!((fa | fb).bits(), a | b);
            assert_eq!((fa & fb).bits(), a & b);
            assert_eq!((fa - fb).bits(), a & !b);
            let mut f = fa;
            f.set(fb, true);
            f.set(fb, false);
            assert_eq!(f, fa - fb);
        }
    }
}

#[test]
fn values_cross_threads() {
    let c = ClassCategory::DEVICE_INPUT;
    let f = ClassStateFlags::INITIALIZED;
    let (c2, f2) = std::thread::spawn(move || (c, f | ClassStateFlags::from_bits_retain(2)))
        .join()
        .unwrap();
    assert_eq!(c2, c);
    assert_eq!(f2.bits(), 3);
}
