//! Safe wrappers over `libavutil/parseutils.h`.
//!
//! The parsers read a NUL-terminated string and return what C stores in its
//! out-parameters, or C's `AVERROR` code: [`parse_color`] (and its prefix and
//! logging-context forms) reads a color (`av_parse_color`),
//! [`parse_video_size`] a frame size (`av_parse_video_size`), and
//! [`parse_date`] and [`parse_duration`] the two modes of `av_parse_time`.
//! None of them keeps a borrow of its input.
//!
//! [`find_info_tag`] reads one tag of a URL query string into a caller's
//! buffer; [`known_color_name`] enumerates the named colors the color parser
//! recognizes.
//!
//! [`parse_ratio`] and [`parse_video_rate`] read a [`Rational`] (an aspect
//! ratio, a frame rate); [`small_strptime`] reads date and time fields into
//! a [`Tm`](glibc_bindings::libc::time::bits::types::struct_tm_h::Tm), and [`timegm`] turns one
//! into seconds since the epoch, refusing the values C's unchecked arithmetic overflows on.

use core::ffi::{CStr, c_char, c_int};
use core::ptr::{self, NonNull};

use glibc_bindings::libc::time::bits::types::struct_tm_h::{TmMut, TmRef};

use super::opt::ObjectRef;
use super::rational::Rational;
use super::{eval, ffi};

/// `AVERROR(EINVAL)`.
const AVERROR_EINVAL: c_int = -(ffi::EINVAL.cast_signed());

/// The bytes C's `av_parse_color` skips as a hex marker before copying the
/// color: 1 for a leading `#`, 2 for a leading `0x` (lowercase only).
fn hex_offset(color: &[u8]) -> usize {
    match color {
        [b'#', ..] => 1,
        [b'0', b'x', ..] => 2,
        _ => 0,
    }
}

/// A `len` at or above which every `slen` is equivalent: C copies
/// `min(slen - hex_offset + 1, 128)` bytes, NUL included, so any `slen` of at
/// least `127 + hex_offset` copies the same 127.
const COLOR_SLEN_CAP: usize = 256;

/// The `slen` C's `av_parse_color` gets for the first `len` bytes of
/// `color`, or `None` when the prefix ends inside the hex marker.
///
/// C does not handle such a prefix: with `slen == hex_offset - 1` it copies
/// nothing and parses an uninitialised stack buffer (UB), and with a smaller
/// `slen` its size computation wraps to 128 and it parses past the prefix.
/// A large `len` is capped at [`COLOR_SLEN_CAP`], which also keeps C's
/// `slen - hex_offset + 1` from overflowing at `INT_MAX`.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    reason = "capped at `COLOR_SLEN_CAP`, below `INT_MAX`"
)]
fn color_slen(color: &[u8], len: usize) -> Option<c_int> {
    if len < hex_offset(color) {
        return None;
    }
    Some(len.min(COLOR_SLEN_CAP) as c_int)
}

/// The alpha specifier C's `av_parse_color` reads for `slen` (at least
/// `hex_offset(color)`): the bytes after the first `@` among the up to 127
/// it copies after the hex marker, or `None` without an `@`.
#[expect(
    clippy::cast_sign_loss,
    reason = "`slen` is at least `hex_offset(color)`, a non-negative `int`"
)]
fn alpha_suffix(color: &[u8], slen: c_int) -> Option<&[u8]> {
    let start = hex_offset(color);
    let copied = (slen as usize - start).min(127);
    let color = &color[start..color.len().min(start + copied)];
    let at = color.iter().position(|&b| b == b'@')?;
    Some(&color[at + 1..])
}

/// Whether glibc's `strtod` reads all of `s` as a NaN: after C-locale
/// whitespace and an optional sign, `nan` in any case, optionally followed
/// by a parenthesised run of ASCII letters, digits and `_`.
fn is_nan_spelling(s: &[u8]) -> bool {
    let s = match s.iter().position(|&b| !matches!(b, b' ' | b'\t'..=b'\r')) {
        Some(i) => &s[i..],
        None => return false,
    };
    let s = s
        .strip_prefix(b"+")
        .or_else(|| s.strip_prefix(b"-"))
        .unwrap_or(s);
    let Some((nan, rest)) = s.split_at_checked(3) else {
        return false;
    };
    if !nan.eq_ignore_ascii_case(b"nan") {
        return false;
    }
    match rest {
        [] => true,
        [b'(', inner @ .., b')'] => inner
            .iter()
            .all(|&b| b.is_ascii_alphanumeric() || b == b'_'),
        _ => false,
    }
}

/// The `slen` for `av_parse_color` of the first `len` bytes of `color`, or
/// `AVERROR(EINVAL)` for what C does not handle: a prefix inside the hex
/// marker (see [`color_slen`]), and an alpha specifier `strtod` reads as
/// NaN, which C range-checks with comparisons NaN fails and then converts to
/// a byte (UB).
fn checked_slen(color: &CStr, len: usize) -> Result<c_int, i32> {
    let bytes = color.to_bytes();
    let slen = color_slen(bytes, len).ok_or(AVERROR_EINVAL)?;
    if alpha_suffix(bytes, slen).is_some_and(is_nan_spelling) {
        return Err(AVERROR_EINVAL);
    }
    Ok(slen)
}

