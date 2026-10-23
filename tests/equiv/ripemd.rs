use ffmpeg_bindings::libavutil::libavutil::{
    ffi,
    ripemd::{self, DigestBits, Ripemd, RipemdBox},
};

/// The single raw C-reference constructor fixture for this type.
fn reference(bits: DigestBits) -> RipemdBox {
    // SAFETY: C returns its complete fresh allocation or NULL.
    let ptr = unsafe { ffi::av_ripemd_alloc() };
    assert!(!ptr.is_null());
    // SAFETY: ptr uniquely owns full C storage, and bits is supported.
    let status = unsafe { ffi::av_ripemd_init(ptr, bits.bits()) };
    assert_eq!(status, 0);
    // SAFETY: initialization completed; RipemdBox adopts the unique block
    // under the same av_free policy required by the raw C allocator.
    unsafe { RipemdBox::from_c(ptr) }.unwrap()
}

/// Snapshot-finalize without mutating the ongoing context, to compare after
/// each update, including buffered partial blocks. This is a C-only probe.
fn digest(context: &RipemdBox, bits: DigestBits) -> Vec<u8> {
    let mut output = vec![0; bits.digest_len()];
    // SAFETY: av_ripemd_size is C's immutable actual context size; the
    // allocation is initialized, resource-free, and contains only static
    // transform code pointers, so a byte copy is an independent context.
    unsafe {
        let len = ffi::av_ripemd_size as usize;
        let copy = ffi::av_memdup(context.as_c_ptr().cast(), len).cast::<ffi::AVRIPEMD>();
        assert!(!copy.is_null());
        ffi::av_ripemd_final(copy, output.as_mut_ptr());
        ffi::av_free(copy.cast());
    }
    output
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn construction_reset_and_each_hash_checkpoint_match_c() {
    let sizes = [
        DigestBits::Bits128,
        DigestBits::Bits160,
        DigestBits::Bits256,
        DigestBits::Bits320,
    ];
    for initial in sizes {
        let mut wrapped = Ripemd::new(initial).unwrap();
        let raw = reference(initial);
        for bits in sizes {
            assert_eq!(wrapped.as_mut().init(bits), Ok(()));
            // SAFETY: raw owns a full context exclusively; bits is supported.
            let status = unsafe { ffi::av_ripemd_init(raw.as_c_ptr(), bits.bits()) };
            assert_eq!(status, 0);
            assert_eq!(
                &wrapped.as_mut().finalize()[..bits.digest_len()],
                digest(&raw, bits)
            );
            wrapped.as_mut().init(bits).unwrap();
            let mut accumulated = Vec::new();
            for len in [0, 1, 54, 1, 7, 1, 64, 129, 1000] {
                let input: Vec<_> = (0..len).map(|i| (i * 17) as u8).collect();
                // A live byte is used even when len is zero: C invokes memcpy.
                let empty = 0u8;
                let ptr = if input.is_empty() {
                    &raw const empty
                } else {
                    input.as_ptr()
                };
                // SAFETY: both contexts are full initialized independently
                // owned C allocations; data is readable for len bytes (and
                // non-null even for zero); C retains no data pointer.
                unsafe {
                    ffi::av_ripemd_update(raw.as_c_ptr(), ptr, len);
                }
                wrapped.as_mut().update(&input);
                accumulated.extend_from_slice(&input);
                assert_eq!(
                    &wrapped.as_mut().finalize()[..bits.digest_len()],
                    digest(&raw, bits)
                );
                // Finalization mutates the hash; rebuild its prefix through only
                // the safe API before extending it at the next checkpoint.
                wrapped.as_mut().init(bits).unwrap();
                wrapped.as_mut().update(&accumulated);
            }
        }
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn rejected_digest_sizes_match_c_errors_without_publishing_invalid_state() {
    for bits in [i32::MIN, -1, 0, 127, 129, 512, i32::MAX] {
        // Give up the valid owner before calling C with unsupported bits:
        // C writes digest_len on failure, so it must not remain a safe owner.
        let ptr = reference(DigestBits::Bits160).into_c();
        // SAFETY: ptr uniquely owns a full C allocation, which init accepts
        // for any bits value. A failure leaves an unpublished raw context.
        let status = unsafe { ffi::av_ripemd_init(ptr, bits) };
        // SAFETY: into_c transferred this av_malloc-family claim; av_free
        // releases it once without reading the possibly invalid hash state.
        unsafe { ffi::av_free(ptr.cast()) };
        assert_eq!(DigestBits::try_from(bits), Err(status));
    }
}

#[test]
#[cfg_attr(miri, ignore = "reads a C global")]
fn batch10_size_is_the_c_global() {
    for _ in 0..4 {
        // SAFETY: this global is an immutable const int with static lifetime.
        let raw = unsafe { &ffi::av_ripemd_size };
        assert_eq!(ripemd::context_size(), raw);
        assert!(std::ptr::eq(ripemd::context_size(), raw));
    }
}
