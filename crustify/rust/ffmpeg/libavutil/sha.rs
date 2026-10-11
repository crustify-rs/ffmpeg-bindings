//! Safe lifecycle and borrowed handles for `libavutil/sha.h`.
//!
//! The body is private to `sha.c`: this opaque layout is used only behind
//! C-allocated pointers, never as inline storage. There are no public fields,
//! sub-allocations, retained input pointers, reference counts, or C clone routines.
//! Owners release their single allocation through [`ShaFree`]. No `Send` or `Sync`
//! implementation is asserted. Hash update/final operations are left to the
//! symbol route; initialization is bound here to the object's lifecycle.

use core::fmt;
use core::ptr::NonNull;

use ffibox::{CBox, CDrop, CNew, define_ctype};

use super::ffi;
use super::mem::Heap;

/// A digest variant supported by this context's initializer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum Algorithm {
    /// SHA-1 (160-bit digest).
    Sha1 = 160,
    /// SHA-224 (224-bit digest).
    Sha224 = 224,
    /// SHA-256 (256-bit digest).
    Sha256 = 256,
}

impl Algorithm {
    /// The digest length in bytes.
    #[must_use]
    pub const fn digest_len(self) -> usize {
        self as usize / 8
    }
}

define_ctype!(
    /// Wraps: AVSHA
    ///
    /// An opaque, initialized C hash context. Construct through [`Self::new`]
    /// or [`ShaBox::new`], not by allocating the opaque layout's Rust size.
    Sha,
    ShaRef,
    ShaMut,
    ffi::AVSHA
);

/// A uniquely owned, initialized context, freed by `av_free`.
///
/// The zero-argument `new()` selects SHA-256; use the layout type's
/// `new(algorithm)` to select a different variant.
///
/// Borrows cannot outlive their owner:
///
/// ```compile_fail
/// use ffmpeg::libavutil::sha::{Algorithm, Sha};
/// let view = {
///     let owner = Sha::new(Algorithm::Sha256).unwrap();
///     owner.as_ref()
/// };
/// let _ = format!("{view:?}");
/// ```
///
/// A shared reborrow freezes the exclusive handle:
///
/// ```compile_fail
/// use ffmpeg::libavutil::sha::{Algorithm, Sha};
/// let mut owner = Sha::new(Algorithm::Sha256).unwrap();
/// let mut exclusive = owner.as_mut();
/// let shared = exclusive.as_ref();
/// exclusive.init(Algorithm::Sha256);
/// let _ = format!("{shared:?}");
/// ```
///
/// The opaque layout has no Rust size, so there is no Rust-sized allocation:
///
/// ```compile_fail,E0277
/// use ffmpeg::libavutil::sha::ShaBox;
/// let _ = ShaBox::new_uninit();
/// ```
pub type ShaBox = CBox<Sha, ShaFree>;

/// Storage-only release of an `av_sha_alloc` context. Unlike [`Heap`], it
/// offers no generic allocator: the opaque layout's Rust size is zero, so
/// only C can size a SHA context.
#[derive(Clone, Copy, Debug, Default)]
pub struct ShaFree;

/// Wraps: av_free
// SAFETY: ShaBox adopts only `av_sha_alloc` storage, which owns no sub-allocations;
// Heap's av_free releases that av_malloc-family block once without reading it.
unsafe impl CDrop<Sha> for ShaFree {
    unsafe fn c_drop(&self, ptr: NonNull<Sha>) {
        // SAFETY: CDrop's caller transfers the unique av_malloc-family claim.
        unsafe { <Heap as CDrop<Sha>>::c_drop(&Heap, ptr) }
    }
}

