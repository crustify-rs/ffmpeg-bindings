//! Safe wrappers over `libavutil/avsscanf.c`: [`scan`] is `av_sscanf`, C's
//! `sscanf` in the C locale, with each destination, a [`ScanArg`], checked
//! against the format before the call, and a [`ScanError`] for what it refuses.

use core::ffi::{CStr, c_char, c_int, c_long, c_longlong, c_short, c_void};
use core::{fmt, ptr};

use super::ffi;

/// The most destinations one [`scan`] call binds.
pub const SCAN_MAX_ARGS: usize = 16;

/// One destination of a [`scan`] conversion.
///
/// An integer conversion stores into an integer of the size its length
/// modifier names, signed or unsigned, and `%p` into a pointer-sized one; a
/// floating-point conversion into an `f32` (no modifier) or an `f64` (`l`);
/// and `%c`, `%s` and `%[` into a byte buffer, which must hold the
/// conversion's width (plus the terminator `%s` and `%[` add). The `From`
/// conversions build one from a mutable reference.
#[derive(Debug)]
pub enum ScanArg<'a> {
    /// A signed byte: `%hhd`, `%hhn`.
    I8(&'a mut i8),
    /// An unsigned byte: `%hhu`, `%hhx`.
    U8(&'a mut u8),
    /// A `short`: `%hd`.
    I16(&'a mut i16),
    /// An `unsigned short`: `%hu`.
    U16(&'a mut u16),
    /// An `int`: `%d`, `%i`, `%n`.
    I32(&'a mut i32),
    /// An `unsigned`: `%u`, `%x`, `%o`.
    U32(&'a mut u32),
    /// A 64-bit integer: `%lld`, `%jd`, and `%ld` where `long` is 64-bit.
    I64(&'a mut i64),
    /// An unsigned 64-bit integer: `%llu`, `%llx`, and `%lu` where `long` is
    /// 64-bit.
    U64(&'a mut u64),
    /// A pointer-sized signed integer: `%zd`, `%td` where `long` is
    /// pointer-sized.
    Isize(&'a mut isize),
    /// A pointer-sized unsigned integer: `%zu`, and the address `%p` reads.
    Usize(&'a mut usize),
    /// A `float`: `%f`, `%e`, `%g`, `%a`.
    F32(&'a mut f32),
    /// A `double`: `%lf`, `%le`, `%lg`, `%la`.
    F64(&'a mut f64),
    /// A byte buffer: `%c` (no terminator), `%s` and `%[` (terminated).
    Bytes(&'a mut [u8]),
}

macro_rules! scan_arg_from {
    ($($t:ty => $v:ident),* $(,)?) => {$(
        impl<'a> From<&'a mut $t> for ScanArg<'a> {
            fn from(dst: &'a mut $t) -> Self {
                Self::$v(dst)
            }
        }
    )*};
}
scan_arg_from!(
    i8 => I8, u8 => U8, i16 => I16, u16 => U16, i32 => I32, u32 => U32,
    i64 => I64, u64 => U64, isize => Isize, usize => Usize, f32 => F32,
    f64 => F64, [u8] => Bytes,
);

impl<'a, const N: usize> From<&'a mut [u8; N]> for ScanArg<'a> {
    fn from(dst: &'a mut [u8; N]) -> Self {
        Self::Bytes(dst)
    }
}

impl ScanArg<'_> {
    /// The integer's size in bytes, for an integer destination.
    fn int_size(&self) -> Option<usize> {
        Some(match self {
            Self::I8(_) | Self::U8(_) => 1,
            Self::I16(_) | Self::U16(_) => 2,
            Self::I32(_) | Self::U32(_) => 4,
            Self::I64(_) | Self::U64(_) => 8,
            Self::Isize(_) | Self::Usize(_) => size_of::<usize>(),
            Self::F32(_) | Self::F64(_) | Self::Bytes(_) => return None,
        })
    }

    /// The destination's address, which C writes through.
    fn as_mut_ptr(&mut self) -> *mut c_void {
        match self {
            Self::I8(p) => ptr::from_mut(*p).cast(),
            Self::U8(p) => ptr::from_mut(*p).cast(),
            Self::I16(p) => ptr::from_mut(*p).cast(),
            Self::U16(p) => ptr::from_mut(*p).cast(),
            Self::I32(p) => ptr::from_mut(*p).cast(),
            Self::U32(p) => ptr::from_mut(*p).cast(),
            Self::I64(p) => ptr::from_mut(*p).cast(),
            Self::U64(p) => ptr::from_mut(*p).cast(),
            Self::Isize(p) => ptr::from_mut(*p).cast(),
            Self::Usize(p) => ptr::from_mut(*p).cast(),
            Self::F32(p) => ptr::from_mut(*p).cast(),
            Self::F64(p) => ptr::from_mut(*p).cast(),
            Self::Bytes(b) => b.as_mut_ptr().cast(),
        }
    }
}

/// Why [`scan`] rejected a format before calling C. Each variant carries
/// the byte offset of the `%` that starts the offending conversion, where
/// there is one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ScanError {
    /// A conversion C does not implement, or one this wrapper cannot bind
    /// safely: `%m` allocation, `%n$` positions, `%C` / `%S`, `L`, length
    /// modifiers on `%c`, `%s`, `%[` and `%p`, or a `%` ending the format.
    Unsupported(usize),
    /// A `%[` whose set has no closing `]`.
    UnterminatedSet(usize),
    /// A field width beyond `i32::MAX`, which overflows C's width counter.
    WidthOverflow(usize),
    /// A `%s` or `%[` without a positive width, which would write past any
    /// buffer.
    Unbounded(usize),
    /// A conversion with no destination left to bind.
    MissingArg(usize),
    /// A destination of the wrong kind or size for its conversion.
    Mismatch(usize),
    /// A byte buffer too small for its conversion's width.
    BufferTooSmall(usize),
    /// More destinations than conversions.
    ExtraArgs,
    /// More than [`SCAN_MAX_ARGS`] destinations.
    TooManyArgs,
}

impl fmt::Display for ScanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Unsupported(at) => write!(f, "unsupported conversion at byte {at}"),
            Self::UnterminatedSet(at) => write!(f, "unterminated %[ set at byte {at}"),
            Self::WidthOverflow(at) => write!(f, "field width overflows at byte {at}"),
            Self::Unbounded(at) => write!(f, "unbounded string conversion at byte {at}"),
            Self::MissingArg(at) => write!(f, "no destination for the conversion at byte {at}"),
            Self::Mismatch(at) => write!(f, "wrong destination for the conversion at byte {at}"),
            Self::BufferTooSmall(at) => {
                write!(f, "buffer too small for the conversion at byte {at}")
            }
            Self::ExtraArgs => f.write_str("more destinations than conversions"),
            Self::TooManyArgs => write!(f, "more than {SCAN_MAX_ARGS} destinations"),
        }
    }
}

impl std::error::Error for ScanError {}

/// The length modifier of a conversion, as `av_sscanf` reads it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ScanSize {
    /// None: `int`, `float`.
    Default,
    /// `hh`: `char`.
    Char,
    /// `h`: `short`.
    Short,
    /// `l`, `z`, `t`: `long`; `double` for a floating-point conversion.
    Long,
    /// `ll`, `j`: `long long`.
    LongLong,
    /// `L`.
    LongDouble,
}

/// Checks `args` against the conversions of `format`, as `av_sscanf` parses
/// them, and returns their addresses in conversion order, NULL-padded to
/// [`SCAN_MAX_ARGS`].
#[expect(
    clippy::too_many_lines,
    reason = "one pass over the format string, as C's scanner is"
)]
#[expect(
    clippy::match_same_arms,
    reason = "the arms follow C's cases one for one"
)]
#[expect(
    clippy::items_after_statements,
    reason = "the item is declared where the code it serves begins"
)]
fn bind_scan_args(
    format: &[u8],
    args: &mut [ScanArg<'_>],
) -> Result<[*mut c_void; SCAN_MAX_ARGS], ScanError> {
    if args.len() > SCAN_MAX_ARGS {
        return Err(ScanError::TooManyArgs);
    }
    let mut ptrs = [ptr::null_mut(); SCAN_MAX_ARGS];
    let mut bound = 0;
    let at = |i: usize| format.get(i).copied();
    let mut i = 0;
    while let Some(c) = at(i) {
        // Whitespace and literal bytes bind nothing, and nor does `%%`.
        if c != b'%' {
            i += 1;
            continue;
        }
        if at(i + 1) == Some(b'%') {
            i += 2;
            continue;
        }
        let start = i;
        i += 1;
        let suppress = at(i) == Some(b'*');
        if suppress {
            i += 1;
        } else if at(i).is_some_and(|c| c.is_ascii_digit()) && at(i + 1) == Some(b'$') {
            return Err(ScanError::Unsupported(start));
        }
        let mut width: i32 = 0;
        while let Some(d) = at(i).filter(u8::is_ascii_digit) {
            width = width
                .checked_mul(10)
                .and_then(|w| w.checked_add(i32::from(d - b'0')))
                .ok_or(ScanError::WidthOverflow(start))?;
            i += 1;
        }
        let size = match (at(i), at(i + 1)) {
            (Some(b'h'), Some(b'h')) => ScanSize::Char,
            (Some(b'h'), _) => ScanSize::Short,
            (Some(b'l'), Some(b'l')) | (Some(b'j'), _) => ScanSize::LongLong,
            (Some(b'l' | b'z' | b't'), _) => ScanSize::Long,
            (Some(b'L'), _) => ScanSize::LongDouble,
            _ => ScanSize::Default,
        };
        i += match size {
            ScanSize::Default => 0,
            ScanSize::Char => 2,
            ScanSize::LongLong if at(i) == Some(b'l') => 2,
            _ => 1,
        };
        let conv = at(i).ok_or(ScanError::Unsupported(start))?;
        i += 1;
        /// What a conversion stores.
        enum Store {
            Int(usize),
            Float(usize),
            Bytes(usize),
        }
        let store = match (conv, size) {
            (b'd' | b'i' | b'o' | b'u' | b'x' | b'X' | b'n', size) => Store::Int(match size {
                ScanSize::Default => size_of::<c_int>(),
                ScanSize::Char => size_of::<c_char>(),
                ScanSize::Short => size_of::<c_short>(),
                ScanSize::Long => size_of::<c_long>(),
                ScanSize::LongLong => size_of::<c_longlong>(),
                ScanSize::LongDouble => return Err(ScanError::Unsupported(start)),
            }),
            (b'a' | b'e' | b'f' | b'g' | b'A' | b'E' | b'F' | b'G', ScanSize::Default) => {
                Store::Float(size_of::<f32>())
            }
            (b'a' | b'e' | b'f' | b'g' | b'A' | b'E' | b'F' | b'G', ScanSize::Long) => {
                Store::Float(size_of::<f64>())
            }
            (b'p', ScanSize::Default) => Store::Int(size_of::<*mut c_void>()),
            (b'c', ScanSize::Default) => Store::Bytes(width.max(1).unsigned_abs() as usize),
            (b's' | b'[', ScanSize::Default) => {
                if conv == b'[' {
                    // C reads `^`, then a leading `]` or `-` as a member, then
                    // members up to the first `]`.
                    if at(i) == Some(b'^') {
                        i += 1;
                    }
                    if matches!(at(i), Some(b']' | b'-')) {
                        i += 1;
                    }
                    let close = format[i.min(format.len())..]
                        .iter()
                        .position(|&c| c == b']')
                        .ok_or(ScanError::UnterminatedSet(start))?;
                    i += close + 1;
                }
                if width == 0 && !suppress {
                    return Err(ScanError::Unbounded(start));
                }
                Store::Bytes(width.unsigned_abs() as usize + 1)
            }
            _ => return Err(ScanError::Unsupported(start)),
        };
        if suppress {
            continue;
        }
        let arg = args.get_mut(bound).ok_or(ScanError::MissingArg(start))?;
        let fits = match (store, &*arg) {
            (Store::Int(n), arg) => arg.int_size() == Some(n),
            (Store::Float(4), ScanArg::F32(_)) | (Store::Float(8), ScanArg::F64(_)) => true,
            (Store::Float(_), _) => false,
            (Store::Bytes(n), ScanArg::Bytes(b)) => {
                if b.len() < n {
                    return Err(ScanError::BufferTooSmall(start));
                }
                true
            }
            (Store::Bytes(_), _) => false,
        };
        if !fits {
            return Err(ScanError::Mismatch(start));
        }
        ptrs[bound] = arg.as_mut_ptr();
        bound += 1;
    }
    if bound != args.len() {
        return Err(ScanError::ExtraArgs);
    }
    Ok(ptrs)
}

/// Wraps: `av_sscanf`
///
/// Reads `input` by `format`, as C's `sscanf` in the C locale, storing each
/// conversion into the next destination of `args`.
///
/// Returns the number of destinations assigned, which stops at the first
/// conversion that fails to match, or `None` when the input ends before any
/// is assigned (C's `EOF`). Destinations past the last assigned one are
/// unchanged, except that a failed `%c` may have written part of its buffer.
///
/// `format` is checked against `args` before C runs, so a mismatch is a
/// [`ScanError`] rather than a stray write: every unsuppressed conversion
/// needs exactly one destination of its type ([`ScanArg`]), `%s` and `%[`
/// need a width (`%15s` into 16 bytes), and conversions C mishandles are
/// refused (see [`ScanError::Unsupported`]).
///
/// ```
/// use ffmpeg_bindings::libavutil::libavutil::avsscanf::scan;
/// let (mut w, mut h, mut name) = (0i32, 0i32, [0u8; 8]);
/// let n = scan(
///     c"640x480 vga",
///     c"%dx%d %7s",
///     &mut [(&mut w).into(), (&mut h).into(), (&mut name).into()],
/// );
/// assert_eq!(n, Ok(Some(3)));
/// assert_eq!((w, h, &name[..4]), (640, 480, &b"vga\0"[..]));
/// ```
///
/// # Errors
///
/// Returns the number of destinations assigned, which stops at the first conversion that fails to
/// match, or `None` when the input ends before any is assigned (C's `EOF`).
pub fn scan(
    input: &CStr,
    format: &CStr,
    args: &mut [ScanArg<'_>],
) -> Result<Option<usize>, ScanError> {
    let p = bind_scan_args(format.to_bytes(), args)?;
    // SAFETY: `input` and `format` are live NUL-terminated strings, which C
    // only reads. `bind_scan_args` checked that `format` has one conversion
    // C implements per address in `p`, in order, and that each address is an
    // exclusively borrowed destination of the size that conversion stores
    // (with room for each `%c` / `%s` / `%[` width and terminator), so C's
    // `va_arg` reads only bound addresses and writes only within them. The
    // NULL padding is never read. C keeps no pointer after returning.
    let ret = unsafe {
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
    Ok(usize::try_from(ret).ok())
}

#[cfg(test)]
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    reason = "test inputs are chosen within range, and the casts mirror the C conversions under comparison"
)]
#[expect(
    clippy::many_single_char_names,
    reason = "names follow the C fields, formulas and fixtures they mirror"
)]
#[expect(
    clippy::redundant_closure_for_method_calls,
    reason = "each test is one scenario, its helpers and steps kept where they are used"
)]
mod unit_tests {
    use super::*;

