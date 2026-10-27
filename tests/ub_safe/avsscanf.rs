//! UB tests for `libavutil/avsscanf.c`'s [`scan`], through the safe API only:
//! exact destination buffers, hostile inputs and formats, the destination limit
//! and concurrent calls.

use std::ffi::CString;

use ffmpeg_bindings::libavutil::libavutil::avsscanf::{SCAN_MAX_ARGS, ScanArg, ScanError, scan};

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn scans_fill_exact_buffers_without_overrun() {
    let input = CString::new("x".repeat(5000)).unwrap();
    for width in 1..=32usize {
        // `%Ns` / `%N[x]` need N + 1 bytes, `%Nc` N: exactly that, on
        // the heap.
        for (conv, room) in [("s", width + 1), ("[x]", width + 1), ("c", width)] {
            let format = CString::new(format!("%{width}{conv}")).unwrap();
            let mut buf = vec![0u8; room].into_boxed_slice();
            let n = scan(&input, &format, &mut [(&mut buf[..]).into()]).unwrap();
            assert_eq!(n, Some(1));
            let mut short = vec![0u8; room - 1].into_boxed_slice();
            let e = scan(&input, &format, &mut [(&mut short[..]).into()]);
            assert_eq!(e, Err(ScanError::BufferTooSmall(0)));
        }
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn hostile_inputs_and_formats_are_sound() {
    let mut i = 0i32;
    let mut l = 0i64;
    let mut d = 0f64;
    let mut b = [0u8; 4];
    let digits = CString::new("9".repeat(10_000)).unwrap();
    assert_eq!(scan(&digits, c"%d", &mut [(&mut i).into()]), Ok(Some(1)));
    assert_eq!(scan(&digits, c"%lf", &mut [(&mut d).into()]), Ok(Some(1)));
    let exp = CString::new(format!("1e{}", "9".repeat(400))).unwrap();
    assert_eq!(scan(&exp, c"%lf", &mut [(&mut d).into()]), Ok(Some(1)));
    let hex = CString::new(format!("0x{}p-99999999999", "f".repeat(300))).unwrap();
    assert_eq!(scan(&hex, c"%la", &mut [(&mut d).into()]), Ok(Some(1)));
    for input in [
        c"", c" ", c"-", c"0x", c"+", c"nan(", c"infinit", c".e5", c"1e",
    ] {
        let _ = scan(input, c"%lld", &mut [(&mut l).into()]);
        let _ = scan(input, c"%lf", &mut [(&mut d).into()]);
        let _ = scan(input, c"%3c", &mut [(&mut b[..3]).into()]);
    }
    for format in [
        c"%99999999999d",
        c"%1$d",
        c"%ms",
        c"%s",
        c"%[",
        c"%",
        c"%Lf",
        c"%S",
    ] {
        assert!(
            scan(c"1", format, &mut [(&mut i).into()]).is_err(),
            "{format:?}"
        );
        assert!(
            scan(c"1", format, &mut [(&mut b).into()]).is_err(),
            "{format:?}"
        );
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn the_destination_limit_is_enforced() {
    let mut many = [0i32; SCAN_MAX_ARGS + 1];
    let input = CString::new("1 ".repeat(SCAN_MAX_ARGS + 1)).unwrap();
    let mut args: Vec<ScanArg<'_>> = many.iter_mut().map(ScanArg::from).collect();
    let full = CString::new("%d".repeat(SCAN_MAX_ARGS + 1)).unwrap();
    assert_eq!(scan(&input, &full, &mut args), Err(ScanError::TooManyArgs));
    // BSan's runtime gives some of a long variadic call's stack-passed
    // pointers the provenance of a dead stack frame and reports C's
    // writes through them as use-after-free, with no Rust code involved:
    // `av_sscanf` called directly with 16 live `&raw mut` destinations
    // fails the same way, while ASan finds nothing and the values arrive.
    // The first four destinations travel in registers; the other tests
    // use at most three.
    if std::env::var("CRUSTIFY_BUILD_VARIANT").as_deref() == Ok("bsan") {
        return;
    }
    let max = CString::new("%d".repeat(SCAN_MAX_ARGS)).unwrap();
    assert_eq!(scan(&input, &max, &mut args[1..]), Ok(Some(SCAN_MAX_ARGS)));
    drop(args);
    assert!(many[1..].iter().all(|&v| v == 1));
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn concurrent_scans_do_not_race() {
    std::thread::scope(|scope| {
        for t in 0..8 {
            scope.spawn(move || {
                for k in 0..200 {
                    let text = CString::new(format!("{t} {k} v{k}")).unwrap();
                    let (mut a, mut b, mut s) = (0i32, 0u32, [0u8; 8]);
                    let n = scan(
                        &text,
                        c"%d %u %7s",
                        &mut [(&mut a).into(), (&mut b).into(), (&mut s).into()],
                    );
                    assert_eq!(n, Ok(Some(3)));
                    assert_eq!((a, b), (t, k));
                }
            });
        }
    });
}
