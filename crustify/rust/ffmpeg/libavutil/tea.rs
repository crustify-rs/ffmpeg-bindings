//! Owned and borrowed views of the opaque TEA context.
//!
//! C keeps the layout private. The context contains only inline key words and
//! a round count; no key pointer is retained and there is no clone primitive.
//! Storage remains C-allocated and is released by the [`super::mem::Heap`] policy.

use ffibox::define_ctype;

use super::{ffi, mem::HeapBox};

define_ctype!(
    /// Wraps: AVTEA
    ///
    /// An initialized TEA context. Its private C fields are not exposed.
    /// No `Send`/`Sync` guarantee is added.
    ///
    /// Handles cannot outlive their owner:
    /// ```compile_fail
    /// use ffmpeg::libavutil::tea::{Tea, TeaRef};
    /// fn dangling() -> TeaRef<'static> {
    ///     let owner = Tea::new(&[0; 16], 64).unwrap();
    ///     owner.as_ref()
    /// }
    /// ```
    /// Exclusive handles cannot alias:
    /// ```compile_fail
    /// use ffmpeg::libavutil::tea::Tea;
    /// let mut owner = Tea::new(&[0; 16], 64).unwrap();
    /// let mut first = owner.as_mut();
    /// let mut second = owner.as_mut();
    /// first.init(&[1; 16], 64);
    /// second.init(&[2; 16], 64);
    /// ```
    Tea, TeaRef, TeaMut, ffi::AVTEA
);

/// Sole ownership of a C-allocated TEA context, freed with `av_free`.
pub type TeaBox = HeapBox<Tea>;

impl Tea {
    /// Wraps: av_tea_alloc
    ///
    /// Allocate and initialize a context; `None` means allocation failed.
    /// `rounds` is passed unchanged (64 is the standard round count).
    /// The key is copied, so its borrow ends when this call returns.
    pub fn new(key: &[u8; 16], rounds: i32) -> Option<TeaBox> {
        // SAFETY: the allocator returns fresh, zero-filled C storage with no
        // nested resources. Heap frees it with the paired av_free. The owner
        // stays private until init has established the context's key/rounds.
        let mut owner = unsafe { TeaBox::from_c(ffi::av_tea_alloc()) }?;
        owner.as_mut().init(key, rounds);
        Some(owner)
    }
}

impl TeaMut<'_> {
    /// Wraps: av_tea_init
    ///
    /// Replace the key and round count. C copies the 16 key bytes and retains
    /// no pointer to the key. All signed round counts are accepted by C.
    pub fn init(&mut self, key: &[u8; 16], rounds: i32) {
        // SAFETY: the handle exclusively borrows a full C allocation. C writes
        // only that context and reads exactly the 16 live key bytes, retaining
        // neither pointer. The scalar round count is stored without arithmetic.
        unsafe { ffi::av_tea_init(self.as_mut_ptr(), key.as_ptr(), rounds) }
    }
}

/// Wraps: av_tea_size
///
/// The C context's size in bytes, borrowed from immutable C static storage.
pub fn context_size() -> &'static i32 {
    // SAFETY: C defines this global as `const int = sizeof(context)`; it has
    // static lifetime and no writer anywhere in the library.
    unsafe { &ffi::av_tea_size }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn layout_matches_ffi() {
        assert_eq!(
            core::mem::size_of::<Tea>(),
            core::mem::size_of::<ffi::AVTEA>()
        );
        assert_eq!(
            core::mem::align_of::<Tea>(),
            core::mem::align_of::<ffi::AVTEA>()
        );
        assert_eq!(
            core::mem::size_of::<TeaRef<'_>>(),
            core::mem::size_of::<usize>()
        );
        assert_eq!(
            core::mem::size_of::<TeaMut<'_>>(),
            core::mem::size_of::<usize>()
        );
    }

    #[test]
    #[cfg(not(miri))]
    fn raw_ownership_round_trip() {
        let owner = Tea::new(&[7; 16], 64).unwrap();
        let ptr = owner.into_c();
        // SAFETY: into_c transferred the sole live allocation, still initialized
        // and paired with Heap's av_free; no borrow remains.
        let owner = unsafe { TeaBox::from_c(ptr) }.unwrap();
        drop(owner);
    }
}
