//! Safe opaque layout and lifecycle for `libavutil/camellia.h`.
//!
//! C owns the concrete layout and allocation. Contexts own no sub-allocations
//! and retain no key pointer. There is no public C clone operation.
//!
//! Initialization rewrites a shared C lookup table. All safe initialization
//! calls are serialized; future crypt wrappers must use the same internal
//! lock. Raw C calls must exclude concurrent initialization/crypt accesses.

use super::{ffi, mem::HeapBox};
use ffibox::define_ctype;
use std::sync::{Mutex, MutexGuard};

define_ctype!(
    /// Wraps: AVCAMELLIA
    ///
    /// Opaque C context, constructed only behind a pointer as [`CamelliaBox`].
    /// The bindgen layout is opaque: never allocate inline storage for C to use.
    Camellia, CamelliaRef, CamelliaMut, ffi::AVCAMELLIA
);

/// A sole context allocated by `av_camellia_alloc` and freed by `av_free`.
/// Not `Clone`: its layout is private and C publishes no copy routine.
pub type CamelliaBox = HeapBox<Camellia>;

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
        .unwrap_or_else(|poisoned| poisoned.into_inner())
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
    /// Wraps: av_camellia_alloc
    ///
    /// Allocate and initialize a context. C copies the schedule, not the key
    /// pointer. No construction-phase storage is exposed to the caller.
    pub fn new(key: &[u8]) -> Result<CamelliaBox, InitError> {
        key_bits(key)?;
        // SAFETY: C returns a fresh av_mallocz allocation of the concrete
        // context's full size, or NULL. All its scalar fields are initialized,
        // it owns no other resources, and Heap releases it with av_free.
        let mut context = unsafe { CamelliaBox::from_c(ffi::av_camellia_alloc()) }
            .ok_or(InitError::AllocationFailed)?;
        context.as_mut().init(key)?;
        Ok(context)
    }
}

impl CamelliaMut<'_> {
    /// Wraps: av_camellia_init
    ///
    /// Replace the schedule. An invalid key leaves the context unchanged.
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

/// Wraps: av_camellia_size
///
/// The C context's size in bytes, borrowed from immutable C static storage.
pub fn context_size() -> &'static i32 {
    // SAFETY: C defines this global as `const int = sizeof(context)`; it has
    // static lifetime and no writer anywhere in the library.
    unsafe { &ffi::av_camellia_size }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

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
    fn raw_owner_round_trip() {
        let context = Camellia::new(&[0; 16]).unwrap();
        let ptr = context.into_c();
        // SAFETY: ptr came from this owner's into_c; no intervening access or
        // free occurred, and its unique ownership is transferred back once.
        let mut context = unsafe { CamelliaBox::from_c(ptr) }.unwrap();
        context.as_mut().init(&[1; 32]).unwrap();
    }
}
