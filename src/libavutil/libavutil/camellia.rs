//! Safe opaque layout and lifecycle for `libavutil/camellia.h`.
//!
//! C owns the concrete layout and allocation. Contexts own no sub-allocations
//! and retain no key pointer. There is no public C clone operation.
//!
//! Initialization rewrites a shared C lookup table. All safe initialization
//! calls are serialized; future crypt wrappers must use the same internal
//! lock. Raw C calls must exclude concurrent initialization/crypt accesses.

use core::ptr::NonNull;
use std::sync::{Mutex, MutexGuard};

use ffibox::{CBox, CDrop, define_ctype};

use super::aes::{CryptError, Direction, buffer_count};
use super::ffi;
use super::mem::Heap;

define_ctype!(
    /// Wraps: AVCAMELLIA
    ///
    /// Opaque C context, constructed only behind a pointer as [`CamelliaBox`].
    /// The bindgen layout is opaque: never allocate inline storage for C to use.
    Camellia,
    CamelliaRef,
    CamelliaMut,
    ffi::AVCAMELLIA
);

/// A sole context allocated by `av_camellia_alloc` and freed by `av_free`.
/// Not `Clone`: its layout is private and C publishes no copy routine.
///
/// The opaque layout has no Rust size, so there is no Rust-sized allocation:
///
/// ```compile_fail,E0277
/// use ffmpeg_bindings::libavutil::libavutil::camellia::CamelliaBox;
/// let _ = CamelliaBox::new_uninit();
/// ```
pub type CamelliaBox = CBox<Camellia, CamelliaFree>;

/// Storage-only release of an `av_camellia_alloc` context. Unlike [`Heap`], it
/// offers no generic allocator: the opaque layout's Rust size is zero, so
/// only C can size a Camellia context.
#[derive(Clone, Copy, Debug, Default)]
pub struct CamelliaFree;

/// Wraps: `av_free`
// SAFETY: CamelliaBox adopts only `av_camellia_alloc` storage, which owns no sub-allocations;
// Heap's av_free releases that av_malloc-family block once without reading it.
unsafe impl CDrop<Camellia> for CamelliaFree {
    unsafe fn c_drop(&self, ptr: NonNull<Camellia>) {
        // SAFETY: CDrop's caller transfers the unique av_malloc-family claim.
        unsafe { <Heap as CDrop<Camellia>>::c_drop(&Heap, ptr) }
    }
}

/// Failure to construct or initialize a context.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InitError {
    /// Camellia accepts exactly 16-, 24-, or 32-byte keys.
    InvalidKeyLength,
    /// C could not allocate a context.
    AllocationFailed,
}

// av_camellia_init calls computeSP, rewriting global SP even after its first
// use. Every safe init AND future crypt wrapper must hold this same lock,
// because crypt reads SP. Raw C callers must exclude concurrent SP access.
static C_ACCESS: Mutex<()> = Mutex::new(());

pub(crate) fn lock_c_access() -> MutexGuard<'static, ()> {
    C_ACCESS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn key_bits(key: &[u8]) -> Result<i32, InitError> {
    match key.len() {
        16 => Ok(128),
        24 => Ok(192),
        32 => Ok(256),
        _ => Err(InitError::InvalidKeyLength),
    }
}

impl Camellia {
    /// Wraps: `av_camellia_alloc`
    ///
    /// Allocate and initialize a context. C copies the schedule, not the key
    /// pointer. No construction-phase storage is exposed to the caller.
    ///
    /// # Errors
    ///
    /// Returns [`InitError::InvalidKeyLength`] for a key that is not 16, 24 or 32 bytes, and
    /// [`InitError::AllocationFailed`] when C cannot allocate the context.
    pub fn new(key: &[u8]) -> Result<CamelliaBox, InitError> {
        key_bits(key)?;
        // SAFETY: as the comment below states, for this part of the operation.
        let camellia_alloc = unsafe { ffi::av_camellia_alloc() };
        // SAFETY: C returns a fresh av_mallocz allocation of the concrete
        // context's full size, or NULL. All its scalar fields are initialized,
        // it owns no other resources, and CamelliaFree releases it with av_free.
        let mut context =
            unsafe { CamelliaBox::from_c(camellia_alloc) }.ok_or(InitError::AllocationFailed)?;
        context.as_mut().init(key)?;
        Ok(context)
    }
}

impl CamelliaMut<'_> {
    /// Wraps: `av_camellia_init`
    ///
    /// Replace the schedule. An invalid key leaves the context unchanged.
    ///
    /// # Errors
    ///
    /// Returns [`InitError::InvalidKeyLength`] for a key that is not 16, 24 or 32 bytes.
    ///
    /// # Panics
    ///
    /// Only if C rejects a key length it accepts, which the check above validated.
    pub fn init(&mut self, key: &[u8]) -> Result<(), InitError> {
        let bits = key_bits(key)?;
        let _guard = lock_c_access();
        // SAFETY: the exclusive handle addresses the full C allocation. The
        // validated bit count makes C read exactly key.len() bytes, retaining
        // no pointer. The lock excludes other safe accesses to global SP.
        let status = unsafe { ffi::av_camellia_init(self.as_mut_ptr(), key.as_ptr(), bits) };
        assert_eq!(status, 0, "C rejected a validated Camellia key length");
        Ok(())
    }
}

