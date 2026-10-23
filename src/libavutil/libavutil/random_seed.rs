//! Safe wrappers over `libavutil/random_seed.h`.
//!
//! [`get`] draws a best-effort 32-bit seed for a PRNG; [`random_bytes`] fills
//! a caller buffer with cryptographically secure random bytes, and
//! [`random_bytes_uninit`] one that is not yet initialised.

use core::mem::MaybeUninit;
use std::sync::{Mutex, MutexGuard};

// C's timing-jitter fallback mutates static i/buffer. Share this lock with
// indirect seed consumers (notably AES-CTR's random-IV routine). Raw C users
// must independently exclude concurrent fallback access.
static SEED_ACCESS: Mutex<()> = Mutex::new(());

pub(crate) fn lock_seed_access() -> MutexGuard<'static, ()> {
    SEED_ACCESS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

use super::ffi;

/// Wraps: `av_get_random_seed`
///
/// A best-effort random 32-bit seed for a faster PRNG: from the system's
/// entropy source, or, failing that, from timing jitter. It can be slow; call
/// it again for more bits.
#[must_use]
pub fn get() -> u32 {
    let _guard = lock_seed_access();
    // SAFETY: `av_get_random_seed` takes no arguments and has no
    // pointer preconditions. The lock excludes other safe users of its
    // static timing-jitter fallback state.
    unsafe { ffi::av_get_random_seed() }
}

/// Wraps: `av_random_bytes`
///
/// Fills `buf` with cryptographically secure random bytes, suitable for keys.
///
/// `Err` holds C's negative `AVERROR` code when no secure source could supply
/// them (`AVERROR_UNKNOWN` when `/dev/urandom` cannot be read in full); `buf`
/// may then be partly overwritten. An empty `buf` still consults the source,
/// as in C.
///
/// # Errors
///
/// `Err` holds C's negative `AVERROR` code when no secure source could supply them
/// (`AVERROR_UNKNOWN` when `/dev/urandom` cannot be read in full); `buf` may then be partly
/// overwritten.
pub fn random_bytes(buf: &mut [u8]) -> Result<(), i32> {
    // An empty slice's pointer is dangling, which C's `fread` may not receive
    // even for a zero length; a live byte stands in for it.
    let mut empty = 0u8;
    let dst = if buf.is_empty() {
        &raw mut empty
    } else {
        buf.as_mut_ptr()
    };
    // SAFETY: `dst` is valid for writes of `buf.len()` bytes, `buf`'s or the
    // local's; `av_random_bytes` writes at most that many there and keeps no
    // pointer to them.
    let ret = unsafe { ffi::av_random_bytes(dst, buf.len()) };
    if ret < 0 { Err(ret) } else { Ok(()) }
}

/// Wraps: `av_random_bytes`
///
/// As [`random_bytes`], for a buffer that need not be initialised: on success
/// the whole of `buf`, now initialised, as bytes.
///
/// # Errors
///
/// Returns the negative `AVERROR` code `av_random_bytes` returns when it fails.
pub fn random_bytes_uninit(buf: &mut [MaybeUninit<u8>]) -> Result<&mut [u8], i32> {
    let mut empty = 0u8;
    let dst = if buf.is_empty() {
        &raw mut empty
    } else {
        buf.as_mut_ptr().cast()
    };
    // SAFETY: as in `random_bytes`; C only writes the bytes, never reads them.
    let ret = unsafe { ffi::av_random_bytes(dst, buf.len()) };
    if ret < 0 {
        return Err(ret);
    }
    // SAFETY: on success `av_random_bytes` wrote all `buf.len()` bytes (it
    // reports a short read as an error), so every element is initialised.
    Ok(unsafe { buf.assume_init_mut() })
}

#[cfg(test)]
#[expect(
    clippy::assert_is_empty,
    reason = "assertions keep the form of the C behaviour they check"
)]
mod unit_tests {
    use super::*;

    #[test]
    fn seed_lock_recovers_after_unwind() {
        let result = std::panic::catch_unwind(|| {
            let _guard = lock_seed_access();
            panic!("poison seed access without calling C");
        });
        assert!(result.is_err());
        drop(lock_seed_access());
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn fills_every_byte() {
        // 64 random bytes are all zero with probability 2^-512.
        let mut buf = [0u8; 64];
        random_bytes(&mut buf).unwrap();
        assert!(buf.iter().any(|&b| b != 0));
        let mut again = [0u8; 64];
        random_bytes(&mut again).unwrap();
        assert_ne!(buf, again);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn uninit_buffer_reads_back_initialised() {
        let mut buf = [MaybeUninit::<u8>::uninit(); 48];
        let bytes = random_bytes_uninit(&mut buf).unwrap();
        assert_eq!(bytes.len(), 48);
        assert!(bytes.iter().any(|&b| b != 0));
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn empty_buffers_succeed() {
        random_bytes(&mut []).unwrap();
        assert!(random_bytes_uninit(&mut []).unwrap().is_empty());
    }
}
