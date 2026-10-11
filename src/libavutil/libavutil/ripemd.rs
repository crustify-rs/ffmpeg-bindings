//! Safe opaque layout and lifecycle for `libavutil/ripemd.h`.
//!
//! C alone knows the context's size and private fields. No inline constructor,
//! zeroable marker or generic allocation policy is supplied for this layout.

use core::ptr::NonNull;

use ffibox::{CBox, CDrop, define_ctype};

use super::{ffi, mem::Heap};

define_ctype!(
    /// Wraps: AVRIPEMD
    ///
    /// An opaque, initialized C hashing context, held behind an [`RipemdBox`].
    Ripemd, RipemdRef, RipemdMut, ffi::AVRIPEMD
);

/// Sole ownership of a complete C-allocated hashing context.
/// No thread-safety assertion is made for the wrapped C type:
///
/// ```compile_fail,E0277
/// use ffmpeg::libavutil::ripemd::RipemdBox;
/// fn requires_send<T: Send>() {}
/// requires_send::<RipemdBox>();
/// ```
pub type RipemdBox = CBox<Ripemd, RipemdFree>;

/// Storage-only release through the existing av_malloc-family policy. Unlike
/// Heap, this policy offers no generic allocator for the incomplete C layout.
#[derive(Clone, Copy, Debug, Default)]
pub struct RipemdFree;

// SAFETY: RipemdBox adopts only av_ripemd_alloc storage, released by Heap's
// av_free policy; the context has no owned sub-allocations or field disposer.
unsafe impl CDrop<Ripemd> for RipemdFree {
    unsafe fn c_drop(&self, ptr: NonNull<Ripemd>) {
        // SAFETY: CDrop's caller transfers the unique av_malloc-family claim.
        unsafe { <Heap as CDrop<Ripemd>>::c_drop(&Heap, ptr) }
    }
}

/// The four digest sizes accepted by C initialization.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DigestBits {
    Bits128,
    Bits160,
    Bits256,
    Bits320,
}

impl DigestBits {
    /// Number of bits in the digest.
    pub const fn bits(self) -> i32 {
        match self {
            Self::Bits128 => 128,
            Self::Bits160 => 160,
            Self::Bits256 => 256,
            Self::Bits320 => 320,
        }
    }

    /// Number of output bytes required by this algorithm.
    pub const fn digest_len(self) -> usize {
        (self.bits() / 8) as usize
    }
}

impl TryFrom<i32> for DigestBits {
    type Error = i32;

    fn try_from(bits: i32) -> Result<Self, Self::Error> {
        match bits {
            128 => Ok(Self::Bits128),
            160 => Ok(Self::Bits160),
            256 => Ok(Self::Bits256),
            320 => Ok(Self::Bits320),
            _ => Err(-22),
        }
    }
}

impl Ripemd {
    /// Wraps: av_ripemd_alloc
    ///
    /// Allocate through C and initialize for the chosen digest size before
    /// publishing the owner. None means C allocation failed.
    pub fn new(bits: DigestBits) -> Option<RipemdBox> {
        // SAFETY: C's allocator has no arguments or preconditions and returns
        // its actual private context size, zero-filled, or NULL.
        let ptr = NonNull::new(unsafe { ffi::av_ripemd_alloc() })?;
        // SAFETY: ptr uniquely owns the full C allocation. Initialization
        // writes the active state and installs a static transform function;
        // the enum admits only supported sizes, for which C returns zero.
        let result = unsafe { ffi::av_ripemd_init(ptr.as_ptr(), bits.bits()) };
        // No Rust operation can unwind between allocation and initialization.
        // Account for a mismatched foreign implementation without leaking.
        if result < 0 {
            // SAFETY: the unpublished av_malloc-family block is owned here;
            // av_free reads none of its possibly incomplete contents.
            unsafe { ffi::av_free(ptr.as_ptr().cast()) };
            return None;
        }
        // SAFETY: C has now fully initialized the context for every safe
        // operation. Ownership is unique and RipemdFree pairs it with av_free.
        unsafe { RipemdBox::from_c(ptr.as_ptr()) }
    }
}

impl RipemdMut<'_> {
    /// Wraps: av_ripemd_init
    ///
    /// Reset the context for a supported digest size. C retains no pointer
    /// to caller-owned data. Returns C's status (zero for these four sizes).
    pub fn init(&mut self, bits: DigestBits) -> Result<(), i32> {
        // SAFETY: this handle exclusively borrows a full initialized C
        // context; bits admits only sizes C accepts. C retains no pointer.
        let result = unsafe { ffi::av_ripemd_init(self.as_mut_ptr(), bits.bits()) };
        if result < 0 { Err(result) } else { Ok(()) }
    }
}

/// Wraps: av_ripemd_size
///
/// The C context's size in bytes, borrowed from immutable C static storage.
pub fn context_size() -> &'static i32 {
    // SAFETY: C defines this global as `const int = sizeof(context)`; it has
    // static lifetime and no writer anywhere in the library.
    unsafe { &ffi::av_ripemd_size }
}

impl RipemdMut<'_> {
    /// Wraps: av_ripemd_update
    ///
    /// Append input bytes. C copies them and retains no input pointer.
    pub fn update(&mut self, data: &[u8]) {
        // SAFETY: the handle exclusively borrows an initialized full C context;
        // nonempty data provides len readable bytes; empty input uses a real static
        // byte for C's zero-length memcpy. C retains no pointer.
        unsafe {
            ffi::av_ripemd_update(
                self.as_mut_ptr(),
                if data.is_empty() {
                    c"".as_ptr().cast()
                } else {
                    data.as_ptr()
                },
                data.len(),
            )
        }
    }

    /// Wraps: av_ripemd_final
    ///
    /// Finish the current message. The digest occupies the prefix of the returned
    /// array; bytes beyond the selected algorithm's digest length are zero.
    /// The maximum-size output remains safe even after changing the algorithm.
    /// Reinitialize the context before starting another message.
    pub fn finalize(&mut self) -> [u8; 40] {
        let mut digest = [0; 40];
        // SAFETY: exclusive initialized context; the output covers the largest
        // supported digest. C writes only the active digest and retains no pointer.
        unsafe { ffi::av_ripemd_final(self.as_mut_ptr(), digest.as_mut_ptr()) };
        digest
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn digest_sizes_are_exhaustive_and_ffi_free() {
        for (bits, len) in [(128, 16), (160, 20), (256, 32), (320, 40)] {
            let size = DigestBits::try_from(bits).unwrap();
            assert_eq!(size.bits(), bits);
            assert_eq!(size.digest_len(), len);
        }
        for bits in [i32::MIN, -1, 0, 127, 129, 512, i32::MAX] {
            assert_eq!(DigestBits::try_from(bits), Err(-22));
        }
    }

    #[test]
    fn raw_adoption_round_trip_keeps_the_full_c_allocation() {
        let owner = Ripemd::new(DigestBits::Bits320).unwrap();
        let ptr = owner.into_c();
        // SAFETY: into_c transfers the initialized unique allocation, and
        // RipemdFree is its matching storage-only av_free release policy.
        let mut owner = unsafe { RipemdBox::from_c(ptr) }.unwrap();
        owner.as_mut().init(DigestBits::Bits128).unwrap();
    }
}
