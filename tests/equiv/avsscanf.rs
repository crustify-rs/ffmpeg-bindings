//! Equivalence of `libavutil/avsscanf.c`'s [`scan`] with `av_sscanf`, case by
//! case and in sequence, on every conversion and across C's numeric scanners.
#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    reason = "test inputs are chosen within range, and the casts mirror the C conversions under comparison"
)]
#![expect(
    clippy::similar_names,
    reason = "names follow the C fields, formulas and fixtures they mirror"
)]

use core::ffi::{CStr, c_void};

use ffmpeg_bindings::libavutil::libavutil::avsscanf::{ScanArg, scan};
use ffmpeg_bindings::libavutil::libavutil::ffi;

/// Every destination type, on each side.
#[derive(Debug, Default)]
struct Dest {
    i0: i32,
    i1: i32,
    i2: i32,
    u: u32,
    hh: i8,
    uhh: u8,
    h: i16,
    uh: u16,
    ll: i64,
    ull: u64,
    z: usize,
    zs: isize,
    f: f32,
    d: f64,
    p: usize,
    b0: [u8; 12],
    b1: [u8; 12],
}

#[derive(Clone, Copy)]
enum F {
    I0,
    I1,
    I2,
    U,
    Hh,
    Uhh,
    H,
    Uh,
    Ll,
    Ull,
    Z,
    Zs,
    Fl,
    D,
    P,
    B0,
    B1,
}

impl Dest {
    /// Each field as a destination, in `F` order.
    fn args(&mut self) -> [Option<ScanArg<'_>>; 17] {
        let Dest {
            i0,
            i1,
            i2,
            u,
            hh,
            uhh,
            h,
            uh,
            ll,
            ull,
            z,
            zs,
            f,
            d,
            p,
            b0,
            b1,
        } = self;
        [
            Some(i0.into()),
            Some(i1.into()),
            Some(i2.into()),
            Some(u.into()),
            Some(hh.into()),
            Some(uhh.into()),
            Some(h.into()),
            Some(uh.into()),
            Some(ll.into()),
            Some(ull.into()),
            Some(z.into()),
            Some(zs.into()),
            Some(f.into()),
            Some(d.into()),
            Some(p.into()),
            Some(b0.into()),
            Some(b1.into()),
        ]
    }

    /// Each field's address, for C.
    fn c_ptr(this: *mut Dest, f: F) -> *mut c_void {
        // SAFETY: `this` is to a live object, as the block below relies on, so the
        // place of its `b0` field is in bounds; nothing is accessed here.
        let b0_ptr = unsafe { &raw mut (*this).b0 };
        // SAFETY: `this` is to a live object, as the block below relies on, so the
        // place of its `b1` field is in bounds; nothing is accessed here.
        let b1_ptr = unsafe { &raw mut (*this).b1 };
        // SAFETY: `this` is to a live object, as the block below relies on, so the
        // place of its `d` field is in bounds; nothing is accessed here.
        let d_ptr = unsafe { &raw mut (*this).d };
        // SAFETY: `this` is to a live object, as the block below relies on, so the
        // place of its `f` field is in bounds; nothing is accessed here.
        let f_ptr = unsafe { &raw mut (*this).f };
        // SAFETY: `this` is to a live object, as the block below relies on, so the
        // place of its `h` field is in bounds; nothing is accessed here.
        let h_ptr = unsafe { &raw mut (*this).h };
        // SAFETY: `this` is to a live object, as the block below relies on, so the
        // place of its `hh` field is in bounds; nothing is accessed here.
        let hh_ptr = unsafe { &raw mut (*this).hh };
        // SAFETY: `this` is to a live object, as the block below relies on, so the
        // place of its `i0` field is in bounds; nothing is accessed here.
        let i0_ptr = unsafe { &raw mut (*this).i0 };
        // SAFETY: `this` is to a live object, as the block below relies on, so the
        // place of its `i1` field is in bounds; nothing is accessed here.
        let i1_ptr = unsafe { &raw mut (*this).i1 };
        // SAFETY: `this` is to a live object, as the block below relies on, so the
        // place of its `i2` field is in bounds; nothing is accessed here.
        let i2_ptr = unsafe { &raw mut (*this).i2 };
        // SAFETY: `this` is to a live object, as the block below relies on, so the
        // place of its `ll` field is in bounds; nothing is accessed here.
        let ll_ptr = unsafe { &raw mut (*this).ll };
        // SAFETY: `this` is to a live object, as the block below relies on, so the
        // place of its `p` field is in bounds; nothing is accessed here.
        let p_ptr = unsafe { &raw mut (*this).p };
        // SAFETY: `this` is to a live object, as the block below relies on, so the
        // place of its `u` field is in bounds; nothing is accessed here.
        let u_ptr = unsafe { &raw mut (*this).u };
        // SAFETY: `this` is to a live object, as the block below relies on, so the
        // place of its `uh` field is in bounds; nothing is accessed here.
        let uh_ptr = unsafe { &raw mut (*this).uh };
        // SAFETY: `this` is to a live object, as the block below relies on, so the
        // place of its `uhh` field is in bounds; nothing is accessed here.
        let uhh_ptr = unsafe { &raw mut (*this).uhh };
        // SAFETY: `this` is to a live object, as the block below relies on, so the
        // place of its `ull` field is in bounds; nothing is accessed here.
        let ull_ptr = unsafe { &raw mut (*this).ull };
        // SAFETY: `this` is to a live object, as the block below relies on, so the
        // place of its `z` field is in bounds; nothing is accessed here.
        let z_ptr = unsafe { &raw mut (*this).z };
        // SAFETY: `this` is to a live object, as the block below relies on, so the
        // place of its `zs` field is in bounds; nothing is accessed here.
        let zs_ptr = unsafe { &raw mut (*this).zs };
        // `this` points at a live `Dest`; only addresses are taken.
        match f {
            F::I0 => i0_ptr.cast(),
            F::I1 => i1_ptr.cast(),
            F::I2 => i2_ptr.cast(),
            F::U => u_ptr.cast(),
            F::Hh => hh_ptr.cast(),
            F::Uhh => uhh_ptr.cast(),
            F::H => h_ptr.cast(),
            F::Uh => uh_ptr.cast(),
            F::Ll => ll_ptr.cast(),
            F::Ull => ull_ptr.cast(),
            F::Z => z_ptr.cast(),
            F::Zs => zs_ptr.cast(),
            F::Fl => f_ptr.cast(),
            F::D => d_ptr.cast(),
            F::P => p_ptr.cast(),
            F::B0 => b0_ptr.cast(),
            F::B1 => b1_ptr.cast(),
        }
    }
}

