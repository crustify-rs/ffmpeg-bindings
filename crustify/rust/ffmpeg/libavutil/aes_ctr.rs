//! Safe wrappers over `libavutil/aes_ctr.h`.
//!
//! The private context contains an inline AES context, counters and a block offset;
//! it owns no pointers. Initialization copies the key and resets the counter and
//! offset. Only C knows its size and alignment. IV and processing operations belong
//! to the subsequent symbol batch.

use super::{aes::InitError, ffi};
use core::{mem::MaybeUninit, ptr::NonNull};
use ffibox::{CAlloc, CBox, CDrop, define_ctype, impl_cdrop};

define_ctype!(
    /// Wraps: AVAESCTR
    ///
    /// An initialized AES-128 CTR context, opaque in the public C API. An adopted
    /// context must have been successfully initialized by `av_aes_ctr_init`.
    /// Use [`AesCtr::new`] rather than an inline value or a generic allocator.
    AesCtr, AesCtrRef, AesCtrMut, ffi::AVAESCTR
);

/// Wraps: av_aes_ctr_free
///
/// Release a context's full C allocation, including its inline AES state.
#[derive(Clone, Copy, Debug, Default)]
pub struct AesCtrFree;

// SAFETY: av_aes_ctr_free releases the av_malloc-family allocation once. The
// context has no separate owned pointers or resources, and C retains no alias.
impl_cdrop!(AesCtrFree, AesCtr, ffi::av_aes_ctr_free);

// SAFETY: av_aes_ctr_free calls av_free without reading construction-phase bytes.
unsafe impl CDrop<MaybeUninit<AesCtr>> for AesCtrFree {
    unsafe fn c_drop(&self, ptr: NonNull<MaybeUninit<AesCtr>>) {
        // SAFETY: the caller transfers the live C allocation for one release;
        // free neither reads the context nor requires successful initialization.
        unsafe { ffi::av_aes_ctr_free(ptr.as_ptr().cast()) }
    }
}

/// Wraps: av_aes_ctr_alloc
// SAFETY: C allocates its full private body with the right size and alignment;
// AesCtrFree pairs with the allocator for both construction and initialized storage.
unsafe impl CAlloc<AesCtr> for AesCtrFree {
    fn c_alloc(&self) -> Option<NonNull<MaybeUninit<AesCtr>>> {
        // SAFETY: the allocator takes no arguments and transfers fresh storage.
        NonNull::new(unsafe { ffi::av_aes_ctr_alloc() }.cast())
    }
}

/// Sole ownership of a CTR context, with shared and exclusive borrowed handles.
///
/// An exclusive handle cannot outlive its owner:
/// ```compile_fail,E0505
/// use ffmpeg::libavutil::aes_ctr::AesCtr;
/// let mut ctr = AesCtr::new(&[0; 16]).unwrap();
/// let mut handle = ctr.as_mut();
/// drop(ctr);
/// handle.init(&[1; 16]).unwrap();
/// ```
/// Reborrows cannot alias a mutation:
/// ```compile_fail,E0499
/// use ffmpeg::libavutil::aes_ctr::AesCtr;
/// let mut ctr = AesCtr::new(&[0; 16]).unwrap();
/// let mut first = ctr.as_mut();
/// let mut second = ctr.as_mut();
/// first.init(&[1; 16]).unwrap();
/// second.init(&[2; 16]).unwrap();
/// ```
pub type AesCtrBox = CBox<AesCtr, AesCtrFree>;

impl AesCtr {
    /// Allocate and initialize AES-CTR with a copied 16-byte key and a zero IV.
    /// Set an appropriate IV through the processing API before encrypting messages;
    /// never reuse a key/IV combination.
    pub fn new(key: &[u8; 16]) -> Result<AesCtrBox, InitError> {
        let mut storage = AesCtrBox::new_uninit().ok_or(InitError::AllocationFailed)?;
        // SAFETY: storage holds the full, exclusively owned C allocation; the
        // fixed-size key provides the 16 bytes C copies without retaining a pointer.
        let code = unsafe { ffi::av_aes_ctr_init(storage.as_c_ptr(), key.as_ptr()) };
        if code < 0 {
            return Err(InitError::InitializationFailed(code));
        }
        // SAFETY: C initialized the inline AES schedule and reset counter/offset.
        // The encrypted-counter scratch buffer is only read after C fills it.
        Ok(unsafe { storage.assume_init() })
    }
}

impl AesCtrMut<'_> {
    /// Wraps: av_aes_ctr_init
    ///
    /// Replace the copied key and reset the full counter and partial-block state.
    pub fn init(&mut self, key: &[u8; 16]) -> Result<(), InitError> {
        // SAFETY: the handle exclusively borrows an initialized full C context;
        // C copies exactly 16 key bytes and does not retain the caller pointer.
        let code = unsafe { ffi::av_aes_ctr_init(self.as_mut_ptr(), key.as_ptr()) };
        if code < 0 {
            Err(InitError::InitializationFailed(code))
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn null_adoption_and_opaque_layout() {
        assert_eq!(
            core::mem::size_of::<AesCtr>(),
            core::mem::size_of::<ffi::AVAESCTR>()
        );
        assert_eq!(
            core::mem::align_of::<AesCtr>(),
            core::mem::align_of::<ffi::AVAESCTR>()
        );
        // SAFETY: a null pointer transfers no allocation and creates no owner.
        assert!(unsafe { AesCtrBox::from_c(core::ptr::null_mut()) }.is_none());
    }

    #[test]
    #[cfg(not(miri))]
    fn raw_roundtrip_and_construction_phase_drop() {
        let owner = AesCtr::new(&[0; 16]).unwrap();
        let ptr = owner.into_c();
        // SAFETY: into_c transferred this initialized allocation; adopt it once.
        let owner = unsafe { AesCtrBox::from_c(ptr) }.unwrap();
        assert_eq!(owner.as_c_ptr(), ptr);
        drop(owner);
        drop(AesCtrBox::new_uninit().unwrap());
    }
}
