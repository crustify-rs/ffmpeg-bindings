//! Compare independent C contexts, seeds, chunking, reset and finalization.
use ffmpeg::libavutil::{ffi, murmur3::Murmur3};

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn streaming_seeds_reset_and_post_final_match_c() {
    let mut wrapped = Murmur3::new().unwrap();
    let reference = Murmur3::new().unwrap();
    let data: Vec<_> = (0..130).map(|i| (i * 73) as u8).collect();
    for seed in [0, 1, u64::MAX, 0x725acc55daddca55] {
        let mut seeded = Murmur3::with_seed(seed).unwrap();
        // SAFETY: exclusively owned full reference context, any seed accepted.
        unsafe { ffi::av_murmur3_init_seeded(reference.as_c_ptr(), seed) };
        let mut expected = [0; 16];
        // SAFETY: live initialized reference and 16 writable output bytes.
        unsafe { ffi::av_murmur3_final(reference.as_c_ptr(), expected.as_mut_ptr()) };
        assert_eq!(seeded.as_mut().finalize(), expected);
        for len in 0..=128 {
            let input = &data[1..1 + len];
            for step in [1, 3, 8, 15, 16, 17, 129] {
                wrapped.as_mut().reset_seeded(seed);
                // SAFETY: exclusive live reference context of C's full size.
                unsafe { ffi::av_murmur3_init_seeded(reference.as_c_ptr(), seed) };
                for chunk in input.chunks(step) {
                    wrapped.as_mut().update(chunk);
                    wrapped.as_mut().update(&[]);
                    // SAFETY: exclusive initialized reference, chunk.len()
                    // readable input bytes; empty update reads nothing.
                    unsafe {
                        ffi::av_murmur3_update(reference.as_c_ptr(), chunk.as_ptr(), chunk.len());
                        ffi::av_murmur3_update(reference.as_c_ptr(), b"".as_ptr(), 0);
                    }
                }
                // Compare final state transitions at each step, then update
                // after final: C's padded partial block remains valid storage.
                for suffix in [&b""[..], &b""[..], &b"post-final"[..]] {
                    wrapped.as_mut().update(suffix);
                    // SAFETY: live exclusive context, borrowed suffix, and
                    // exactly 16 writable output bytes, never retained by C.
                    unsafe {
                        ffi::av_murmur3_update(reference.as_c_ptr(), suffix.as_ptr(), suffix.len());
                        ffi::av_murmur3_final(reference.as_c_ptr(), expected.as_mut_ptr());
                    }
                    assert_eq!(
                        wrapped.as_mut().finalize(),
                        expected,
                        "seed={seed}, len={len}, step={step}"
                    );
                }
            }
        }
    }
    wrapped.as_mut().reset();
    let mut expected = [0; 16];
    // SAFETY: exclusively owned full reference context; init accepts any
    // previous state and final writes exactly 16 bytes without retaining them.
    unsafe {
        ffi::av_murmur3_init(reference.as_c_ptr());
        ffi::av_murmur3_final(reference.as_c_ptr(), expected.as_mut_ptr());
    }
    assert_eq!(wrapped.as_mut().finalize(), expected);
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn interleaved_update_and_final_match_after_each_step() {
    let mut wrapped = Murmur3::new().unwrap();
    let reference = Murmur3::new().unwrap();
    for chunk in [&b"one"[..], &b""[..], &b"two"[..], &[0xff; 80][..]] {
        wrapped.as_mut().update(chunk);
        let mut expected = [0; 16];
        // SAFETY: reference solely owns initialized C storage, chunk supplies
        // readable bytes, and the output is exactly 16 writable bytes. None of
        // these pointers is retained and no Rust handles cover the reference.
        unsafe {
            ffi::av_murmur3_update(reference.as_c_ptr(), chunk.as_ptr(), chunk.len());
            ffi::av_murmur3_final(reference.as_c_ptr(), expected.as_mut_ptr());
        }
        assert_eq!(wrapped.as_mut().finalize(), expected);
    }
}
