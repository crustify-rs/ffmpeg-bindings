//! Owned and borrowed contexts for `libavutil/hash.h`.
//!
//! C keeps the representation private. A context owns its selected algorithm's
//! storage; CRC32 instead borrows an immutable static table, and adler32 needs
//! no child allocation. C has no clone, refcount, or field-disposal primitive.

use core::ffi::{CStr, c_int};
use core::ptr;

use ffibox::{CBox, define_ctype, impl_cdrop};

use super::ffi;

define_ctype!(
    /// Wraps: AVHashContext
    ///
    /// A C hash context, owned as a [`HashContextBox`]. The public C API is
    /// opaque, so there are no field accessors or inline Rust-owned values.
    /// Handles require an allocated context with a valid algorithm; shared
    /// operations only read metadata and do not inspect the hashing state.
    HashContext,
    HashContextRef,
    HashContextMut,
    ffi::AVHashContext
);

/// Wraps: av_hash_freep
///
/// Releases the context and its algorithm storage exactly once. The CRC
/// table is static and is not freed.
#[derive(Clone, Copy, Debug, Default)]
pub struct HashContextFree;

/// Adapts the C pointer-to-pointer release to ffibox's sole-owner policy.
///
/// # Safety
/// `context` must be the sole live allocation returned by `av_hash_alloc`,
/// with no outstanding borrows, and must not be released again.
unsafe fn free_context(context: *mut ffi::AVHashContext) {
    let mut slot = context;
    // SAFETY: the policy receives the uniquely owned allocation from
    // av_hash_alloc. C frees its child and then the context, and clears only
    // this writable local slot; it does not retain the slot's address.
    unsafe { ffi::av_hash_freep(&mut slot) };
}

// SAFETY (of the binding): free_context releases precisely one av_hash_alloc
// allocation and its children, matching every successful constructor branch.
impl_cdrop!(HashContextFree, HashContext, free_context);

/// The sole owner of a C hash context. Not `Clone`: copying its private bytes
/// would duplicate child ownership. Thread traits are intentionally withheld.
///
/// A handle cannot outlive its owner:
/// ```compile_fail
/// use ffmpeg::libavutil::hash::HashContext;
/// let view = {
///     let hash = HashContext::new(c"MD5").unwrap();
///     hash.as_ref()
/// };
/// let _ = view.size();
/// ```
/// Shared and exclusive handles cannot alias:
/// ```compile_fail
/// use ffmpeg::libavutil::hash::HashContext;
/// let mut hash = HashContext::new(c"MD5").unwrap();
/// let shared = hash.as_ref();
/// hash.as_mut().init();
/// let _ = shared.size();
/// ```
/// An exclusive reborrow freezes its parent handle:
/// ```compile_fail
/// use ffmpeg::libavutil::hash::HashContext;
/// let mut hash = HashContext::new(c"MD5").unwrap();
/// let mut parent = hash.as_mut();
/// let mut child = parent.as_mut();
/// parent.init();
/// child.init();
/// ```
pub type HashContextBox = CBox<HashContext, HashContextFree>;

impl HashContext {
    /// Wraps: av_hash_alloc
    ///
    /// Allocates and initializes the named algorithm (case-insensitive).
    /// Returns C's negative error code for an unknown algorithm or allocation
    /// failure. The name is read only during allocation, never retained.
    #[allow(clippy::new_ret_no_self)]
    pub fn new(name: &CStr) -> Result<HashContextBox, c_int> {
        let mut context = ptr::null_mut();
        // SAFETY: context is a writable local output slot; name is a live
        // NUL-terminated string. C reads the name, retains no input pointer,
        // and hands over a fresh allocation on success, NULL on failure.
        let result = unsafe { ffi::av_hash_alloc(&mut context, name.as_ptr()) };
        if result < 0 {
            return Err(result);
        }
        // SAFETY: successful av_hash_alloc produced a uniquely owned context
        // with a valid algorithm and children, releasable by av_hash_freep.
        // The policy may release it even before hashing state is initialized.
        let mut owner = unsafe { HashContextBox::from_c(context) }
            .expect("av_hash_alloc succeeded without an allocation");
        owner.as_mut().init();
        Ok(owner)
    }
}

impl HashContextRef<'_> {
    /// Wraps: av_hash_get_name
    ///
    /// The algorithm's canonical name, backed by C's immutable static table.
    pub fn name(&self) -> &'static CStr {
        // SAFETY: this handle keeps a valid allocated context alive. C only
        // reads its algorithm and returns a non-NULL NUL-terminated string
        // in static const hashdesc storage, independent of the context's life.
        unsafe { CStr::from_ptr(ffi::av_hash_get_name(self.as_ptr())) }
    }

    /// Wraps: av_hash_get_size
    ///
    /// The number of bytes in this algorithm's binary digest.
    pub fn size(&self) -> usize {
        // SAFETY: the live allocated context has an algorithm selected by
        // av_hash_alloc. C only reads it and returns its positive digest size.
        unsafe { ffi::av_hash_get_size(self.as_ptr()) as usize }
    }
}

impl HashContextMut<'_> {
    /// Wraps: av_hash_init
    ///
    /// Initializes or resets the hashing state, including after finalization.
    pub fn init(&mut self) {
        // SAFETY: this exclusive handle keeps an allocated context and its
        // children alive, excludes competing access, and C initializes all
        // hashing state for its valid algorithm. No pointer is retained.
        unsafe { ffi::av_hash_init(self.as_mut_ptr()) };
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn unknown_names_are_errors() {
        for name in [c"", c"not a hash", c"SHA999", c"\xff"] {
            assert_eq!(HashContext::new(name).err(), Some(-(ffi::EINVAL as c_int)));
        }
    }

    #[test]
    fn owner_round_trip_and_null_adoption() {
        // SAFETY: NULL adopts no allocation and must return None.
        assert!(unsafe { HashContextBox::from_c(ptr::null_mut()) }.is_none());
        let owner = HashContext::new(c"MD5").unwrap();
        let raw = owner.into_c();
        // SAFETY: into_c transferred this sole live allocation; no handles
        // remain, and the same release policy adopts it exactly once.
        let mut owner = unsafe { HashContextBox::from_c(raw) }.unwrap();
        owner.as_mut().init();
        assert_eq!(owner.as_ref().name(), c"MD5");
    }
}

/// Wraps: av_hash_names
///
/// Name at a C hash registry index, or `None` for an invalid index.
/// The registry and its NUL-terminated names are immutable static storage.
pub fn name(index: i32) -> Option<&'static CStr> {
    // SAFETY: every integer index is checked by C; no input pointers.
    let ptr = unsafe { ffi::av_hash_names(index) };
    if ptr.is_null() {
        None
    } else {
        // SAFETY: the non-null result points to an immutable, NUL-terminated
        // name in hash.c's static hashdesc array; it is never freed.
        Some(unsafe { CStr::from_ptr(ptr) })
    }
}
