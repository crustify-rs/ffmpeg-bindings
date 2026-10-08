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

/// `av_vlog` through the safe general log, with the default callback.
mod vlog {
    use core::ffi::c_int;
    use std::ffi::CString;
    use std::thread;

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::log::log;

    const DEBUG: c_int = ffi::AV_LOG_DEBUG as c_int;

    /// Levels the default callback drops, keeping the output quiet: it masks
    /// a non-negative level to its low byte, and drops it above the default
    /// `AV_LOG_INFO`.
    const DROPPED: [c_int; 7] = [
        DEBUG,
        ffi::AV_LOG_TRACE as c_int,
        DEBUG | 134 << 8,
        64,
        255,
        0x7fff_ff40,
        c_int::MAX,
    ];

    #[test]
    fn any_message_is_accepted() {
        let long = CString::new(vec![b'%'; 4000]).unwrap();
        for level in DROPPED {
            for message in [c"", c"%s%n%p%d", c"no newline", long.as_c_str()] {
                log(level, message);
            }
        }
    }

    #[test]
    fn printed_lines_are_formatted_whole() {
        let long = CString::new(format!("crustify ub_safe vlog: {}\n", "%".repeat(1500))).unwrap();
        log(
            ffi::AV_LOG_INFO as c_int,
            c"crustify ub_safe vlog: 100% %s %n\n",
        );
        log(ffi::AV_LOG_INFO as c_int, &long);
        log(
            ffi::AV_LOG_INFO as c_int,
            c"crustify ub_safe vlog: \x01\x7f partial",
        );
        log(ffi::AV_LOG_INFO as c_int, c"\n");
    }

    #[test]
    fn logging_from_many_threads_at_once() {
        let workers: Vec<_> = (0..8)
            .map(|i| {
                thread::spawn(move || {
                    let message =
                        CString::new(format!("crustify ub_safe vlog thread {i}\n")).unwrap();
                    for j in 0..200 {
                        log(
                            if j == 0 {
                                ffi::AV_LOG_INFO as c_int
                            } else {
                                DEBUG
                            },
                            &message,
                        );
                    }
                })
            })
            .collect();
        for w in workers {
            w.join().unwrap();
        }
    }
}