/// Inputs and formats across every conversion, width, set form, `%n`,
/// suppression, literal and failure path, with their destinations.
const CASES: &[(&CStr, &CStr, &[F])] = &[
    (c"42", c"%d", &[F::I0]),
    (c"-17 0x1f 077", c"%d %x %o", &[F::I0, F::U, F::I1]),
    (c"0x1F 017 -0", c"%i %i %i", &[F::I0, F::I1, F::I2]),
    (c"300 200", c"%hhd %hhu", &[F::Hh, F::Uhh]),
    (c"65537", c"%hd", &[F::H]),
    (c"70000 -3", c"%hu %hx", &[F::Uh, F::H]),
    (c"-42 0x10", c"%zd %ti", &[F::Zs, F::Z]),
    (c"99999999999999999999", c"%lld", &[F::Ll]),
    (c"18446744073709551615", c"%llu", &[F::Ull]),
    (c"123 -5", c"%zu %jd", &[F::Z, F::Ll]),
    (c"12345", c"%3d%d", &[F::I0, F::I1]),
    (c"3.5e2", c"%f", &[F::Fl]),
    (c"0x1.8p1", c"%la", &[F::D]),
    (c"inf -nan", c"%lf %f", &[F::D, F::Fl]),
    (c"1e400 1e-400", c"%lg %G", &[F::D, F::Fl]),
    (c"-.5", c"%le", &[F::D]),
    (c"hello world", c"%11s %11s", &[F::B0, F::B1]),
    (c"abcdef", c"%3s%11s", &[F::B0, F::B1]),
    (c"abc", c"%2c", &[F::B0]),
    (c"a", c"%3c", &[F::B0]),
    (c"key=value;", c"%11[^=]=%11[^;]", &[F::B0, F::B1]),
    (c"]x-y.", c"%11[]x-y]", &[F::B0]),
    (c"--a!", c"%11[-a]%c", &[F::B0, F::B1]),
    (c"12 34", c"%d%n %d", &[F::I0, F::I1, F::I2]),
    (c"12 34", c"%*d %d", &[F::I0]),
    (c"7 8", c"%d %*s%n", &[F::I0, F::I1]),
    (c"0xdeadbeef", c"%p", &[F::P]),
    (c"", c"%d", &[F::I0]),
    (c"   ", c"%d", &[F::I0]),
    (c"abc", c"%d", &[F::I0]),
    (c"12 abc", c"%d %d", &[F::I0, F::I1]),
    (c"12", c"%d %d", &[F::I0, F::I1]),
    (c"100%", c"%d%%", &[F::I0]),
    (c"100 %  5", c"%d %% %d", &[F::I0, F::I1]),
    (c"x", c"y%d", &[F::I0]),
    (c"", c"x%d", &[F::I0]),
    (c"\t\n 9", c" %d", &[F::I0]),
    (c"5x", c"%dx%n", &[F::I0, F::I1]),
    (c"5", c"%*5c", &[]),
    (c"abc", c"%hhn%n", &[F::Hh, F::I0]),
];

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn sscanf_matches_av_sscanf_case_by_case_and_in_sequence() {
    // One pair of destinations, carried through every case: unassigned
    // destinations must keep earlier values on both sides alike.
    let (mut c, mut r) = (Dest::default(), Dest::default());
    for &(input, format, fields) in CASES.iter().chain(CASES.iter().rev()) {
        let cp = &raw mut c;
        let mut p = [core::ptr::null_mut::<c_void>(); 16];
        for (slot, &f) in p.iter_mut().zip(fields) {
            *slot = Dest::c_ptr(cp, f);
        }
        // SAFETY: `format` has one conversion per address in `p`, each a
        // live field of `c` of the type the conversion stores (every
        // `%s` / `%[` / `%c` width fits the 12-byte buffers); the NULL
        // padding is never read.
        let cret = unsafe {
            ffi::av_sscanf(
                input.as_ptr(),
                format.as_ptr(),
                p[0],
                p[1],
                p[2],
                p[3],
                p[4],
                p[5],
                p[6],
                p[7],
                p[8],
                p[9],
                p[10],
                p[11],
                p[12],
                p[13],
                p[14],
                p[15],
            )
        };

        let mut all = r.args();
        let mut args: Vec<ScanArg<'_>> = fields
            .iter()
            .map(|&f| all[f as usize].take().unwrap())
            .collect();
        let rret = scan(input, format, &mut args).unwrap();
        drop(args);
        assert_eq!(
            rret.map_or(-1, |n| n as i32),
            cret,
            "{input:?} by {format:?}"
        );
        assert_eq!(
            format!("{r:?}"),
            format!("{c:?}"),
            "{input:?} by {format:?}"
        );
    }
}

