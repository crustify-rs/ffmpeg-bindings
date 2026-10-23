//! Compare independent contexts through every multi-call state transition.
#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "test inputs are chosen within range, and the casts mirror the C conversions under comparison"
)]
use ffmpeg_bindings::libavutil::libavutil::ffi;
use ffmpeg_bindings::libavutil::libavutil::md5::Md5;

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn streaming_reset_and_post_final_match_c() {
    let mut wrapped = Md5::new().unwrap();
    let reference = Md5::new().unwrap();
    let data: Vec<_> = (0..258).map(|i| (i * 73) as u8).collect();
    for len in 0..=256 {
        let input = &data[1..=len]; // unaligned, and non-null even when empty
        for step in [1, 7, 16, 55, 64, 65, 257] {
            wrapped.as_mut().reset();
            // SAFETY: reference solely owns the full C context, no handles live.
            unsafe { ffi::av_md5_init(reference.as_c_ptr()) };
            for chunk in input.chunks(step) {
                wrapped.as_mut().update(chunk);
                wrapped.as_mut().update(&[]);
                // For the unsafe operations below: exclusive C context, live input of chunk.len()
                // bytes, and a non-null empty input. C retains neither input
                // pointer.
                {
                    // SAFETY: as the comment above this block states.
                    unsafe {
                        ffi::av_md5_update(reference.as_c_ptr(), chunk.as_ptr(), chunk.len());
                    };
                    // SAFETY: as the comment above this block states.
                    unsafe { ffi::av_md5_update(reference.as_c_ptr(), data.as_ptr(), 0) };
                }
            }
            let mut expected = [0; 16];
            // SAFETY: initialized exclusively owned context, 16 writable bytes.
            unsafe { ffi::av_md5_final(reference.as_c_ptr(), expected.as_mut_ptr()) };
            assert_eq!(
                wrapped.as_mut().finalize(),
                expected,
                "len={len}, step={step}"
            );
            let mut one_shot = [0; 16];
            // SAFETY: input is readable for len bytes and output has 16 writable
            // bytes. The buffers do not overlap, and C retains no pointer.
            unsafe { ffi::av_md5_sum(one_shot.as_mut_ptr(), input.as_ptr(), input.len()) };
            assert_eq!(expected, one_shot);
            // Final changes MD5's state; compare repeated final and a subsequent
            // update rather than silently imposing Rust-only reset semantics.
            for suffix in [&b""[..], &b"post-final"[..]] {
                wrapped.as_mut().update(suffix);
                // For the unsafe operations below: independent live context, borrowed readable
                // suffix, and 16 writable output bytes. Neither buffer is retained.
                {
                    let src = if suffix.is_empty() {
                        data.as_ptr()
                    } else {
                        suffix.as_ptr()
                    };
                    // SAFETY: as the comment above this block states.
                    unsafe { ffi::av_md5_update(reference.as_c_ptr(), src, suffix.len()) };
                    // SAFETY: as the comment above this block states.
                    unsafe { ffi::av_md5_final(reference.as_c_ptr(), expected.as_mut_ptr()) };
                }
                assert_eq!(wrapped.as_mut().finalize(), expected);
            }
        }
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn interleaved_update_and_final_match_after_each_step() {
    let mut wrapped = Md5::new().unwrap();
    let reference = Md5::new().unwrap();
    for chunk in [&b"one"[..], &b""[..], &b"two"[..], &[0xFF; 80][..]] {
        wrapped.as_mut().update(chunk);
        let mut expected = [0; 16];
        // For the unsafe operations below: reference solely owns initialized C storage, chunk
        // supplies readable bytes, and the output is exactly 16 writable bytes. None of
        // these pointers is retained and no Rust handles cover the reference.
        {
            let src = if chunk.is_empty() {
                b"live allocation".as_ptr()
            } else {
                chunk.as_ptr()
            };
            // SAFETY: as the comment above this block states.
            unsafe { ffi::av_md5_update(reference.as_c_ptr(), src, chunk.len()) };
            // SAFETY: as the comment above this block states.
            unsafe { ffi::av_md5_final(reference.as_c_ptr(), expected.as_mut_ptr()) };
        }
        assert_eq!(wrapped.as_mut().finalize(), expected);
    }
}

use ffmpeg_bindings::libavutil::libavutil::md5;

#[test]
#[cfg_attr(miri, ignore = "reads a C global")]
fn batch10_size_is_the_c_global() {
    for _ in 0..4 {
        // SAFETY: this global is an immutable const int with static lifetime.
        let raw = unsafe { &ffi::av_md5_size };
        assert_eq!(md5::context_size(), raw);
        assert!(std::ptr::eq(md5::context_size(), raw));
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn batch10_digest_sequences_match_c() {
    let data: Vec<_> = (0..1025).map(|i| (i * 97) as u8).collect();
    for len in [0, 1, 55, 56, 63, 64, 65, 119, 120, 127, 128, 1024] {
        let mut raw = [0xAA; 16];
        let input = data[1..=len].to_vec();
        let empty = 0u8;
        let ptr = if input.is_empty() {
            &raw const empty
        } else {
            input.as_ptr()
        };
        // SAFETY: source is readable for its length; destination is exclusive
        // 16-byte storage and cannot overlap the separately owned source.
        unsafe { ffi::av_md5_sum(raw.as_mut_ptr(), ptr, input.len()) };
        assert_eq!(md5::sum(&data[1..=len]), raw);
    }
}