/// Wraps: `av_parse_color`
///
/// The red, green, blue and alpha bytes `color` names, in that order.
///
/// `color` is a color name from FFmpeg's table (matched regardless of ASCII
/// case, enumerated by `av_get_known_color_name`), or `RRGGBB` or `RRGGBBAA`
/// in hexadecimal, optionally after `0x` or `#`; `random` or `bikeshed` pick
/// a random color, alpha included. The alpha byte defaults to 255, and a
/// trailing `@` followed by `0x` and a hexadecimal byte, or by a decimal
/// opacity in `[0.0, 1.0]` (scaled by 255 and truncated), replaces it. Only
/// the first 127 bytes after the hex marker are read.
///
/// # Errors
///
/// Fails with `AVERROR(EINVAL)` otherwise, after logging why at `AV_LOG_ERROR` without a context.
/// An opacity `strtod` reads as NaN (`red@nan`), which C would convert to a byte (UB), fails with
/// `AVERROR(EINVAL)` before C runs, without logging.
pub fn parse_color(color: &CStr) -> Result<[u8; 4], i32> {
    parse_color_prefix(color, usize::MAX)
}

/// Wraps: `av_parse_color`
///
/// As [`parse_color`], reading the color from the first `len` bytes of
/// `color` only (the whole string when it is shorter); an invalid alpha
/// specifier is still logged with the whole string.
///
/// # Errors
///
/// A prefix that ends inside the `#` or `0x` marker fails with `AVERROR(EINVAL)` before C runs,
/// without logging: C copies no color for such a prefix and reads an uninitialised buffer, or reads
/// past the prefix.
pub fn parse_color_prefix(color: &CStr, len: usize) -> Result<[u8; 4], i32> {
    let slen = checked_slen(color, len)?;
    let mut rgba = [0u8; 4];
    let log_ctx = ptr::null_mut();
    // SAFETY: `rgba` is a writable 4-byte array and `color` a live
    // NUL-terminated string C only reads (whole, for the log message);
    // `checked_slen` keeps `slen` within `hex_offset..=COLOR_SLEN_CAP`, so C
    // copies at least its NUL and `slen - hex_offset + 1` cannot overflow,
    // and the alpha C converts is not NaN; a NULL log context is "no
    // context".
    let ret = unsafe { ffi::av_parse_color(rgba.as_mut_ptr(), color.as_ptr(), slen, log_ctx) };
    if ret < 0 { Err(ret) } else { Ok(rgba) }
}

/// Wraps: `av_parse_color`
///
/// As [`parse_color`], or with `Some(len)` as [`parse_color_prefix`], logging
/// a failure with `log_ctx` as the context (`None` for no context).
///
/// # Errors
///
/// Returns the negative `AVERROR` code `av_parse_color` returns when it fails.
pub fn parse_color_with_context(
    color: &CStr,
    len: Option<usize>,
    log_ctx: Option<ObjectRef<'_>>,
) -> Result<[u8; 4], i32> {
    let slen = checked_slen(color, len.unwrap_or(usize::MAX))?;
    let mut rgba = [0u8; 4];
    let log_ctx = log_ctx.map_or(ptr::null_mut(), |o| o.as_ptr());
    // SAFETY: as for `parse_color_prefix`; `log_ctx` is NULL or a live
    // `AVClass`-enabled object whose class callbacks `av_log` may call, as
    // `ObjectRef` guarantees for the call.
    let ret = unsafe { ffi::av_parse_color(rgba.as_mut_ptr(), color.as_ptr(), slen, log_ctx) };
    if ret < 0 { Err(ret) } else { Ok(rgba) }
}

/// Wraps: `av_parse_video_size`
///
/// The width and height, both positive, that `size` gives: a named size
/// (`vga`, `hd1080`, `4k`, ...; case-sensitive) or `<width>x<height>`, two
/// decimal integers as `strtol` reads them (leading whitespace and a sign
/// allowed) separated by any one byte. C narrows each integer to `int`
/// modulo 2^32, so `4294967297x1` is `(1, 1)`.
///
/// # Errors
///
/// Fails with `AVERROR(EINVAL)` on trailing bytes or a dimension that is not positive.
pub fn parse_video_size(size: &CStr) -> Result<(i32, i32), i32> {
    let (mut width, mut height) = (0, 0);
    // SAFETY: `width` and `height` are writable locals, and `size` a live
    // NUL-terminated string that C only reads, up to its NUL.
    let ret = unsafe { ffi::av_parse_video_size(&raw mut width, &raw mut height, size.as_ptr()) };
    if ret < 0 {
        Err(ret)
    } else {
        Ok((width, height))
    }
}

/// `av_parse_time` of `timestr` in the given mode.
fn parse_time(timestr: &CStr, duration: bool) -> Result<i64, i32> {
    let mut timeval = 0;
    // SAFETY: `timeval` is a writable local and `timestr` a live
    // NUL-terminated string that C only reads, up to its NUL.
    let ret =
        unsafe { ffi::av_parse_time(&raw mut timeval, timestr.as_ptr(), c_int::from(duration)) };
    if ret < 0 { Err(ret) } else { Ok(timeval) }
}

