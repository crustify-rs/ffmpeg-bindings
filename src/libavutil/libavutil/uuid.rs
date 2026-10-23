//! Safe wrappers over `libavutil/uuid.h`.
//! UUIDs are opaque 16-byte values, not allocated foreign objects.

use core::ffi::CStr;

use super::ffi;

/// Replaces: `av_uuid_copy`
///
/// Copy the complete UUID. This is equivalent to C's inline memcpy, with
/// Rust borrows excluding overlap; there is no foreign lifetime to preserve.
pub fn copy(destination: &mut [u8; 16], source: &[u8; 16]) {
    *destination = *source;
}

/// Replaces: `av_uuid_equal`
///
/// Compare all sixteen bytes (equivalent to C's inline memcmp).
#[must_use]
pub fn equal(left: &[u8; 16], right: &[u8; 16]) -> bool {
    left == right
}

/// Replaces: `av_uuid_nil`
///
/// Set every byte to zero (equivalent to C's inline memset).
pub fn nil(uuid: &mut [u8; 16]) {
    *uuid = [0; 16];
}

/// Wraps: `av_uuid_parse`
///
/// Parse C's 36-byte, case-insensitive UUID representation. On failure,
/// return C's error rather than exposing a partially initialized value.
///
/// # Errors
///
/// Returns the negative `AVERROR` code `av_uuid_parse` returns when it fails.
pub fn parse(input: &CStr) -> Result<[u8; 16], i32> {
    let mut uuid = [0; 16];
    // SAFETY: input is NUL-terminated, output is exclusive 16-byte storage.
    // C validates length before reading digits and retains neither pointer.
    let ret = unsafe { ffi::av_uuid_parse(input.as_ptr(), uuid.as_mut_ptr()) };
    if ret < 0 { Err(ret) } else { Ok(uuid) }
}

/// Wraps: `av_uuid_parse_range`
///
/// Parse a byte range without requiring a terminating NUL. As in C, the
/// four separator positions are skipped, not validated as hyphens.
///
/// # Errors
///
/// Returns the negative `AVERROR` code `av_uuid_parse_range` returns when it fails.
pub fn parse_range(input: &[u8]) -> Result<[u8; 16], i32> {
    if input.len() != 36 {
        return Err(-(ffi::EINVAL.cast_signed()));
    }
    let mut uuid = [0; 16];
    // SAFETY: `36` is within the block `input.as_ptr()` points to, as the operation below
    // relies on, so the offset stays in bounds; nothing is accessed here.
    let elem = unsafe { input.as_ptr().add(36) };
    // SAFETY: both endpoints belong to the same 36-byte allocation, the end
    // is one-past and never read; output holds all 16 bytes and cannot alias
    // input. C borrows both only for this call.
    let ret =
        unsafe { ffi::av_uuid_parse_range(input.as_ptr().cast(), elem.cast(), uuid.as_mut_ptr()) };
    if ret < 0 { Err(ret) } else { Ok(uuid) }
}

/// Wraps: `av_uuid_unparse`
///
/// Serialize lowercase text plus its NUL into an exact-size destination.
///
/// # Panics
///
/// Only if C's UUID formatter leaves the output without its NUL.
pub fn unparse<'a>(uuid: &[u8; 16], output: &'a mut [u8; 37]) -> &'a CStr {
    // SAFETY: C reads exactly 16 bytes, writes exactly 37 including NUL,
    // retains no pointers, and Rust borrows exclude overlap.
    unsafe { ffi::av_uuid_unparse(uuid.as_ptr(), output.as_mut_ptr().cast()) };
    CStr::from_bytes_with_nul(output).expect("C UUID formatter terminates ASCII")
}

/// Wraps: `av_uuid_urn_parse`
///
/// Parse a UUID prefixed by `urn:uuid:` (case-insensitive), as C does.
///
/// # Errors
///
/// Returns the negative `AVERROR` code `av_uuid_urn_parse` returns when it fails.
pub fn urn_parse(input: &CStr) -> Result<[u8; 16], i32> {
    let mut uuid = [0; 16];
    // SAFETY: input is NUL-terminated, output is exclusive 16-byte storage.
    // C checks the prefix and UUID length before reading digits; no retention.
    let ret = unsafe { ffi::av_uuid_urn_parse(input.as_ptr(), uuid.as_mut_ptr()) };
    if ret < 0 { Err(ret) } else { Ok(uuid) }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn rust_inline_equivalents_and_range_rejection() {
        let source = [255; 16];
        let mut destination = [0; 16];
        copy(&mut destination, &source);
        assert!(equal(&destination, &source));
        nil(&mut destination);
        assert!(equal(&destination, &[0; 16]));
        assert!(!equal(&destination, &source));
        for len in [0, 35, 37, 100] {
            assert!(parse_range(&vec![b'0'; len]).is_err());
        }
    }
}