/// Numeric inputs across C's integer and floating-point scanners: bases,
/// prefixes, overflow, long mantissas, rounding halfway cases, denormals,
/// hex floats, and partial `inf` / `nan` spellings.
const NUMBERS: [&CStr; 48] = [
    c"0",
    c"123",
    c"-0",
    c"+7",
    c"0x",
    c"0xg",
    c"0X1fG",
    c"077",
    c"08",
    c"-",
    c"+",
    c"18446744073709551616",
    c"-9223372036854775809",
    c"42",
    c"0.1",
    c"123456789012",
    c"1234567890123456789012345",
    c"1.0000000000000000000000001",
    c"0.000001e-300",
    c"2.2250738585072011e-308",
    c"4.9e-324",
    c"1e-320",
    c"2e-324",
    c"123456789e-5",
    c"9007199254740993",
    c"9007199254740995",
    c"0.500000000000000166533453693773481063544750213623046875",
    c"1.7976931348623157e308",
    c"1.8e308",
    c"00000.0001",
    c".",
    c"1e",
    c"1e+",
    c"0x1p99999",
    c"0x0.0001p0",
    c"0x.p1",
    c"0x1p-1070",
    c"0x1.fffffffffffff8p1023",
    c"0x123456789abcdef0123p0",
    c"0x1.000000000000081p0",
    c"infinity",
    c"infinit",
    c"INF",
    c"nan(123)",
    c"nan(",
    c"nan(x y",
    c"NaN",
    c"1e400",
];

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn sscanf_numbers_match_av_sscanf() {
    let formats: [(&CStr, F); 9] = [
        (c"%d", F::I0),
        (c"%i", F::I1),
        (c"%x", F::U),
        (c"%o", F::I2),
        (c"%lli", F::Ll),
        (c"%f", F::Fl),
        (c"%lf", F::D),
        (c"%la", F::D),
        (c"%3lg", F::D),
    ];
    let (mut c, mut r) = (Dest::default(), Dest::default());
    for input in NUMBERS {
        for (format, field) in formats {
            let p = Dest::c_ptr(&raw mut c, field);
            let n = core::ptr::null_mut::<c_void>();
            // SAFETY: `format` has one conversion, whose destination is
            // the live field of `c` at `p`, of the type it stores; the
            // NULL padding is never read.
            let cret = unsafe {
                ffi::av_sscanf(
                    input.as_ptr(),
                    format.as_ptr(),
                    p,
                    n,
                    n,
                    n,
                    n,
                    n,
                    n,
                    n,
                    n,
                    n,
                    n,
                    n,
                    n,
                    n,
                    n,
                    n,
                )
            };
            let mut all = r.args();
            let arg = all[field as usize].take().unwrap();
            let rret = scan(input, format, &mut [arg]).unwrap();
            assert_eq!(
                rret.map_or(-1, |n| n as i32),
                cret,
                "{input:?} by {format:?}"
            );
            assert_eq!(
                format!("{r:?}"),
                format!("{c:?}"),
                "{input:?} by {format:?}"
            );
        }
    }
}