/// Wraps: `av_parse_time`
///
/// The microseconds since 1970-01-01 00:00:00 UTC of the date `date`:
///
/// ```text
/// [{YYYY-MM-DD|YYYYMMDD}[T|t| ]]{HH:MM:SS|HHMMSS}[.m...][Z|z|{+|-}HH[[:]MM]]
/// now
/// ```
///
/// The time is local unless `Z` or a UTC offset follows it (an offset needs
/// the date part); a missing date is today's, and `now` (any case) is the
/// current time. Up to six fractional digits count.
///
/// # Errors
///
/// Fails with `AVERROR(EINVAL)` on a malformed date, and `AVERROR(ERANGE)` when the microseconds
/// overflow an `i64`.
pub fn parse_date(date: &CStr) -> Result<i64, i32> {
    parse_time(date, false)
}

/// Wraps: `av_parse_time`
///
/// The microseconds the duration `duration` spans:
///
/// ```text
/// [-][HH:]MM:SS[.m...][s|ms|us]
/// [-]S+[.m...][s|ms|us]
/// ```
///
/// with up to 9999 hours, minutes and seconds below 60, or any `i64` count
/// of seconds (or of the unit its suffix names); up to six fractional digits
/// count (three after `ms`, none after `us`).
///
/// # Errors
///
/// Fails with `AVERROR(EINVAL)` on a malformed duration, and `AVERROR(ERANGE)` when it overflows an
/// `i64` of microseconds.
pub fn parse_duration(duration: &CStr) -> Result<i64, i32> {
    parse_time(duration, true)
}

/// Wraps: `av_find_info_tag`
///
/// The value of tag `tag` in `info`, a URL query string of the form
/// `?tag1=val1&tag2=val2...` (the leading `?` is optional), written into
/// `buf` as C writes it: `+` decoded to a space, truncated to `buf.len() - 1`
/// bytes and NUL-terminated; a tag with no `=` has the empty value. Tag names
/// are compared on their first 127 bytes. `None` when no tag matches, with
/// `buf` holding whatever C wrote for the last tag it read.
///
/// An empty `buf` cannot hold the terminator C always writes: the tag is then
/// looked up into a one-byte scratch buffer, and a match yields the empty
/// value.
pub fn find_info_tag<'b>(buf: &'b mut [u8], tag: &CStr, info: &CStr) -> Option<&'b CStr> {
    if buf.is_empty() {
        let mut scratch = [0u8; 1];
        return find_info_tag(&mut scratch, tag, info).map(|_| c"");
    }
    // C writes at most `arg_size` bytes; a longer buffer is used in part.
    let size = c_int::try_from(buf.len()).unwrap_or(c_int::MAX);
    // SAFETY: `buf` holds at least `size >= 1` writable bytes, all C writes
    // (`arg_size - 1` value bytes and the terminator); `tag` and `info` are
    // live C strings C only reads during the call.
    let found = unsafe {
        ffi::av_find_info_tag(
            buf.as_mut_ptr().cast::<c_char>(),
            size,
            tag.as_ptr(),
            info.as_ptr(),
        )
    };
    if found == 0 {
        return None;
    }
    // C NUL-terminated its value within the first `size` bytes.
    CStr::from_bytes_until_nul(buf).ok()
}

/// Wraps: `av_get_known_color_name`
///
/// Entry `index` of the table of named colors `av_parse_color` recognizes,
/// in its order (alphabetical, `"AliceBlue"` first): the color's name and
/// its red, green and blue components. `None` past the end of the table.
#[must_use]
pub fn known_color_name(index: usize) -> Option<(&'static CStr, [u8; 3])> {
    let index = c_int::try_from(index).ok()?;
    let mut rgb = ptr::null();
    // SAFETY: `rgb` is a writable slot; C bounds-checks `index` and returns
    // NULL past the table, else stores a pointer into it.
    let name = unsafe { ffi::av_get_known_color_name(index, &raw mut rgb) };
    if name.is_null() || rgb.is_null() {
        return None;
    }
    // SAFETY: a non-NULL result is an entry of the `static const` color
    // table: `name` a string literal, `rgb` its 3-byte `rgb_color` member,
    // both immutable for the program's lifetime.
    Some((unsafe { CStr::from_ptr(name) }, unsafe {
        rgb.cast::<[u8; 3]>().read()
    }))
}

// Options wave 1, batch 1: ratios, video rates, date fields and UTC times.

/// `av_parse_ratio` of `ratio`, refused as [`parse_ratio_with_context`]
/// documents.
fn parse_ratio_raw(
    ratio: &CStr,
    max: i32,
    log_offset: i32,
    log_ctx: Option<ObjectRef<'_>>,
) -> Result<Rational, i32> {
    if max < 0 || eval::refuses(ratio.to_bytes(), log_offset) {
        return Err(AVERROR_EINVAL);
    }
    let log_ctx = log_ctx.map_or(ptr::null_mut(), |o| o.as_ptr());
    let mut q = ffi::AVRational::default();
    // SAFETY: `q` is a writable local and `ratio` a live NUL-terminated
    // string C only reads; a non-negative `max` keeps `av_reduce` and
    // `av_d2q` within `int`, `refuses` rejected every expression whose
    // evaluation could reach undefined behaviour at `log_offset`, and
    // `log_ctx` is NULL or a live object whose class callbacks `av_log` may
    // call, as `ObjectRef` guarantees for the call.
    let ret = unsafe { ffi::av_parse_ratio(&raw mut q, ratio.as_ptr(), max, log_offset, log_ctx) };
    if ret < 0 {
        Err(ret)
    } else {
        Ok(Rational::from_raw(q))
    }
}

