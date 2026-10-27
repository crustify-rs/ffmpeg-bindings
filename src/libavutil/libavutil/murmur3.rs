//! Safe ownership and borrowing of `libavutil/murmur3.h`'s opaque context.
//!
//! Implements C's 128-bit `MurmurHash3` variant for 64-bit platforms. This is a
//! non-cryptographic hash. Context storage and private layout remain C-owned.

use core::ptr::NonNull;

use ffibox::{CBox, CDrop, CMutSeam, CNew, define_ctype};

use super::ffi;

define_ctype!(
    /// Wraps: AVMurMur3
    ///
    /// Opaque MurmurHash3 context. No private fields are exposed by C's API.
    Murmur3,
    Murmur3Ref,
    Murmur3Mut,
    ffi::AVMurMur3
);

/// Allocation and release policy for an initialized `MurmurHash3` context.
#[derive(Clone, Copy, Debug, Default)]
pub struct Murmur3Free;

/// Sole owner of a `MurmurHash3` context, released through `av_free`.
///
/// The opaque layout has no Rust size, so there is no Rust-sized allocation:
///
/// ```compile_fail,E0277
/// use ffmpeg_bindings::libavutil::libavutil::murmur3::Murmur3Box;
/// let _ = Murmur3Box::new_uninit();
/// ```
pub type Murmur3Box = CBox<Murmur3, Murmur3Free>;

/// Wraps: `av_free`
// SAFETY: the owner hands this policy one uniquely owned av_malloc-family
// context allocation. The context has no child resources; av_free releases
// the whole allocation without reading its state.
unsafe impl CDrop<Murmur3> for Murmur3Free {
    unsafe fn c_drop(&self, ptr: NonNull<Murmur3>) {
        // SAFETY: the caller transfers the live context allocation to this
        // release policy exactly once. av_free accepts its erased pointer.
        unsafe { ffi::av_free(ptr.as_ptr().cast()) }
    }
}

/// Wraps: `av_murmur3_alloc`
// SAFETY: allocation uses C's full private size, init establishes all state
// invariants, and the paired release policy calls av_free exactly once.
unsafe impl CNew<Murmur3> for Murmur3Free {
    fn c_new(&self) -> Option<NonNull<Murmur3>> {
        // SAFETY: allocation has no preconditions and returns a fresh C context
        // or NULL, which is rejected before attempting initialization.
        let ptr = NonNull::new(unsafe { ffi::av_murmur3_alloc() })?;
        // SAFETY: a fresh, uniquely owned, correctly sized C context.
        unsafe { ffi::av_murmur3_init(ptr.as_ptr()) };
        Some(ptr.cast())
    }
}

impl Murmur3 {
    /// Allocate and initialize with C's fixed seed, or `None` on allocation failure.
    #[must_use]
    pub fn new() -> Option<Murmur3Box> {
        Murmur3Box::new()
    }

    /// Allocate and initialize with a caller-selected seed.
    #[must_use]
    pub fn with_seed(seed: u64) -> Option<Murmur3Box> {
        let mut context = Self::new()?;
        context.as_mut().reset_seeded(seed);
        Some(context)
    }
}

