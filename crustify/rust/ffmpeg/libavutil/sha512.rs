//! Safe lifecycle and borrowed handles for `libavutil/sha512.h`.
//!
//! The body is private to `sha512.c`: this opaque layout is used only behind
//! C-allocated pointers, never as inline storage. There are no public fields,
//! sub-allocations, retained input pointers, reference counts, or C clone routines.
//! Owners release their single allocation through [`Heap`]. No `Send` or `Sync`
//! implementation is asserted. Hash update/final operations are left to the
//! symbol route; initialization is bound here to the object's lifecycle.

use core::fmt;
use core::ptr::NonNull;

use ffibox::{CNew, define_ctype};

use super::ffi;
use super::mem::{Heap, HeapBox};

/// A digest variant supported by this context's initializer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum Algorithm {
    /// SHA-512/224 (224-bit digest).
    Sha512224 = 224,
    /// SHA-512/256 (256-bit digest).
    Sha512256 = 256,
    /// SHA-384 (384-bit digest).
    Sha384 = 384,
    /// SHA-512 (512-bit digest).
    Sha512 = 512,
}

impl Algorithm {
    /// The digest length in bytes.
    #[must_use]
    pub const fn digest_len(self) -> usize {
        self as usize / 8
    }
}

define_ctype!(
    /// Wraps: AVSHA512
    ///
    /// An opaque, initialized C hash context. Construct through [`Self::new`]
    /// or [`Sha512Box::new`], not by allocating the opaque layout's Rust size.
    Sha512,
    Sha512Ref,
    Sha512Mut,
    ffi::AVSHA512
);

/// A uniquely owned, initialized context, freed by `av_free`.
///
/// The zero-argument `new()` selects SHA-512; use the layout type's
/// `new(algorithm)` to select a different variant.
///
/// Borrows cannot outlive their owner:
///
/// ```compile_fail
/// use ffmpeg::libavutil::sha512::{Algorithm, Sha512};
/// let view = {
///     let owner = Sha512::new(Algorithm::Sha512).unwrap();
///     owner.as_ref()
/// };
/// let _ = format!("{view:?}");
/// ```
///
/// A shared reborrow freezes the exclusive handle:
///
/// ```compile_fail
/// use ffmpeg::libavutil::sha512::{Algorithm, Sha512};
/// let mut owner = Sha512::new(Algorithm::Sha512).unwrap();
/// let mut exclusive = owner.as_mut();
/// let shared = exclusive.as_ref();
/// exclusive.init(Algorithm::Sha512);
/// let _ = format!("{shared:?}");
/// ```
pub type Sha512Box = HeapBox<Sha512>;

impl Sha512 {
    /// Wraps: av_sha512_alloc
    ///
    /// Allocate and initialize a context for `algorithm`; `None` on allocation
    /// failure. Invalid digest sizes cannot be represented by [`Algorithm`].
    #[must_use]
    pub fn new(algorithm: Algorithm) -> Option<Sha512Box> {
        // SAFETY: the allocator has no arguments or preconditions and returns
        // a fresh, zero-filled, full-sized context, or NULL.
        let ptr = unsafe { ffi::av_sha512_alloc() };
        if ptr.is_null() {
            return None;
        }
        // SAFETY: `ptr` uniquely owns full-sized C storage. Every enum value
        // selects a supported initializer; C stores no borrowed pointers.
        let status = unsafe { ffi::av_sha512_init(ptr, algorithm as i32) };
        if status != 0 {
            // SAFETY: this is the fresh av_mallocz allocation, not yet adopted
            // or borrowed; av_free never reads the object.
            unsafe { ffi::av_free(ptr.cast()) };
            return None;
        }
        // SAFETY: C initialized the context in its own full-sized allocation;
        // ownership is unique and Heap releases it through av_free once.
        unsafe { Sha512Box::from_c(ptr) }
    }
}

impl fmt::Debug for Sha512Ref<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The address describes identity only; no private C state is read.
        f.debug_struct("Sha512Ref")
            .field("ptr", &self.as_ptr())
            .finish()
    }
}

impl fmt::Debug for Sha512Mut<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_ref().fmt(f)
    }
}

// SAFETY: the typed constructor returns a uniquely owned, initialized context
// in av_malloc-family storage, exactly the allocation Heap's CDrop releases.
unsafe impl CNew<Sha512> for Heap {
    fn c_new(&self) -> Option<NonNull<Sha512>> {
        Sha512::new(Algorithm::Sha512).and_then(|owner| NonNull::new(owner.into_raw()))
    }
}

impl Sha512Mut<'_> {
    /// Wraps: av_sha512_init
    ///
    /// Start a new hash, discarding the previous state. The buffer need not be
    /// cleared: C resets the count and never reads stale bytes before filling
    /// the block. This may also change the digest variant.
    pub fn init(&mut self, algorithm: Algorithm) {
        // SAFETY: the handle grants exclusive access to a live full-sized
        // context, and the enum admits only sizes for which C returns zero.
        let status = unsafe { ffi::av_sha512_init(self.as_mut_ptr(), algorithm as i32) };
        debug_assert_eq!(status, 0, "supported digest initializer failed");
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn algorithm_lengths() {
        assert_eq!(Algorithm::Sha512224.digest_len(), 28);
        assert_eq!(Algorithm::Sha512256.digest_len(), 32);
        assert_eq!(Algorithm::Sha384.digest_len(), 48);
        assert_eq!(Algorithm::Sha512.digest_len(), 64);
    }

    #[test]
    fn pointer_layout_matches_the_ffi_seam() {
        assert_eq!(
            core::mem::size_of::<Sha512Box>(),
            core::mem::size_of::<*mut ()>()
        );
        assert_eq!(
            core::mem::size_of::<Option<Sha512Box>>(),
            core::mem::size_of::<*mut ()>()
        );
        assert_eq!(
            core::mem::size_of::<Sha512Ref<'_>>(),
            core::mem::size_of::<*mut ()>()
        );
        assert_eq!(
            core::mem::size_of::<Sha512Mut<'_>>(),
            core::mem::size_of::<*mut ()>()
        );
    }

    #[test]
    fn owner_raw_round_trip() {
        let owner = Sha512::new(Algorithm::Sha512).unwrap();
        let ptr = owner.into_c();
        // SAFETY: into_c transferred the sole initialized C allocation; no
        // handles exist, and Heap remains its matching release policy.
        let mut owner = unsafe { Sha512Box::from_c(ptr) }.unwrap();
        owner.as_mut().init(Algorithm::Sha512);
    }
}

/// Wraps: av_sha512_size
///
/// The C context's size in bytes, borrowed from immutable C static storage.
pub fn context_size() -> &'static i32 {
    // SAFETY: C defines this global as `const int = sizeof(context)`; it has
    // static lifetime and no writer anywhere in the library.
    unsafe { &ffi::av_sha512_size }
}