/// Wraps: `av_parse_ratio`
///
/// The ratio `ratio` gives, both terms at most `max` in magnitude:
///
/// - `<num>:<den>` and nothing after it, two decimal integers as glibc's `sscanf` reads `%d`
///   (leading whitespace and a sign allowed, narrowed to `int` modulo 2^32, `-1` beyond the `long`
///   range), reduced to lowest terms ([`reduce`](super::rational::reduce)), the closest
///   approximation within `max` when it does not fit; `0:0` and `x:0` stay undefined and infinite;
/// - anything else, an expression of the evaluator's own constants and functions
///   ([`eval::parse_and_eval`] with no constants or functions), whose value [`Rational::from_f64`]
///   approximates within `max`.
///
/// # Errors
///
/// The evaluator's: `AVERROR(EINVAL)` for an expression that does not parse
/// or evaluates to NaN (logged at `AV_LOG_ERROR` in an `"Eval"` context),
/// `AVERROR(ENOMEM)`. Also `AVERROR(EINVAL)`, before C runs, for a negative
/// `max`, which C's reduction cannot bound, and for an expression
/// [`eval::parse_and_eval`] refuses.
///
/// ```
/// use ffmpeg_bindings::libavutil::libavutil::parseutils::parse_ratio;
/// use ffmpeg_bindings::libavutil::libavutil::rational::Rational;
/// assert_eq!(
///     parse_ratio(c"16:12", 255)
///         .map(Rational::into_raw)
///         .map(|q| (q.num, q.den)),
///     Ok((4, 3))
/// );
/// assert_eq!(
///     parse_ratio(c"0.75", 255)
///         .map(Rational::into_raw)
///         .map(|q| (q.num, q.den)),
///     Ok((3, 4))
/// );
/// ```
pub fn parse_ratio(ratio: &CStr, max: i32) -> Result<Rational, i32> {
    parse_ratio_raw(ratio, max, 0, None)
}

/// Wraps: `av_parse_ratio`
///
/// As [`parse_ratio`], logging an expression's errors with `log_offset`
/// added to their level, as messages of an `"Eval"` context whose parent is
/// `log_ctx` (`None` for none); `ffi::AV_LOG_MAX_OFFSET` silences them, as
/// C's `av_parse_ratio_quiet` does.
///
/// # Errors
///
/// As [`parse_ratio`]; the refused expressions include a `log_offset` that
/// overflows an `int` added to C's levels.
pub fn parse_ratio_with_context(
    ratio: &CStr,
    max: i32,
    log_offset: i32,
    log_ctx: Option<ObjectRef<'_>>,
) -> Result<Rational, i32> {
    parse_ratio_raw(ratio, max, log_offset, log_ctx)
}

/// Wraps: `av_parse_video_rate`
///
/// The frame rate `rate` gives, both terms positive: a named rate (`ntsc`,
/// `pal`, `qntsc`, `qpal`, `sntsc`, `spal`, `film`, `ntsc-film`;
/// case-sensitive), or a ratio as [`parse_ratio`] reads it, quietly,
/// within 1001000 (`"30000/1001"`, `"29.97"`), or within `i32::MAX` when that
/// leaves a zero term.
///
/// # Errors
///
/// `AVERROR(EINVAL)` for a rate that does not parse or whose terms are not
/// both positive, `AVERROR(ENOMEM)`; nothing is logged. Also
/// `AVERROR(EINVAL)`, before C runs, for an expression
/// [`eval::parse_and_eval`] refuses.
pub fn parse_video_rate(rate: &CStr) -> Result<Rational, i32> {
    if eval::refuses(rate.to_bytes(), ffi::AV_LOG_MAX_OFFSET.cast_signed()) {
        return Err(AVERROR_EINVAL);
    }
    let mut q = ffi::AVRational::default();
    // SAFETY: `q` is a writable local and `rate` a live NUL-terminated
    // string C only reads; C's quiet ratio parses use non-negative bounds,
    // and `refuses` rejected every expression whose quiet evaluation could
    // reach undefined behaviour.
    let ret = unsafe { ffi::av_parse_video_rate(&raw mut q, rate.as_ptr()) };
    if ret < 0 {
        Err(ret)
    } else {
        Ok(Rational::from_raw(q))
    }
}