impl Sha {
    /// Wraps: av_sha_alloc
    ///
    /// Allocate and initialize a context for `algorithm`; `None` on allocation
    /// failure. Invalid digest sizes cannot be represented by [`Algorithm`].
    #[must_use]
    pub fn new(algorithm: Algorithm) -> Option<ShaBox> {
        // SAFETY: the allocator has no arguments or preconditions and returns
        // a fresh, zero-filled, full-sized context, or NULL.
        let ptr = unsafe { ffi::av_sha_alloc() };
        if ptr.is_null() {
            return None;
        }
        // SAFETY: `ptr` uniquely owns full-sized C storage. Every enum value
        // selects a supported initializer; C stores no borrowed pointers.
        let status = unsafe { ffi::av_sha_init(ptr, algorithm as i32) };
        if status != 0 {
            // SAFETY: this is the fresh av_mallocz allocation, not yet adopted
            // or borrowed; av_free never reads the object.
            unsafe { ffi::av_free(ptr.cast()) };
            return None;
        }
        // SAFETY: C initialized the context in its own full-sized allocation;
        // ownership is unique and ShaFree releases it through av_free once.
        unsafe { ShaBox::from_c(ptr) }
    }
}

impl fmt::Debug for ShaRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The address describes identity only; no private C state is read.
        f.debug_struct("ShaRef")
            .field("ptr", &self.as_ptr())
            .finish()
    }
}

impl fmt::Debug for ShaMut<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_ref().fmt(f)
    }
}

// SAFETY: the typed constructor returns a uniquely owned, initialized context
// in av_malloc-family storage, exactly the allocation ShaFree releases.
unsafe impl CNew<Sha> for ShaFree {
    fn c_new(&self) -> Option<NonNull<Sha>> {
        Sha::new(Algorithm::Sha256).and_then(|owner| NonNull::new(owner.into_raw()))
    }
}

impl ShaMut<'_> {
    /// Wraps: av_sha_init
    ///
    /// Start a new hash, discarding the previous state. The buffer need not be
    /// cleared: C resets the count and never reads stale bytes before filling
    /// the block. This may also change the digest variant.
    pub fn init(&mut self, algorithm: Algorithm) {
        // SAFETY: the handle grants exclusive access to a live full-sized
        // context, and the enum admits only sizes for which C returns zero.
        let status = unsafe { ffi::av_sha_init(self.as_mut_ptr(), algorithm as i32) };
        debug_assert_eq!(status, 0, "supported digest initializer failed");
    }
}

/// Wraps: av_sha_size
///
/// The C context's size in bytes, borrowed from immutable C static storage.
pub fn context_size() -> &'static i32 {
    // SAFETY: C defines this global as `const int = sizeof(context)`; it has
    // static lifetime and no writer anywhere in the library.
    unsafe { &ffi::av_sha_size }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn algorithm_lengths() {
        assert_eq!(Algorithm::Sha1.digest_len(), 20);
        assert_eq!(Algorithm::Sha224.digest_len(), 28);
        assert_eq!(Algorithm::Sha256.digest_len(), 32);
    }

    #[test]
    fn pointer_layout_matches_the_ffi_seam() {
        assert_eq!(
            core::mem::size_of::<ShaBox>(),
            core::mem::size_of::<*mut ()>()
        );
        assert_eq!(
            core::mem::size_of::<Option<ShaBox>>(),
            core::mem::size_of::<*mut ()>()
        );
        assert_eq!(
            core::mem::size_of::<ShaRef<'_>>(),
            core::mem::size_of::<*mut ()>()
        );
        assert_eq!(
            core::mem::size_of::<ShaMut<'_>>(),
            core::mem::size_of::<*mut ()>()
        );
    }

    #[test]
    fn owner_raw_round_trip() {
        let owner = Sha::new(Algorithm::Sha256).unwrap();
        let ptr = owner.into_c();
        // SAFETY: into_c transferred the sole initialized C allocation; no
        // handles exist, and ShaFree remains its matching release policy.
        let mut owner = unsafe { ShaBox::from_c(ptr) }.unwrap();
        owner.as_mut().init(Algorithm::Sha256);
    }
}