    /// `bind_scan_args` on `format`, with its addresses dropped.
    fn bind(format: &str, args: &mut [ScanArg<'_>]) -> Result<usize, ScanError> {
        let ptrs = bind_scan_args(format.as_bytes(), args)?;
        Ok(ptrs.iter().take_while(|p| !p.is_null()).count())
    }

    #[test]
    fn scan_binds_each_destination_in_order() {
        let (mut a, mut b, mut c, mut d) = (0i32, 0u8, 0f64, [0u8; 4]);
        let mut args: [ScanArg<'_>; 4] = [
            (&mut a).into(),
            (&mut b).into(),
            (&mut c).into(),
            (&mut d).into(),
        ];
        let expected: Vec<*mut c_void> = args.iter_mut().map(ScanArg::as_mut_ptr).collect();
        let ptrs = bind_scan_args(b" %d%%x %hhx,%lf %3c%*s", &mut args).unwrap();
        assert_eq!(&ptrs[..4], &expected[..]);
        assert!(ptrs[4..].iter().all(|p| p.is_null()));
    }

    #[test]
    fn scan_bound_addresses_stay_writable_until_the_call() {
        // What C does with the addresses: write through each, after binding
        // returned and before the destinations are read again.
        let mut many = [0i32; SCAN_MAX_ARGS];
        let mut buf = [0u8; 4];
        let mut args: Vec<ScanArg<'_>> = many[1..].iter_mut().map(ScanArg::from).collect();
        args.push((&mut buf).into());
        let format = format!("{}%3s", "%d".repeat(SCAN_MAX_ARGS - 1));
        let ptrs = bind_scan_args(format.as_bytes(), &mut args).unwrap();
        for (i, p) in ptrs[..SCAN_MAX_ARGS - 1].iter().enumerate() {
            // SAFETY: each address is an exclusively borrowed `i32`
            // destination, which nothing else touches until `args` drops.
            unsafe { p.cast::<i32>().write(i as i32 + 1) };
        }
        // SAFETY: the last address is the 4-byte buffer.
        unsafe { ptrs[SCAN_MAX_ARGS - 1].cast::<[u8; 4]>().write(*b"abc\0") };
        drop(args);
        let expected: Vec<i32> = (0..SCAN_MAX_ARGS as i32).collect();
        assert_eq!(&many[..], &expected[..]);
        assert_eq!(&buf, b"abc\0");
    }

    #[test]
    fn scan_integer_sizes_follow_the_length_modifier() {
        let (mut i8_, mut i16_, mut i32_, mut i64_, mut us) = (0i8, 0i16, 0i32, 0i64, 0usize);
        assert_eq!(bind("%hhd", &mut [(&mut i8_).into()]), Ok(1));
        assert_eq!(bind("%hn", &mut [(&mut i16_).into()]), Ok(1));
        assert_eq!(bind("%o", &mut [(&mut i32_).into()]), Ok(1));
        assert_eq!(bind("%lld", &mut [(&mut i64_).into()]), Ok(1));
        assert_eq!(bind("%jx", &mut [(&mut i64_).into()]), Ok(1));
        assert_eq!(bind("%zu", &mut [(&mut us).into()]), Ok(1));
        assert_eq!(bind("%p", &mut [(&mut us).into()]), Ok(1));
        assert_eq!(
            bind("%ld", &mut [(&mut i64_).into()]),
            if size_of::<c_long>() == 8 {
                Ok(1)
            } else {
                Err(ScanError::Mismatch(0))
            }
        );
        // Signedness is free; size is not.
        let mut u = 0u32;
        assert_eq!(bind("%d", &mut [(&mut u).into()]), Ok(1));
        assert_eq!(
            bind("%hd", &mut [(&mut i32_).into()]),
            Err(ScanError::Mismatch(0))
        );
        assert_eq!(
            bind("%d", &mut [(&mut i64_).into()]),
            Err(ScanError::Mismatch(0))
        );
        let mut f = 0f32;
        assert_eq!(
            bind("%d", &mut [(&mut f).into()]),
            Err(ScanError::Mismatch(0))
        );
    }

    #[test]
    fn scan_floats_and_bytes_check_kind_and_room() {
        let (mut f, mut d, mut i) = (0f32, 0f64, 0i32);
        assert_eq!(bind("%G", &mut [(&mut f).into()]), Ok(1));
        assert_eq!(bind("%la", &mut [(&mut d).into()]), Ok(1));
        assert_eq!(
            bind("%f", &mut [(&mut d).into()]),
            Err(ScanError::Mismatch(0))
        );
        assert_eq!(
            bind("%le", &mut [(&mut f).into()]),
            Err(ScanError::Mismatch(0))
        );
        assert_eq!(
            bind("%e", &mut [(&mut i).into()]),
            Err(ScanError::Mismatch(0))
        );
        assert_eq!(
            bind("%Lf", &mut [(&mut d).into()]),
            Err(ScanError::Unsupported(0))
        );

        let mut buf = [0u8; 4];
        assert_eq!(bind("%c", &mut [(&mut buf[..1]).into()]), Ok(1));
        assert_eq!(bind("%0c", &mut [(&mut buf[..1]).into()]), Ok(1));
        assert_eq!(bind("%4c", &mut [(&mut buf).into()]), Ok(1));
        assert_eq!(
            bind("%5c", &mut [(&mut buf).into()]),
            Err(ScanError::BufferTooSmall(0))
        );
        assert_eq!(
            bind("%c", &mut [(&mut buf[..0]).into()]),
            Err(ScanError::BufferTooSmall(0))
        );
        assert_eq!(bind("%3s", &mut [(&mut buf).into()]), Ok(1));
        assert_eq!(
            bind("%4s", &mut [(&mut buf).into()]),
            Err(ScanError::BufferTooSmall(0))
        );
        assert_eq!(bind("%3[a-z]", &mut [(&mut buf).into()]), Ok(1));
        assert_eq!(
            bind("%s", &mut [(&mut buf).into()]),
            Err(ScanError::Unbounded(0))
        );
        assert_eq!(
            bind("%0[a]", &mut [(&mut buf).into()]),
            Err(ScanError::Unbounded(0))
        );
        assert_eq!(
            bind("%3s", &mut [(&mut i).into()]),
            Err(ScanError::Mismatch(0))
        );
        assert_eq!(
            bind("%c", &mut [(&mut i).into()]),
            Err(ScanError::Mismatch(0))
        );
        // Suppressed, nothing is written, so no width is needed.
        assert_eq!(bind("%*s%*[^x]", &mut []), Ok(0));
    }

    #[test]
    fn scan_sets_end_at_c_closing_bracket() {
        let mut buf = [0u8; 8];
        let mut i = 0i32;
        // A leading `]` or `-` (after an optional `^`) is a member.
        for f in ["%7[]a]%d", "%7[^]a]%d", "%7[-a]%d", "%7[^-]%d", "%7[a-]%d"] {
            assert_eq!(
                bind(f, &mut [(&mut buf).into(), (&mut i).into()]),
                Ok(2),
                "{f}"
            );
        }
        assert_eq!(
            bind("%7[abc", &mut [(&mut buf).into()]),
            Err(ScanError::UnterminatedSet(0))
        );
        assert_eq!(
            bind("%7[]", &mut [(&mut buf).into()]),
            Err(ScanError::UnterminatedSet(0))
        );
        assert_eq!(
            bind("%7[^", &mut [(&mut buf).into()]),
            Err(ScanError::UnterminatedSet(0))
        );
        assert_eq!(bind("x%*[", &mut []), Err(ScanError::UnterminatedSet(1)));
    }

    #[test]
    fn scan_refuses_what_c_mishandles() {
        let mut i = 0i32;
        let mut buf = [0u8; 8];
        for (f, at) in [
            ("%", 0),
            ("ab%", 2),
            ("%1$d", 0),
            ("%md", 0),
            ("%5ms", 0),
            ("%C", 0),
            ("%5S", 0),
            ("%q", 0),
            ("%hq", 0),
            ("% d", 0),
            ("%Ld", 0),
            ("%Ln", 0),
            ("%lc", 0),
            ("%l5s", 0),
            ("%hp", 0),
            ("%*$d", 0),
        ] {
            let r = bind(f, &mut [(&mut i).into()]);
            let r2 = bind(f, &mut [(&mut buf).into()]);
            assert_eq!(r, Err(ScanError::Unsupported(at)), "{f}");
            assert_eq!(r2, Err(ScanError::Unsupported(at)), "{f}");
        }
    }

    #[test]
    fn scan_width_overflow_is_refused() {
        let mut buf = [0u8; 8];
        assert_eq!(
            bind("%2147483647c", &mut [(&mut buf).into()]),
            Err(ScanError::BufferTooSmall(0))
        );
        assert_eq!(
            bind("%2147483648c", &mut [(&mut buf).into()]),
            Err(ScanError::WidthOverflow(0))
        );
        assert_eq!(
            bind(" %99999999999d", &mut []),
            Err(ScanError::WidthOverflow(1))
        );
    }

    #[test]
    fn scan_destination_count_must_match() {
        let (mut a, mut b) = (0i32, 0i32);
        assert_eq!(
            bind("%d %d", &mut [(&mut a).into()]),
            Err(ScanError::MissingArg(3))
        );
        assert_eq!(
            bind("%d", &mut [(&mut a).into(), (&mut b).into()]),
            Err(ScanError::ExtraArgs)
        );
        assert_eq!(
            bind("%*d %%", &mut [(&mut a).into()]),
            Err(ScanError::ExtraArgs)
        );
        assert_eq!(bind("", &mut []), Ok(0));

        let mut many = [0i32; SCAN_MAX_ARGS + 1];
        let mut args: Vec<ScanArg<'_>> = many.iter_mut().map(ScanArg::from).collect();
        let fmt = "%d".repeat(SCAN_MAX_ARGS + 1);
        assert_eq!(bind(&fmt, &mut args), Err(ScanError::TooManyArgs));
        assert_eq!(bind(&fmt[2..], &mut args[1..]), Ok(SCAN_MAX_ARGS));
    }

    #[test]
    fn scan_error_display_names_the_offset() {
        assert_eq!(
            ScanError::Unsupported(3).to_string(),
            "unsupported conversion at byte 3"
        );
        assert_eq!(
            ScanError::TooManyArgs.to_string(),
            "more than 16 destinations"
        );
        let all = [
            ScanError::Unsupported(1),
            ScanError::UnterminatedSet(1),
            ScanError::WidthOverflow(1),
            ScanError::Unbounded(1),
            ScanError::MissingArg(1),
            ScanError::Mismatch(1),
            ScanError::BufferTooSmall(1),
            ScanError::ExtraArgs,
            ScanError::TooManyArgs,
        ];
        let texts: std::collections::HashSet<String> = all.iter().map(|e| e.to_string()).collect();
        assert_eq!(texts.len(), all.len());
        let _: &dyn std::error::Error = &all[0];
    }

    #[test]
    fn scan_arg_from_picks_the_variant() {
        let mut a = 0i8;
        assert!(matches!(ScanArg::from(&mut a), ScanArg::I8(_)));
        let mut b = 0u16;
        assert!(matches!(ScanArg::from(&mut b), ScanArg::U16(_)));
        let mut c = 0isize;
        assert!(matches!(ScanArg::from(&mut c), ScanArg::Isize(_)));
        let mut d = [0u8; 3];
        assert!(matches!(ScanArg::from(&mut d), ScanArg::Bytes(b) if b.len() == 3));
        let e = &mut d[..2];
        assert!(matches!(ScanArg::from(e), ScanArg::Bytes(b) if b.len() == 2));
        assert!(format!("{:?}", ScanArg::from(&mut a)).starts_with("I8("));
    }
}