/// Wraps: `av_small_strptime`
///
/// Reads the start of `s` as `fmt` describes it, storing each field it
/// reads in `tm`, and returns the rest of `s`: the bytes after the last one
/// read, possibly the empty string. `fmt` is a subset of `strptime`'s:
///
/// - `%H`: an hour, 0 to 23, of at most 2 digits (`tm_hour`);
/// - `%J`: hours, of at most 4 digits (`tm_hour`);
/// - `%M`, `%S`: a minute or second, 0 to 59, of at most 2 digits (`tm_min`, `tm_sec`);
/// - `%Y`: a year of at most 4 digits (`tm_year`, less 1900);
/// - `%m`: a month, 1 to 12, of at most 2 digits (`tm_mon`, less 1);
/// - `%d`: a day of the month, 1 to 31, of at most 2 digits (`tm_mday`);
/// - `%T`: `%H:%M:%S`;
/// - `%b`, `%B`, `%h`: a month's English name, its first three letters or all of it, in any case
///   (`tm_mon`);
/// - `%%`: a `%`;
/// - a white-space byte: any run of white space, possibly empty;
/// - any other byte: itself.
///
/// Numbers are decimal digits only: no sign or white space.
///
/// `None` when `s` does not match, or `fmt` holds another `%` conversion
/// or ends in `%`; `tm` then keeps the fields stored before the mismatch.
/// No other field of `tm` is written.
///
/// ```
/// use ffmpeg_bindings::libavutil::libavutil::parseutils::small_strptime;
/// use glibc_bindings::libc::time::bits::types::struct_tm_h::Tm;
/// let mut tm = Tm::new();
/// let rest = small_strptime(c"2024-02-29 12:34:56 UTC", c"%Y-%m-%d %T", tm.as_mut());
/// assert_eq!(rest, Some(c" UTC"));
/// assert_eq!(
///     (tm.as_ref().year(), tm.as_ref().mon(), tm.as_ref().mday()),
///     (124, 1, 29)
/// );
/// assert_eq!(tm.as_ref().sec(), 56);
/// ```
#[must_use]
pub fn small_strptime<'s>(s: &'s CStr, fmt: &CStr, mut tm: TmMut<'_>) -> Option<&'s CStr> {
    // SAFETY: `s` and `fmt` are live NUL-terminated strings C only reads,
    // never past their NULs (a trailing `%` fails as it reads the NUL), and
    // `tm` a live `struct tm` of which C writes only integer fields.
    let rest = unsafe { ffi::av_small_strptime(s.as_ptr(), fmt.as_ptr(), tm.as_mut_tm_ptr()) };
    // SAFETY: a non-NULL result points into `s`, at a byte C has not read
    // past or at its NUL, so it is a suffix of the same string, live for
    // `'s`.
    NonNull::new(rest).map(|p| unsafe { CStr::from_ptr(p.as_ptr()) })
}

/// The `int` sum C's `av_timegm` computes the days of `tm` with, and its
/// seconds of the day, or `None` when either overflows an `int` (C's signed
/// overflow, undefined behaviour). Mirrors C's operations in their order.
fn timegm_terms(tm: TmRef<'_>) -> Option<(c_int, c_int)> {
    let (mut y, mut m) = (tm.year().checked_add(1900)?, tm.mon().checked_add(1)?);
    if m < 3 {
        m = m.checked_add(12)?;
        y = y.checked_sub(1)?;
    }
    let days = tm
        .mday()
        .checked_add(153i32.checked_mul(m)?.checked_sub(457)? / 5)?
        .checked_add(365i32.checked_mul(y)?)?
        .checked_add(y / 4)?
        .checked_sub(y / 100)?
        .checked_add(y / 400)?
        .checked_sub(719_469)?;
    let secs = 3600i32
        .checked_mul(tm.hour())?
        .checked_add(60i32.checked_mul(tm.min())?)?
        .checked_add(tm.sec())?;
    Some((days, secs))
}

/// Wraps: `av_timegm`
///
/// The seconds since 1970-01-01 00:00:00 UTC of the UTC time `tm`, from its
/// year, month, day, hour, minute and second; the other fields are ignored.
/// The fields are not range-checked: an overflowing one carries into the
/// next (minute 60 is the next hour's first, month 12 the next year's
/// January; a day 0 is the month's eve).
///
/// `None` when the computation overflows C's `int` arithmetic, which C
/// does not check (undefined behaviour): for a year beyond about 5.8
/// million either way, or an hour, minute or second whose seconds of the
/// day exceed the `int` range.
///
/// ```
/// use ffmpeg_bindings::libavutil::libavutil::parseutils::timegm;
/// use glibc_bindings::libc::time::bits::types::struct_tm_h::Tm;
/// let mut tm = Tm::new();
/// let mut t = tm.as_mut();
/// t.set_year(100);
/// t.set_mday(1);
/// assert_eq!(timegm(tm.as_ref()), Some(946_684_800));
/// ```
#[must_use]
pub fn timegm(tm: TmRef<'_>) -> Option<i64> {
    timegm_terms(tm)?;
    // SAFETY: the shared handle covers a live `struct tm`, which C only
    // reads although its parameter is not `const`; `timegm_terms` checked
    // that none of C's `int` operations overflows.
    Some(unsafe { ffi::av_timegm(tm.as_tm_ptr().cast_mut()) })
}

/// Whether `av_parse_color(color)` (the whole string, `slen` of -1) reaches
/// an alpha specifier `strtod` reads as NaN (`"red@nan"`): C then stores the
/// NaN in a `uint8_t`, undefined behaviour (`parseutils.c:433`). As
/// [`checked_slen`] decides it for [`parse_color`], for an option value
/// `av_opt_set` hands C whole.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    reason = "`COLOR_SLEN_CAP` is below `INT_MAX`"
)]
pub(crate) fn color_alpha_is_nan(color: &[u8]) -> bool {
    alpha_suffix(color, COLOR_SLEN_CAP as c_int).is_some_and(is_nan_spelling)
}

#[cfg(test)]
#[expect(
    clippy::cast_possible_truncation,
    reason = "test inputs are chosen within range, and the casts mirror the C conversions under comparison"
)]
mod unit_tests {
    use super::*;