/// Wraps: `av_camellia_size`
///
/// The C context's size in bytes, borrowed from immutable C static storage.
#[must_use]
pub fn context_size() -> &'static i32 {
    // SAFETY: C defines this global as `const int = sizeof(context)`; it has
    // static lifetime and no writer anywhere in the library.
    unsafe { &ffi::av_camellia_size }
}

impl CamelliaRef<'_> {
    /// Wraps: `av_camellia_crypt`
    ///
    /// Encrypt or decrypt whole blocks.
    /// `None` selects ECB; `Some` selects CBC and updates the IV after each block.
    /// Buffers must have equal lengths; errors leave all state unchanged.
    ///
    /// # Errors
    ///
    /// Returns a [`CryptError`] when it rejects the buffers, leaving the context, output and IV
    /// unchanged.
    pub fn crypt(
        &self,
        dst: &mut [u8],
        src: &[u8],
        iv: Option<&mut [u8; 16]>,
        direction: Direction,
    ) -> Result<(), CryptError> {
        let count = buffer_count(dst.len(), src.len(), 16)?;
        if count == 0 {
            return Ok(());
        }
        let _guard = lock_c_access();
        // SAFETY: the shared handle addresses an initialized full context;
        // equal-length disjoint slices hold count whole blocks, and any IV is
        // a separate writable 16-byte array. C retains no buffer pointers.
        // C reads but never writes the schedule despite its mutable ABI pointer.
        // The guard also excludes every safe writer/reader of C's global SP.
        unsafe {
            ffi::av_camellia_crypt(
                self.as_ptr().cast_mut(),
                dst.as_mut_ptr(),
                src.as_ptr(),
                count,
                iv.map_or(core::ptr::null_mut(), |iv| iv.as_mut_ptr()),
                direction.as_int(),
            );
        };
        Ok(())
    }

    /// In-place form of [`Self::crypt`], with the same modes and validation.
    ///
    /// # Errors
    ///
    /// Returns a [`CryptError`] when it rejects the buffers, leaving the context, output and IV
    /// unchanged.
    pub fn crypt_in_place(
        &self,
        data: &mut [u8],
        iv: Option<&mut [u8; 16]>,
        direction: Direction,
    ) -> Result<(), CryptError> {
        let count = buffer_count(data.len(), data.len(), 16)?;
        if count == 0 {
            return Ok(());
        }
        let _guard = lock_c_access();
        let ptr = data.as_mut_ptr();
        // SAFETY: one exclusive slice provides count whole readable/writable
        // blocks. C explicitly supports src == dst, preserving ciphertext before
        // CBC decryption writes it. The initialized context and optional IV are
        // borrowed without writers and disjoint; no pointer is retained.
        // C never writes the schedule despite its mutable ABI pointer.
        // The guard excludes all safe accesses to the shared C lookup table.
        unsafe {
            ffi::av_camellia_crypt(
                self.as_ptr().cast_mut(),
                ptr,
                ptr,
                count,
                iv.map_or(core::ptr::null_mut(), |iv| iv.as_mut_ptr()),
                direction.as_int(),
            );
        };
        Ok(())
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn rejected_buffers_are_atomic() {
        let key = [0; 16];
        let context = Camellia::new(&key).unwrap();
        let mut data = [0x99; 16 * 2];
        let mut iv = [0x77; 16];
        assert_eq!(
            context
                .as_ref()
                .crypt(&mut data, &[0; 16], Some(&mut iv), Direction::Encrypt),
            Err(CryptError::LengthMismatch)
        );
        assert_eq!(
            context
                .as_ref()
                .crypt_in_place(&mut data[..16 - 1], Some(&mut iv), Direction::Decrypt),
            Err(CryptError::IncompleteBlock)
        );
        assert_eq!(data, [0x99; 16 * 2]);
        assert_eq!(iv, [0x77; 16]);
    }

    #[test]
    fn invalid_key_lengths() {
        for len in [0, 1, 15, 17, 23, 25, 31, 33] {
            assert!(matches!(
                Camellia::new(&vec![0; len]),
                Err(InitError::InvalidKeyLength)
            ));
        }
        assert_eq!(key_bits(&[0; 16]), Ok(128));
        assert_eq!(key_bits(&[0; 24]), Ok(192));
        assert_eq!(key_bits(&[0; 32]), Ok(256));
    }

    #[test]
    fn lock_recovers_after_rust_unwind() {
        let unwind = std::panic::catch_unwind(|| {
            let _guard = lock_c_access();
            panic!("exercise lock poisoning without touching C");
        });
        assert!(unwind.is_err());
        drop(lock_c_access());
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn raw_owner_round_trip() {
        let context = Camellia::new(&[0; 16]).unwrap();
        let ptr = context.into_c();
        // SAFETY: ptr came from this owner's into_c; no intervening access or
        // free occurred, and its unique ownership is transferred back once.
        let mut context = unsafe { CamelliaBox::from_c(ptr) }.unwrap();
        context.as_mut().init(&[1; 32]).unwrap();
    }
}
