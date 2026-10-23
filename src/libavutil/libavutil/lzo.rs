//! Safe wrappers over `libavutil/lzo.h`.

use super::ffi;

/// The C error bitmask and both remaining-length out parameters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DecodeStatus {
    /// C's `AV_LZO_*` error bits; 0 when the input decoded whole.
    pub error_flags: i32,
    /// The input bytes C left unread.
    pub input_remaining: usize,
    /// The output bytes C left unwritten.
    pub output_remaining: usize,
}

fn padded(bytes: &[u8], padding: usize) -> Result<Vec<u8>, i32> {
    i32::try_from(bytes.len()).map_err(|_| -(ffi::EINVAL.cast_signed()))?;
    let len = bytes
        .len()
        .checked_add(padding)
        .ok_or(-(ffi::EINVAL.cast_signed()))?;
    let mut buffer = vec![0; len];
    buffer[..bytes.len()].copy_from_slice(bytes);
    Ok(buffer)
}

/// Wraps: `av_lzo1x_decode`
///
/// Decode `LZO1x` with C's required input/output padding hidden in private
/// initialized scratch buffers. Both lengths must fit C's `int`.
/// Return the C status even on malformed input: partial writes and remaining
/// lengths are preserved. C may modify bytes beyond the reported written
/// prefix, but no byte beyond `output` is exposed. No pointers are retained.
///
/// # Errors
///
/// Returns the negative `AVERROR` code `av_lzo1x_decode` returns when it fails.
pub fn decode(input: &[u8], output: &mut [u8]) -> Result<DecodeStatus, i32> {
    let source = padded(input, ffi::AV_LZO_INPUT_PADDING as usize)?;
    let mut destination = padded(output, ffi::AV_LZO_OUTPUT_PADDING as usize)?;
    // `padded` refused lengths beyond `int`.
    let mut in_left = i32::try_from(input.len()).map_err(|_| -(ffi::EINVAL.cast_signed()))?;
    let mut out_left = i32::try_from(output.len()).map_err(|_| -(ffi::EINVAL.cast_signed()))?;
    // SAFETY: the independent initialized allocations include C's required
    // readable 8-byte and writable 12-byte padding, even for empty buffers.
    // Both logical lengths fit int. C borrows the buffers and exclusive
    // scalar slots only for this call and never stores or frees a pointer.
    let flags = unsafe {
        ffi::av_lzo1x_decode(
            destination.as_mut_ptr().cast(),
            &raw mut out_left,
            source.as_ptr().cast(),
            &raw mut in_left,
        )
    };
    output.copy_from_slice(&destination[..output.len()]);
    Ok(DecodeStatus {
        error_flags: flags,
        input_remaining: in_left.unsigned_abs() as usize,
        output_remaining: out_left.unsigned_abs() as usize,
    })
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn rust_padding_is_initialized_and_independent() {
        let input = [1, 2, 3];
        let mut copy = padded(&input, 8).unwrap();
        assert_eq!(&copy[..3], &input);
        assert_eq!(&copy[3..], &[0; 8]);
        copy[0] = 9;
        assert_eq!(input[0], 1);
        assert_eq!(padded(&[], 12).unwrap(), vec![0; 12]);
        assert!(padded(&input, usize::MAX).is_err());
    }

    #[test]
    fn lengths_beyond_int_are_refused_before_copying() {
        // A zeroed allocation: refusing it must not touch its pages.
        let long = vec![0u8; i32::MAX as usize + 1];
        let einval = -(ffi::EINVAL.cast_signed());
        assert_eq!(padded(&long, 8), Err(einval));
        assert_eq!(decode(&long, &mut [0; 4]), Err(einval));
        let mut long = long;
        assert_eq!(decode(&[17, 0, 0], &mut long), Err(einval));
    }
}