    #[test]
    fn hex_offset_matches_only_lowercase_markers() {
        assert_eq!(hex_offset(b""), 0);
        assert_eq!(hex_offset(b"#"), 1);
        assert_eq!(hex_offset(b"#ff0000"), 1);
        assert_eq!(hex_offset(b"0"), 0);
        assert_eq!(hex_offset(b"0x"), 2);
        assert_eq!(hex_offset(b"0xff0000"), 2);
        assert_eq!(hex_offset(b"0Xff0000"), 0);
        assert_eq!(hex_offset(b"x0"), 0);
        assert_eq!(hex_offset(b"red"), 0);
    }

    #[test]
    fn color_slen_refuses_prefixes_inside_the_marker_and_caps_the_rest() {
        assert_eq!(color_slen(b"#ff0000", 0), None);
        assert_eq!(color_slen(b"#ff0000", 1), Some(1));
        assert_eq!(color_slen(b"0xff0000", 0), None);
        assert_eq!(color_slen(b"0xff0000", 1), None);
        assert_eq!(color_slen(b"0xff0000", 2), Some(2));
        assert_eq!(color_slen(b"red", 0), Some(0));
        assert_eq!(color_slen(b"red", 3), Some(3));
        assert_eq!(color_slen(b"red", 255), Some(255));
        assert_eq!(color_slen(b"red", 256), Some(256));
        assert_eq!(color_slen(b"red", 257), Some(256));
        assert_eq!(color_slen(b"red", usize::MAX), Some(256));
        assert_eq!(color_slen(b"", usize::MAX), Some(256));
    }

    #[test]
    fn the_cap_is_past_every_copy_c_makes() {
        // C copies `min(slen - hex_offset + 1, 128)` bytes: at the cap the
        // first operand is above 128 for every marker.
        for offset in 0..=2 {
            assert!(COLOR_SLEN_CAP - offset + 1 > 128);
        }
        const { assert!(COLOR_SLEN_CAP < c_int::MAX as usize) };
    }

    #[test]
    fn alpha_suffix_follows_the_first_at_within_the_copied_bytes() {
        assert_eq!(alpha_suffix(b"red", 256), None);
        assert_eq!(alpha_suffix(b"red@0.5", 256), Some(&b"0.5"[..]));
        assert_eq!(alpha_suffix(b"red@0.5@1", 256), Some(&b"0.5@1"[..]));
        assert_eq!(alpha_suffix(b"#ff0000@nan", 256), Some(&b"nan"[..]));
        assert_eq!(alpha_suffix(b"0xff0000@", 256), Some(&b""[..]));
        // The prefix ends before the `@`, or within the specifier.
        assert_eq!(alpha_suffix(b"red@nan", 3), None);
        assert_eq!(alpha_suffix(b"red@nan", 4), Some(&b""[..]));
        assert_eq!(alpha_suffix(b"red@nan", 6), Some(&b"na"[..]));
        assert_eq!(alpha_suffix(b"#ff0000@nan", 8), Some(&b""[..]));
        assert_eq!(alpha_suffix(b"#ff0000@nan", 11), Some(&b"nan"[..]));
        // C copies at most 127 bytes after the marker.
        let mut long = vec![b'a'; 125];
        long.extend_from_slice(b"@nan");
        assert_eq!(alpha_suffix(&long, 256), Some(&b"n"[..]));
        let mut long = vec![b'#'; 128];
        long.extend_from_slice(b"@nan");
        assert_eq!(alpha_suffix(&long, 256), None);
    }

    #[test]
    fn is_nan_spelling_examples() {
        for s in [
            &b"nan"[..],
            b"NaN",
            b"-nan",
            b"+NAN",
            b"\x0b nan",
            b"nan()",
            b"nan(a_1)",
        ] {
            assert!(is_nan_spelling(s), "{s:?}");
        }
        for s in [
            &b""[..],
            b"na",
            b"nanx",
            b"nan ",
            b"nan(",
            b"nan(-)",
            b"+-nan",
            b"0x1",
            b"inf",
        ] {
            assert!(!is_nan_spelling(s), "{s:?}");
        }
    }

    unsafe extern "C" {
        fn strtod(s: *const c_char, end: *mut *mut c_char) -> f64;
    }