impl Murmur3Mut<'_> {
    /// Wraps: `av_murmur3_init`
    ///
    /// Restart hashing with C's fixed seed, discarding all previous state.
    pub fn reset(&mut self) {
        // SAFETY: the exclusive handle keeps the full C context live; init
        // initializes in place and neither reads nor retains previous input.
        unsafe { ffi::av_murmur3_init(self.as_mut_ptr()) }
    }

    /// Wraps: `av_murmur3_init_seeded`
    ///
    /// Restart hashing with the given seed, discarding all previous state.
    pub fn reset_seeded(&mut self, seed: u64) {
        // SAFETY: exclusive access to the full context allocation. Every u64
        // seed is accepted and init initializes all context fields in place.
        unsafe { ffi::av_murmur3_init_seeded(self.as_mut_ptr(), seed) }
    }

    /// Wraps: `av_murmur3_update`
    ///
    /// Append bytes, borrowed only for the duration of the C call.
    pub fn update(&mut self, data: &[u8]) {
        // SAFETY: exclusive initialized context and len readable input bytes;
        // C does not mutate or retain the input and accepts empty slices.
        unsafe { ffi::av_murmur3_update(self.as_mut_ptr(), data.as_ptr(), data.len()) }
    }

    /// Wraps: `av_murmur3_final`
    ///
    /// Output the 16 digest bytes in C's little-endian order. This zero-pads
    /// the partial block as C does; use [`Self::reset`] or [`Self::reset_seeded`]
    /// before starting a new message.
    pub fn finalize(&mut self) -> [u8; 16] {
        let mut digest = [0; 16];
        // SAFETY: exclusive initialized context and exactly 16 writable bytes;
        // C writes every output byte and retains neither pointer.
        unsafe { ffi::av_murmur3_final(self.as_mut_ptr(), digest.as_mut_ptr()) };
        digest
    }
}

#[cfg(test)]
mod unit_tests {
    use ffibox::CRefSeam;

    use super::*;

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn allocation_failure() {
        const FLAG: &str = "CRUSTIFY_MURMUR3_FAILURE_CHILD";
        if std::env::var_os(FLAG).is_some() {
            // SAFETY: this isolated child runs only this exact test, before any
            // FFmpeg allocation, so no concurrent C allocator reads or writes
            // the process-global limit. Rust's allocator does not use this limit.
            unsafe { ffi::av_max_alloc(0) };
            assert!(Murmur3::new().is_none());
            assert!(Murmur3::with_seed(u64::MAX).is_none());
            return;
        }
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "libavutil::murmur3::unit_tests::allocation_failure",
                "--test-threads=1",
            ])
            .env(FLAG, "1")
            .status()
            .unwrap();
        assert!(status.success());
    }

    #[test]
    fn layout_and_handle_sizes() {
        use core::mem::{align_of, size_of};
        assert_eq!(size_of::<Murmur3>(), size_of::<ffi::AVMurMur3>());
        assert_eq!(align_of::<Murmur3>(), align_of::<ffi::AVMurMur3>());
        assert_eq!(size_of::<Murmur3Ref<'_>>(), size_of::<usize>());
        assert_eq!(size_of::<Murmur3Mut<'_>>(), size_of::<usize>());
        assert_eq!(size_of::<Option<Murmur3Box>>(), size_of::<usize>());
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn owner_round_trip_and_null_adoption() {
        let owner = Murmur3::with_seed(0).unwrap();
        let ptr = owner.into_c();
        // SAFETY: into_c handed us the sole initialized C allocation, with no
        // surviving handles. Adoption restores its matching release policy.
        let mut owner = unsafe { Murmur3Box::from_c(ptr) }.unwrap();
        assert_eq!(owner.as_mut().finalize(), [0; 16]);
        // SAFETY: NULL is rejected without accessing storage or adopting it.
        assert!(unsafe { Murmur3Box::from_c(core::ptr::null_mut()) }.is_none());
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn borrowed_raw_seams_preserve_identity() {
        let mut owner = Murmur3::new().unwrap();
        let shared = owner.as_ref();
        // SAFETY: owner keeps the context alive and only shared handles exist.
        let borrowed = unsafe { Murmur3Ref::from_ptr(shared.as_ptr().cast_mut()) }.unwrap();
        assert_eq!(borrowed.as_ptr(), shared.as_ptr());
        let mut exclusive = owner.as_mut();
        // SAFETY: exclusive is unused until this shorter exclusive borrow ends;
        // the owner keeps initialized C storage alive throughout.
        let mut borrowed = unsafe { Murmur3Mut::from_ptr(exclusive.as_mut_ptr()) }.unwrap();
        borrowed.update(b"seam");
        borrowed.reset_seeded(0);
        assert_eq!(borrowed.finalize(), [0; 16]);
    }
}
