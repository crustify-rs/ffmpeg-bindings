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

/// Wraps: av_base64_encode
///
/// Encode into `output`, including a terminating NUL. Return a view tied to
/// its exclusive borrow. Insufficient space or an unrepresentable C length
/// returns `-EINVAL` without writing anything.
pub fn encode<'a>(input: &[u8], output: &'a mut [u8]) -> Result<&'a CStr, i32> {
    let needed = encoded_size(input.len())?;
    let capacity = i32::try_from(output.len()).map_err(|_| -(ffi::EINVAL as i32))?;
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
pub fn decode(input: &CStr, output: &mut [u8]) -> Result<usize, i32> {
    let capacity = i32::try_from(output.len()).map_err(|_| -(ffi::EINVAL as i32))?;
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
}
