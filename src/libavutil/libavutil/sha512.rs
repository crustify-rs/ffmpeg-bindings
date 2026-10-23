//! Safe lifecycle and borrowed handles for `libavutil/sha512.h`.
//!
//! The body is private to `sha512.c`: this opaque layout is used only behind
//! C-allocated pointers, never as inline storage. There are no public fields,
//! sub-allocations, retained input pointers, reference counts, or C clone routines.
//! Owners release their single allocation through [`Sha512Free`]. No `Send` or `Sync`
//! implementation is asserted. Initialization, updates and finalization require
//! exclusive handles; output arrays cover every supported digest variant.

use core::fmt;
use core::ptr::NonNull;

use ffibox::{CBox, CDrop, CNew, define_ctype};

use super::ffi;
use super::mem::Heap;

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
/// use ffmpeg_bindings::libavutil::libavutil::sha512::{Algorithm, Sha512};
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
/// use ffmpeg_bindings::libavutil::libavutil::sha512::{Algorithm, Sha512};
/// let mut owner = Sha512::new(Algorithm::Sha512).unwrap();
/// let mut exclusive = owner.as_mut();
/// let shared = exclusive.as_ref();
/// exclusive.init(Algorithm::Sha512);
/// let _ = format!("{shared:?}");
/// ```
///
/// The opaque layout has no Rust size, so there is no Rust-sized allocation:
///
/// ```compile_fail,E0277
/// use ffmpeg_bindings::libavutil::libavutil::sha512::Sha512Box;
/// let _ = Sha512Box::new_uninit();
/// ```
pub type Sha512Box = CBox<Sha512, Sha512Free>;

/// Storage-only release of an `av_sha512_alloc` context. Unlike [`Heap`], it
/// offers no generic allocator: the opaque layout's Rust size is zero, so
/// only C can size a SHA-512 context.
#[derive(Clone, Copy, Debug, Default)]
pub struct Sha512Free;

/// Wraps: `av_free`
// SAFETY: Sha512Box adopts only `av_sha512_alloc` storage, which owns no sub-allocations;
// Heap's av_free releases that av_malloc-family block once without reading it.
unsafe impl CDrop<Sha512> for Sha512Free {
    unsafe fn c_drop(&self, ptr: NonNull<Sha512>) {
        // SAFETY: CDrop's caller transfers the unique av_malloc-family claim.
        unsafe { <Heap as CDrop<Sha512>>::c_drop(&Heap, ptr) }
    }
}

impl Sha512 {
    /// Wraps: `av_sha512_alloc`
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
        // ownership is unique and Sha512Free releases it through av_free once.
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
// in av_malloc-family storage, exactly the allocation Sha512Free releases.
unsafe impl CNew<Sha512> for Sha512Free {
    fn c_new(&self) -> Option<NonNull<Sha512>> {
        Sha512::new(Algorithm::Sha512).and_then(|owner| NonNull::new(owner.into_raw()))
    }
}

impl Sha512Mut<'_> {
    /// Wraps: `av_sha512_init`
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

/// Wraps: `av_sha512_size`
///
/// The C context's size in bytes, borrowed from immutable C static storage.
#[must_use]
pub fn context_size() -> &'static i32 {
    // SAFETY: C defines this global as `const int = sizeof(context)`; it has
    // static lifetime and no writer anywhere in the library.
    unsafe { &ffi::av_sha512_size }
}

impl Sha512Mut<'_> {
    /// Wraps: `av_sha512_update`
    ///
    /// Append input bytes. C copies them and retains no input pointer.
    pub fn update(&mut self, data: &[u8]) {
        // SAFETY: the handle exclusively borrows an initialized full C context;
        // nonempty data provides len readable bytes; empty input uses a real static
        // byte for C's zero-length memcpy. C retains no pointer.
        unsafe {
            ffi::av_sha512_update(
                self.as_mut_ptr(),
                if data.is_empty() {
                    c"".as_ptr().cast()
                } else {
                    data.as_ptr()
                },
                data.len(),
            );
        }
    }

    /// Wraps: `av_sha512_final`
    ///
    /// Finish the current message. The digest occupies the prefix of the returned
    /// array; bytes beyond the selected algorithm's digest length are zero.
    /// The maximum-size output remains safe even after changing the algorithm.
    /// Reinitialize the context before starting another message.
    pub fn finalize(&mut self) -> [u8; 64] {
        let mut digest = [0; 64];
        // SAFETY: exclusive initialized context; the output covers the largest
        // supported digest. C writes only the active digest and retains no pointer.
        unsafe { ffi::av_sha512_final(self.as_mut_ptr(), digest.as_mut_ptr()) };
        digest
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
        assert_eq!(size_of::<Sha512Box>(), size_of::<*mut ()>());
        assert_eq!(size_of::<Option<Sha512Box>>(), size_of::<*mut ()>());
        assert_eq!(size_of::<Sha512Ref<'_>>(), size_of::<*mut ()>());
        assert_eq!(size_of::<Sha512Mut<'_>>(), size_of::<*mut ()>());
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn owner_raw_round_trip() {
        let owner = Sha512::new(Algorithm::Sha512).unwrap();
        let ptr = owner.into_c();
        // SAFETY: into_c transferred the sole initialized C allocation; no
        // handles exist, and Sha512Free remains its matching release policy.
        let mut owner = unsafe { Sha512Box::from_c(ptr) }.unwrap();
        owner.as_mut().init(Algorithm::Sha512);
    }
}
