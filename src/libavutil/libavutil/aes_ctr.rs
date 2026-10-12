//! Safe wrappers over `libavutil/aes_ctr.h`.
//!
//! The private context contains an inline AES context, counters and a block offset;
//! it owns no pointers. Initialization copies the key and resets the counter and
//! offset. Only C knows its size and alignment. IV getters return snapshots;
//! processing and setters borrow caller buffers only for the synchronous call.

use super::aes::{CryptError, buffer_count};
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

impl AesCtrRef<'_> {
    /// Wraps: av_aes_ctr_get_iv
    ///
    /// Snapshot the full 16-byte counter, including its current block counter.
    /// A partial crypt call may already have advanced the counter for its scratch block.
    pub fn iv(&self) -> [u8; 16] {
        // SAFETY: get_iv only returns the inline initialized counter without
        // writing the context despite its mutable C parameter. The shared handle
        // excludes mutation; copy all 16 bytes before the context can be freed.
        unsafe {
            ffi::av_aes_ctr_get_iv(self.as_ptr().cast_mut())
                .cast::<[u8; 16]>()
                .read()
        }
    }
}

impl AesCtrMut<'_> {
    /// Wraps: av_aes_ctr_set_iv
    ///
    /// Copy the 8-byte nonce, zero the block counter and discard partial-block state.
    /// Never reuse a key/nonce pair for different messages.
    pub fn set_iv(&mut self, iv: &[u8; 8]) {
        // SAFETY: exclusive initialized context, exactly 8 readable bytes;
        // C copies the nonce and retains no caller pointer.
        unsafe { ffi::av_aes_ctr_set_iv(self.as_mut_ptr(), iv.as_ptr()) };
    }

    /// Wraps: av_aes_ctr_set_full_iv
    ///
    /// Copy the full counter and discard partial-block state.
    pub fn set_full_iv(&mut self, iv: &[u8; 16]) {
        // SAFETY: exclusive initialized context, exactly 16 readable bytes;
        // C copies the full counter and retains no caller pointer.
        unsafe { ffi::av_aes_ctr_set_full_iv(self.as_mut_ptr(), iv.as_ptr()) };
    }

    /// Wraps: av_aes_ctr_increment_iv
    ///
    /// Increment the big-endian nonce with wrapping arithmetic, zero the block
    /// counter, and discard partial-block state. Avoid nonce reuse on wraparound.
    pub fn increment_iv(&mut self) {
        // SAFETY: the exclusive handle addresses the initialized full context;
        // the routine only updates its inline counter and offset.
        unsafe { ffi::av_aes_ctr_increment_iv(self.as_mut_ptr()) };
    }

    /// Wraps: av_aes_ctr_set_random_iv
    ///
    /// Ask C to choose an 8-byte nonce; reset block counter and partial state.
    /// C's random-seed API does not guarantee cryptographic uniqueness or entropy.
    pub fn set_random_iv(&mut self) {
        let _guard = super::random_seed::lock_seed_access();
        // SAFETY: the exclusive handle addresses the initialized full context;
        // C stores only copied nonce bytes. The shared seed lock serializes
        // its unsynchronized static timing-jitter fallback with other safe users.
        unsafe { ffi::av_aes_ctr_set_random_iv(self.as_mut_ptr()) };
    }

    /// Wraps: av_aes_ctr_crypt
    ///
    /// XOR any byte count with the CTR stream, preserving partial-block state
    /// across calls. Buffers must have equal lengths fitting C's int range.
    /// Encryption and decryption are identical; CTR supplies no authentication.
    pub fn crypt(&mut self, dst: &mut [u8], src: &[u8]) -> Result<(), CryptError> {
        let count = buffer_count(dst.len(), src.len(), 1)?;
        if count == 0 {
            return Ok(());
        }
        // SAFETY: the initialized full context is exclusive; equal-length,
        // disjoint slices hold count bytes, within C's int range. No pointers
        // are retained; unaligned user buffers are supported by the C routine.
        unsafe { ffi::av_aes_ctr_crypt(self.as_mut_ptr(), dst.as_mut_ptr(), src.as_ptr(), count) };
        Ok(())
    }

    /// In-place form of [`Self::crypt`], preserving the same stream state.
    pub fn crypt_in_place(&mut self, data: &mut [u8]) -> Result<(), CryptError> {
        let count = buffer_count(data.len(), data.len(), 1)?;
        if count == 0 {
            return Ok(());
        }
        let ptr = data.as_mut_ptr();
        // SAFETY: exclusive initialized context and a readable/writable slice
        // of count bytes. C supports src == dst and keeps no user pointers.
        unsafe { ffi::av_aes_ctr_crypt(self.as_mut_ptr(), ptr, ptr, count) };
        Ok(())
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn rejected_lengths_preserve_stream() {
        let mut owner = AesCtr::new(&[0; 16]).unwrap();
        let mut data = [9; 17];
        owner.as_mut().crypt_in_place(&mut data[..1]).unwrap();
        let before = owner.as_ref().iv();
        assert_eq!(
            owner.as_mut().crypt(&mut data, &[0; 16]),
            Err(CryptError::LengthMismatch)
        );
        assert_eq!(owner.as_ref().iv(), before);
        assert_eq!(data[1..], [9; 16]);
        // The rejected call must preserve not just IV but the scratch offset.
        let mut reference = AesCtr::new(&[0; 16]).unwrap();
        reference.as_mut().crypt_in_place(&mut [9]).unwrap();
        let mut expected = data;
        owner.as_mut().crypt_in_place(&mut data).unwrap();
        reference.as_mut().crypt_in_place(&mut expected).unwrap();
        assert_eq!(data, expected);
    }

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
    #[cfg_attr(miri, ignore = "calls C")]
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