    /// Whether libc's `strtod` reads all of `s` as a NaN.
    fn c_reads_nan(s: &[u8]) -> bool {
        let s = std::ffi::CString::new(s).unwrap();
        let mut end = ptr::null_mut();
        // SAFETY: `s` is a live NUL-terminated string and `end` a writable
        // local, which `strtod` points into `s`.
        let d = unsafe { strtod(s.as_ptr(), &raw mut end) };
        // SAFETY: `end` points into `s`, at or before its NUL.
        d.is_nan() && unsafe { *end } == 0
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn is_nan_spelling_matches_libc_strtod() {
        let fixed: [&[u8]; 28] = [
            b"nan",
            b"NaN",
            b"NAN",
            b"+nan",
            b"-nan",
            b" nan",
            b"\t\n\x0b\x0c\r nan",
            b"nan()",
            b"nan(1)",
            b"nan(abc_XYZ09)",
            b"nan(",
            b"nan)",
            b"nan(a-b)",
            b"nan(a b)",
            b"nanx",
            b"na",
            b"",
            b" ",
            b"+-nan",
            b"--nan",
            b"0.5",
            b"inf",
            b"nan ",
            b"nan(0x1f)",
            b"\xa0nan",
            b"nan\xff",
            b"1e999",
            b"0xnan",
        ];
        for s in fixed {
            assert_eq!(
                is_nan_spelling(s),
                c_reads_nan(s),
                "{:?}",
                s.escape_ascii().to_string()
            );
        }
        // Every string of up to four bytes over an alphabet that reaches
        // each branch. Under BSan the 38k four-byte strings alone take over
        // 50 minutes (each a `CString` and a libc call), so the sweep stops
        // at three bytes there; plain and the other sanitizers cover four.
        let alphabet = b" \x0b+-nNaA()_1x.";
        let bsan = std::env::var("CRUSTIFY_FFMPEG_VARIANT").as_deref() == Ok("bsan");
        let mut s = Vec::new();
        for len in 0..=if bsan { 3 } else { 4u32 } {
            for mut i in 0..alphabet.len().pow(len) {
                s.clear();
                for _ in 0..len {
                    s.push(alphabet[i % alphabet.len()]);
                    i /= alphabet.len();
                }
                assert_eq!(
                    is_nan_spelling(&s),
                    c_reads_nan(&s),
                    "{:?}",
                    s.escape_ascii().to_string()
                );
            }
        }
    }

    /// The corrected behaviour: a prefix that ends inside the marker fails
    /// instead of reading an uninitialised buffer (`#`, length 0) or the
    /// bytes after the prefix (`0x`, length 0, which C parses as red).
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn prefixes_inside_the_marker_fail_before_c() {
        for (color, len) in [(c"#ff0000", 0), (c"0xff0000", 0), (c"0xff0000", 1)] {
            assert_eq!(
                parse_color_prefix(color, len),
                Err(AVERROR_EINVAL),
                "{color:?} {len}"
            );
            let ret = parse_color_with_context(color, Some(len), None);
            assert_eq!(ret, Err(AVERROR_EINVAL), "{color:?} {len}");
        }
        // The marker alone is a valid, empty, prefix.
        assert_eq!(parse_color_prefix(c"#ff0000", 1), Err(AVERROR_EINVAL));
        assert_eq!(parse_color_prefix(c"0xff0000", 2), Err(AVERROR_EINVAL));
        assert_eq!(parse_color_prefix(c"#ff0000", 7), Ok([255, 0, 0, 255]));
        assert_eq!(parse_color_prefix(c"0xff0000", 8), Ok([255, 0, 0, 255]));
    }

    /// The corrected behaviour: a NaN opacity fails instead of reaching C's
    /// conversion of NaN to a byte (`UBSan`: `parseutils.c:433`).
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn nan_opacities_fail_before_c() {
        for color in [
            c"red@nan",
            c"#ff0000@NaN",
            c"0xff000080@-nan",
            c"red@ nan(1)",
            c"random@nan",
        ] {
            assert_eq!(parse_color(color), Err(AVERROR_EINVAL), "{color:?}");
            let len = color.count_bytes();
            assert_eq!(
                parse_color_prefix(color, len),
                Err(AVERROR_EINVAL),
                "{color:?}"
            );
        }
        // Cut off before the specifier, the color is valid.
        assert_eq!(parse_color_prefix(c"red@nan", 3), Ok([255, 0, 0, 255]));
        // A NaN spelling C rejects itself still reaches C.
        assert_eq!(parse_color(c"red@nanx"), Err(AVERROR_EINVAL));
        assert_eq!(parse_color(c"red@0.5"), Ok([255, 0, 0, 127]));
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn parse_time_selects_the_mode() {
        assert_eq!(parse_time(c"1.5", true), Ok(1_500_000));
        assert_eq!(parse_time(c"1.5", false), Err(AVERROR_EINVAL));
        assert_eq!(parse_time(c"1970-01-01T00:00:01Z", false), Ok(1_000_000));
        assert_eq!(
            parse_time(c"1970-01-01T00:00:01Z", true),
            Err(AVERROR_EINVAL)
        );
    }

    #[test]
    fn nan_alphas_are_found_where_c_reads_them() {
        for c in [
            "red@nan",
            "red@NaN",
            "#ff0000@ -nan",
            "0xff0000@+NAN(1)",
            "red@\tnan",
        ] {
            assert!(color_alpha_is_nan(c.as_bytes()), "{c}");
        }
        for c in [
            "red",
            "red@0.5",
            "red@0xnan",
            "red@inf",
            "nan",
            "red@na",
            "@",
            "red@nanx",
        ] {
            assert!(!color_alpha_is_nan(c.as_bytes()), "{c}");
        }
        // C reads only the first 127 bytes after the prefix.
        let mut long = vec![b'a'; 124];
        long.extend_from_slice(b"@nan");
        assert!(!color_alpha_is_nan(&long));
        long.remove(0);
        assert!(color_alpha_is_nan(&long));
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn an_empty_buffer_reports_a_match_with_the_empty_value() {
        let mut empty = [0u8; 0];
        assert_eq!(find_info_tag(&mut empty, c"a", c"?a=1"), Some(c""));
        assert_eq!(find_info_tag(&mut empty, c"b", c"?a=1"), None);
    }
    /// Options wave 1, batch 1: ratio refusals and `av_timegm`'s overflow
    /// model.
    mod timegm_and_ratios {
        use glibc_bindings::libc::time::bits::types::struct_tm_h::Tm;

        use super::*;

        /// `av_timegm`'s computation in `i64`, with whether every
        /// intermediate value of C's `int` operations fits an `int`.
        fn model(year: i32, mon: i32, mday: i32, hour: i32, min: i32, sec: i32) -> (i64, bool) {
            let fits = core::cell::Cell::new(true);
            let int = |v: i64| {
                fits.set(fits.get() && i32::try_from(v).is_ok());
                v
            };
            let (mut y, mut m) = (int(i64::from(year) + 1900), int(i64::from(mon) + 1));
            if m < 3 {
                m = int(m + 12);
                y = int(y - 1);
            }
            let a = int(int(153 * m) - 457) / 5;
            let mut days = int(i64::from(mday) + a);
            days = int(days + int(365 * y));
            days = int(days + y / 4);
            days = int(days - y / 100);
            days = int(days + y / 400);
            days = int(days - 719_469);
            let mut secs = int(int(3600 * i64::from(hour)) + int(60 * i64::from(min)));
            secs = int(secs + i64::from(sec));
            (86_400 * days + secs, fits.get())
        }

        fn tm(year: i32, mon: i32, mday: i32, hour: i32, min: i32, sec: i32) -> Tm {
            let mut t = Tm::new();
            let mut m = t.as_mut();
            m.set_year(year);
            m.set_mon(mon);
            m.set_mday(mday);
            m.set_hour(hour);
            m.set_min(min);
            m.set_sec(sec);
            t
        }

        /// Values around each bound C's `int` operations cross.
        const EDGES: [i32; 23] = [
            i32::MIN,
            i32::MIN + 1,
            -5_883_517 - 1900,
            -5_883_516 - 1900,
            -596_524,
            -596_523,
            -35_791_395,
            -1,
            0,
            1,
            2,
            11,
            12,
            59,
            60,
            596_523,
            596_524,
            35_791_394,
            35_791_395,
            5_883_516 - 1900,
            5_883_517 - 1900,
            i32::MAX - 1,
            i32::MAX,
        ];

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn timegm_refuses_exactly_cs_int_overflows() {
            let mut state = 0x1234_5678_9ABC_DEF0u64;
            let mut next = move || {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                EDGES[(state % EDGES.len() as u64) as usize]
            };
            for _ in 0..200_000 {
                let f = [next(), next(), next(), next(), next(), next()];
                let t = tm(f[0], f[1], f[2], f[3], f[4], f[5]);
                let (value, fits) = model(f[0], f[1], f[2], f[3], f[4], f[5]);
                assert_eq!(timegm_terms(t.as_ref()).is_some(), fits, "{f:?}");
                assert_eq!(timegm(t.as_ref()), fits.then_some(value), "{f:?}");
            }
        }

        /// As above for the check alone, which runs no C (Miri).
        #[test]
        fn timegm_terms_flag_every_int_overflow() {
            let windows = EDGES.windows(6);
            for f in windows.chain([&[0, 0, 1, 0, 0, 0][..], &[70, 11, 31, 23, 59, 59]]) {
                let t = tm(f[0], f[1], f[2], f[3], f[4], f[5]);
                let (_, fits) = model(f[0], f[1], f[2], f[3], f[4], f[5]);
                assert_eq!(timegm_terms(t.as_ref()).is_some(), fits, "{f:?}");
            }
            for &e in &EDGES {
                for field in 0..6 {
                    let mut f = [70, 0, 1, 0, 0, 0];
                    f[field] = e;
                    let t = tm(f[0], f[1], f[2], f[3], f[4], f[5]);
                    let (_, fits) = model(f[0], f[1], f[2], f[3], f[4], f[5]);
                    assert_eq!(timegm_terms(t.as_ref()).is_some(), fits, "{f:?}");
                }
            }
        }

        /// The corrected behaviour: C's signed overflows (`UBSan`:
        /// `parseutils.c:577`, `:585`, `:587`) become `None`.
        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn timegm_overflows_are_none() {
            assert_eq!(timegm(tm(6_000_000, 0, 1, 0, 0, 0).as_ref()), None);
            assert_eq!(timegm(tm(70, 0, 1, 600_000, 0, 0).as_ref()), None);
            assert_eq!(timegm(tm(70, i32::MAX, 1, 0, 0, 0).as_ref()), None);
            assert_eq!(timegm(tm(70, 0, i32::MAX, 0, 0, 0).as_ref()), None);
            assert_eq!(timegm(tm(70, 0, 1, 0, 0, 0).as_ref()), Some(0));
            assert_eq!(timegm(tm(70, 0, 0, 0, 0, -1).as_ref()), Some(-86_401));
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn negative_bounds_and_refused_expressions_fail_before_c() {
            assert_eq!(parse_ratio(c"1:2", -1), Err(AVERROR_EINVAL));
            assert_eq!(
                parse_ratio(c"1:2", 0)
                    .map(Rational::into_raw)
                    .map(|q| (q.num, q.den)),
                Ok((0, 1))
            );
            assert_eq!(parse_ratio(c"ld(1e300)", 100), Err(AVERROR_EINVAL));
            assert_eq!(parse_ratio(c"st(0,1)+random(0)", 100), Err(AVERROR_EINVAL));
            assert_eq!(
                parse_ratio_with_context(c"1:2", 10, i32::MAX, None),
                Err(AVERROR_EINVAL)
            );
            assert_eq!(parse_video_rate(c"ld(-1e300)"), Err(AVERROR_EINVAL));
            assert_eq!(
                parse_video_rate(c"ntsc")
                    .map(Rational::into_raw)
                    .map(|q| (q.num, q.den)),
                Ok((30000, 1001))
            );
        }
    }
}
