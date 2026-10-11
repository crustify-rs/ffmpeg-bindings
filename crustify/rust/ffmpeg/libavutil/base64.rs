//! Safe wrappers over `libavutil/base64.h`.

use super::ffi;
use core::ffi::CStr;

fn encoded_size(input_len: usize) -> Result<usize, i32> {
    // C rejects this bound before evaluating its signed-int size expression.
    if input_len >= (u32::MAX / 4) as usize {
        return Err(-(ffi::EINVAL as i32));
    }
    Ok(input_len.div_ceil(3) * 4 + 1)
}

/// The capacity C may use of an output buffer: its size field is an `int`,
/// and C never writes more than the data needs, so a longer buffer offers C
/// `INT_MAX` bytes, as a C caller holding it would.
fn c_capacity(output_len: usize) -> i32 {
    i32::try_from(output_len).unwrap_or(i32::MAX)
}

/// The decoding capacity for an output of `output_len` bytes. A longer
/// output than C can address is offered as `INT_MAX` bytes only when the
/// input, of `input_len()` characters before its NUL, cannot decode to more:
/// `AV_BASE64_DECODE_SIZE`, six bits per character in whole bytes.
fn decode_capacity(output_len: usize, input_len: impl FnOnce() -> usize) -> Result<i32, i32> {
    let capacity = c_capacity(output_len);
    if output_len > capacity as usize {
        let len = input_len();
        if len / 4 * 3 + len % 4 * 3 / 4 > capacity as usize {
            return Err(-(ffi::EINVAL as i32));
        }
    }
    Ok(capacity)
}

/// Wraps: av_base64_encode
///
/// Encode into `output`, including a terminating NUL. Return a view tied to
/// its exclusive borrow. Insufficient space or an input too long for C's
/// `int` size arithmetic returns `-EINVAL` without writing anything.
pub fn encode<'a>(input: &[u8], output: &'a mut [u8]) -> Result<&'a CStr, i32> {
    let needed = encoded_size(input.len())?;
    // `needed` stays below `INT_MAX`, so a buffer of at least `needed` bytes
    // offers C at least `needed`.
    let capacity = c_capacity(output.len());
    if output.len() < needed {
        return Err(-(ffi::EINVAL as i32));
    }
    // SAFETY: lengths fit C's int arithmetic and output includes `needed`
    // bytes for the encoded data and NUL. The two borrows cannot overlap;
    // C retains neither pointer and returns the output pointer on success.
    let ptr = unsafe {
        ffi::av_base64_encode(
            output.as_mut_ptr().cast(),
            capacity,
            input.as_ptr(),
            input.len() as i32,
        )
    };
    if ptr.is_null() {
        return Err(-(ffi::EINVAL as i32));
    }
    Ok(CStr::from_bytes_with_nul(&output[..needed]).expect("C base64 encoder terminates ASCII"))
}

/// Wraps: av_base64_decode
///
/// Decode into `output`. Like the C implementation, a short destination
/// truncates the decoded bytes but still checks the rest of the input.
/// Invalid data returns C's negative error; earlier bytes may be written.
/// C stops at the first NUL or `=`, without requiring canonical padding.
/// An output longer than `INT_MAX` bytes is offered to C as `INT_MAX` bytes;
/// if the input could decode to more than that, `-EINVAL` is returned
/// without writing anything, since C could not report such a length.
pub fn decode(input: &CStr, output: &mut [u8]) -> Result<usize, i32> {
    let capacity = decode_capacity(output.len(), || input.count_bytes())?;
    let mut empty = 0u8;
    let dst = if output.is_empty() {
        &raw mut empty
    } else {
        output.as_mut_ptr()
    };
    // SAFETY: the NUL-terminated input stops C before any read beyond it.
    // C writes at most capacity bytes, including its speculative 4-byte store
    // only when more than 3 bytes remain. A live byte represents empty output.
    // Both buffers are borrowed only for the call and cannot overlap.
    let ret = unsafe { ffi::av_base64_decode(dst, input.as_ptr(), capacity) };
    if ret < 0 { Err(ret) } else { Ok(ret as usize) }
}

/// Wraps: av_base64_decode
///
/// Check C's accepted base64 syntax without producing decoded data.
pub fn validate(input: &CStr) -> Result<(), i32> {
    // SAFETY: a null output selects C's validity-only path; the readable
    // NUL-terminated input bounds every read and C retains no pointer.
    let ret = unsafe { ffi::av_base64_decode(core::ptr::null_mut(), input.as_ptr(), 0) };
    if ret < 0 { Err(ret) } else { Ok(()) }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn rust_size_arithmetic_and_rejections() {
        assert_eq!(encoded_size(0), Ok(1));
        assert_eq!(encoded_size(1), Ok(5));
        assert_eq!(encoded_size(3), Ok(5));
        assert_eq!(encoded_size(4), Ok(9));
        let limit = (u32::MAX / 4) as usize;
        assert_eq!(encoded_size(limit - 1), Ok((limit - 1).div_ceil(3) * 4 + 1));
        assert!(encoded_size(limit).is_err());
        assert!(encoded_size(usize::MAX).is_err());
        let mut output = [0xAA; 4];
        assert!(encode(b"abc", &mut output).is_err());
        assert_eq!(output, [0xAA; 4]);
        assert!(encode(b"", &mut []).is_err());
    }

    #[test]
    fn outputs_beyond_int_max_offer_c_int_max() {
        let max = i32::MAX as usize;
        for len in [0, 1, max - 1, max] {
            assert_eq!(c_capacity(len), len as i32);
            assert_eq!(
                decode_capacity(len, || unreachable!("no length needed")),
                Ok(len as i32)
            );
        }
        for len in [max + 1, usize::MAX] {
            assert_eq!(c_capacity(len), i32::MAX);
        }
        // 715827882 groups of four characters decode to 2147483646 bytes; a
        // tail of one character adds no whole byte, of two one byte (exactly
        // `INT_MAX`), and of three two bytes, one more than C can report.
        let fits = 715_827_882 * 4 + 1;
        for (input_len, expected) in [
            (0, Ok(i32::MAX)),
            (fits, Ok(i32::MAX)),
            (fits + 1, Ok(i32::MAX)),
            (fits + 2, Err(-(ffi::EINVAL as i32))),
            (usize::MAX, Err(-(ffi::EINVAL as i32))),
        ] {
            assert_eq!(
                decode_capacity(max + 1, || input_len),
                expected,
                "{input_len}"
            );
        }
    }
}
